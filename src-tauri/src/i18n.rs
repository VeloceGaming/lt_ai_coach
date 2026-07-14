//! Runtime UI translations. Packaged `<language>/base.json` files beside the
//! executable are read-only defaults. User translations and generated
//! `<language>/mod.json` files live under local app data so Workshop updates
//! cannot overwrite them. English UI text remains bundled in the frontend.

use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TranslationMeta {
    id: String,
    name: String,
    direction: String,
}

#[derive(Serialize)]
pub struct TranslationList {
    languages: Vec<TranslationMeta>,
    warnings: Vec<String>,
}

fn bundled_translations_dir() -> Option<PathBuf> {
    // During development, read the version-controlled translations in the
    // source checkout. Release builds use only the folder beside the executable.
    #[cfg(debug_assertions)]
    if let Some(project_dir) = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("translations"))
        .filter(|dir| dir.is_dir())
    {
        return Some(project_dir);
    }

    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("translations")))
        .filter(|dir| dir.is_dir())
}

pub fn user_translations_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("translations");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create the translations folder: {error}"))?;
    Ok(dir)
}

fn translation_meta(path: &Path, id: &str, label: &str) -> Result<TranslationMeta, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("Could not read {label}: {error}"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{label} is not valid JSON: {error}"))?;
    let meta = value.get("$meta");
    Ok(TranslationMeta {
        id: id.to_string(),
        name: meta
            .and_then(|entry| entry.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        direction: meta
            .and_then(|entry| entry.get("direction"))
            .and_then(Value::as_str)
            .unwrap_or("ltr")
            .to_string(),
    })
}

fn scan_translation_root(
    root: &Path,
    source: &str,
    languages: &mut BTreeMap<String, TranslationMeta>,
    warnings: &mut Vec<String>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("Could not read {source} translations: {error}"));
            return;
        }
    };
    for entry in entries.flatten() {
        let language_dir = entry.path();
        if !language_dir.is_dir() {
            continue;
        }
        let Some(id) = language_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(canonical_language_id)
        else {
            continue;
        };
        if id.eq_ignore_ascii_case("en") {
            continue;
        }
        let path = language_dir.join("base.json");
        if !path.is_file() {
            continue;
        }
        let label = format!("{source} {id}/base.json");
        match translation_meta(&path, &id, &label) {
            Ok(meta) => {
                languages.insert(id, meta);
            }
            Err(error) => warnings.push(error),
        }
    }
}

#[tauri::command]
pub fn list_translations(app: tauri::AppHandle) -> Result<TranslationList, String> {
    let user_dir = user_translations_dir(&app)?;
    let mut languages = BTreeMap::new();
    let mut warnings = Vec::new();
    if let Some(bundled_dir) = bundled_translations_dir() {
        scan_translation_root(&bundled_dir, "packaged", &mut languages, &mut warnings);
    }
    // User metadata intentionally wins when both layers provide the language.
    scan_translation_root(&user_dir, "user", &mut languages, &mut warnings);
    let mut out: Vec<_> = languages.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(TranslationList {
        languages: out,
        warnings,
    })
}

pub fn canonical_language_id(id: &str) -> String {
    match id.to_ascii_lowercase().as_str() {
        "zh-tw" | "zh-hant" => "zh-hant".to_string(),
        "zh-cn" | "zh-hans" => "zh-hans".to_string(),
        _ => id.to_string(),
    }
}

fn merge_translation_file(
    path: &std::path::Path,
    label: &str,
    out: &mut HashMap<String, String>,
) -> Result<(), String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("Could not read {label}: {error}"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{label} is not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label} must contain a JSON object"))?;
    for (key, entry) in object {
        if key != "$meta" {
            if let Some(text) = entry.as_str() {
                out.insert(key.clone(), text.to_string());
            }
        }
    }
    Ok(())
}

fn load_translation_from_roots(
    bundled_dir: Option<&Path>,
    user_dir: &Path,
    id: &str,
) -> Result<HashMap<String, String>, String> {
    let bundled_base = bundled_dir.map(|dir| dir.join(id).join("base.json"));
    let user_base = user_dir.join(id).join("base.json");
    let user_mod = user_dir.join(id).join("mod.json");
    let has_base = bundled_base.as_ref().is_some_and(|path| path.is_file()) || user_base.is_file();
    if id != "en" && !has_base {
        return Err(format!(
            "Could not read {id}/base.json: file does not exist"
        ));
    }

    let mut out = HashMap::new();
    if let Some(path) = bundled_base.filter(|path| path.is_file()) {
        merge_translation_file(&path, &format!("packaged {id}/base.json"), &mut out)?;
    }
    if user_base.is_file() {
        merge_translation_file(&user_base, &format!("user {id}/base.json"), &mut out)?;
    }
    if user_mod.is_file() {
        merge_translation_file(&user_mod, &format!("user {id}/mod.json"), &mut out)?;
    }
    Ok(out)
}

#[tauri::command]
pub fn load_translation(
    app: tauri::AppHandle,
    id: String,
) -> Result<HashMap<String, String>, String> {
    let id = canonical_language_id(&id);
    let user_dir = user_translations_dir(&app)?;
    let bundled_dir = bundled_translations_dir();
    load_translation_from_roots(bundled_dir.as_deref(), &user_dir, &id)
}

// Writes every known translation key with its current (English) text so a
// translator can copy the file and replace the values. Refuses to overwrite an
// existing template rather than risk clobbering in-progress work.
#[tauri::command]
pub fn export_translation_template(
    app: tauri::AppHandle,
    entries: HashMap<String, String>,
) -> Result<String, String> {
    let dir = user_translations_dir(&app)?;
    let template_dir = dir.join("translation-template");
    let path = template_dir.join("base.json");
    if template_dir.exists() {
        return Err(
            "The translation-template folder already exists. Rename or move it before exporting a new one.".to_string(),
        );
    }
    fs::create_dir_all(&template_dir)
        .map_err(|error| format!("Could not create the translation template folder: {error}"))?;
    let sorted: std::collections::BTreeMap<String, String> = entries.into_iter().collect();
    let mut root = serde_json::Map::new();
    root.insert(
        "$meta".to_string(),
        serde_json::json!({ "name": "New Language", "direction": "ltr" }),
    );
    for (key, value) in sorted {
        root.insert(key, Value::String(value));
    }
    let text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|error| format!("Could not build the template: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("Could not write the template: {error}"))?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn open_translations_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = user_translations_dir(&app)?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|error| format!("Could not open the folder: {error}"))?;
    }
    #[cfg(not(windows))]
    {
        let _ = &dir;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{canonical_language_id, load_translation_from_roots};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lt-ai-coach-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn canonicalizes_game_and_legacy_chinese_language_ids() {
        assert_eq!(canonical_language_id("zh-TW"), "zh-hant");
        assert_eq!(canonical_language_id("zh-hant"), "zh-hant");
        assert_eq!(canonical_language_id("zh-CN"), "zh-hans");
        assert_eq!(canonical_language_id("ja"), "ja");
    }

    #[test]
    fn user_files_override_packaged_translations_and_mod_names_load_last() {
        let root = temp_root("translation-layers");
        let bundled = root.join("bundled");
        let user = root.join("user");
        fs::create_dir_all(bundled.join("fr")).unwrap();
        fs::create_dir_all(user.join("fr")).unwrap();
        fs::write(
            bundled.join("fr/base.json"),
            r#"{"$meta":{"name":"Français"},"shared":"packaged","packaged.only":"yes"}"#,
        )
        .unwrap();
        fs::write(
            user.join("fr/base.json"),
            r#"{"$meta":{"name":"Français personnalisé"},"shared":"user","user.only":"yes"}"#,
        )
        .unwrap();
        fs::write(
            user.join("fr/mod.json"),
            r#"{"shared":"mod","champion.custom":"Champion"}"#,
        )
        .unwrap();

        let loaded = load_translation_from_roots(Some(&bundled), &user, "fr").unwrap();
        assert_eq!(loaded["packaged.only"], "yes");
        assert_eq!(loaded["user.only"], "yes");
        assert_eq!(loaded["shared"], "mod");
        assert_eq!(loaded["champion.custom"], "Champion");
        fs::remove_dir_all(root).unwrap();
    }
}

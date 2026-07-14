//! Runtime UI translations. Each community language has its own directory in
//! the project's `translations` folder: `<language>/base.json` is maintained by
//! the translator, while portrait repair owns `mod.json` in the same directory.
//! English UI text remains bundled in the frontend, but `translations/en/mod.json`
//! may still provide names discovered from enabled champion mods.

use serde::Serialize;
use serde_json::Value;
use std::{collections::HashMap, fs, path::PathBuf};
use tauri::Manager;

#[derive(Serialize)]
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

pub fn translations_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    // 1. Source checkout: keep community translations beside the project so they
    //    can be edited and versioned normally. This wins during development.
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("translations"));
    if let Some(dir) = project_dir.filter(|dir| dir.is_dir()) {
        return Ok(dir);
    }

    // 2. Packaged app: a `translations` folder shipped next to the executable.
    //    This is what installed users get. It is visible and editable, so people
    //    can update a language or drop in a new one without a rebuild.
    if let Some(beside_exe) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("translations")))
        .filter(|dir| dir.is_dir())
    {
        return Ok(beside_exe);
    }

    // 3. Fallback: writable per-user app data. Used when nothing is shipped
    //    beside the exe (English still works; other languages are simply absent).
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("translations");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create the translations folder: {error}"))?;
    Ok(dir)
}

#[tauri::command]
pub fn list_translations(app: tauri::AppHandle) -> Result<TranslationList, String> {
    let dir = translations_dir(&app)?;
    let entries = fs::read_dir(&dir)
        .map_err(|error| format!("Could not read the translations folder: {error}"))?;
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for entry in entries.flatten() {
        let language_dir = entry.path();
        if !language_dir.is_dir() {
            continue;
        }
        let Some(id) = language_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if id.eq_ignore_ascii_case("en") {
            continue;
        }
        let path = language_dir.join("base.json");
        if !path.is_file() {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                warnings.push(format!("Could not read {id}/base.json: {error}"));
                continue;
            }
        };
        let value = match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("{id}/base.json is not valid JSON: {error}"));
                continue;
            }
        };
        let meta = value.get("$meta");
        let name = meta
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or(id)
            .to_string();
        let direction = meta
            .and_then(|m| m.get("direction"))
            .and_then(|d| d.as_str())
            .unwrap_or("ltr")
            .to_string();
        out.push(TranslationMeta {
            id: id.to_string(),
            name,
            direction,
        });
    }
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
    let text = fs::read_to_string(path).map_err(|error| format!("Could not read {label}: {error}"))?;
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

#[tauri::command]
pub fn load_translation(
    app: tauri::AppHandle,
    id: String,
) -> Result<HashMap<String, String>, String> {
    let dir = translations_dir(&app)?;
    let id = canonical_language_id(&id);
    let language_dir = dir.join(&id);
    let base_path = language_dir.join("base.json");
    let mod_path = language_dir.join("mod.json");
    let mut out = HashMap::new();

    if base_path.is_file() {
        merge_translation_file(&base_path, &format!("{id}/base.json"), &mut out)?;
    } else if id != "en" {
        return Err(format!("Could not read {id}/base.json: file does not exist"));
    }
    if mod_path.is_file() {
        merge_translation_file(&mod_path, &format!("{id}/mod.json"), &mut out)?;
    }
    Ok(out)
}

// Writes every known translation key with its current (English) text so a
// translator can copy the file and replace the values. Refuses to overwrite an
// existing template rather than risk clobbering in-progress work.
#[tauri::command]
pub fn export_translation_template(
    app: tauri::AppHandle,
    entries: HashMap<String, String>,
) -> Result<String, String> {
    let dir = translations_dir(&app)?;
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
    let dir = translations_dir(&app)?;
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
    use super::canonical_language_id;

    #[test]
    fn canonicalizes_game_and_legacy_chinese_language_ids() {
        assert_eq!(canonical_language_id("zh-TW"), "zh-hant");
        assert_eq!(canonical_language_id("zh-hant"), "zh-hant");
        assert_eq!(canonical_language_id("zh-CN"), "zh-hans");
        assert_eq!(canonical_language_id("ja"), "ja");
    }
}

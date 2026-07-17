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

// Keeps a translator's base.json complete as English gains new UI keys. Only
// absent entries are inserted; existing values (translated or otherwise) are
// never replaced. The line-based insertion preserves the translator's layout
// and keeps the keys in the same order as the built-in English dictionary.
fn add_missing_fallback_entries(
    path: &Path,
    label: &str,
    fallback_entries: &[(String, String)],
) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Could not read {label}: {error}"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{label} is not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label} must contain a JSON object"))?;
    let fallback_by_key: HashMap<&str, &str> = fallback_entries
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let had_final_newline = text.ends_with('\n');
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();

    // Older repair builds appended fallbacks after language-specific keys such
    // as champion names. Move only English-valued fallback entries back into
    // the canonical UI-key order; translated values stay exactly where they are.
    let first_extra = top_level_entry_lines(&lines)
        .into_iter()
        .find_map(|(start, _, key)| {
            (key != "$meta" && !fallback_by_key.contains_key(key.as_str())).then_some(start)
        });
    if let Some(first_extra) = first_extra {
        let top_level_keys: HashMap<usize, String> =
            top_level_entry_lines(&lines)
                .into_iter()
                .map(|(start, _, key)| (start, key))
                .collect();
        lines = lines
            .into_iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let should_move = index > first_extra
                    && top_level_keys.get(&index).is_some_and(|key| {
                        fallback_by_key.get(key.as_str()).is_some_and(|fallback| {
                            object.get(key).and_then(Value::as_str) == Some(*fallback)
                        })
                    });
                (!should_move).then_some(line)
            })
            .collect();
    }

    for (fallback_index, (key, fallback)) in fallback_entries.iter().enumerate() {
        let entries = top_level_entry_lines(&lines);
        if entries.iter().any(|(_, _, entry)| entry == key) {
            continue;
        }
        let next_key = fallback_entries[fallback_index + 1..]
            .iter()
            .map(|(next, _)| next)
            .find(|next| entries.iter().any(|(_, _, entry)| entry == *next));
        let mut insertion = next_key
            .and_then(|next| {
                entries
                    .iter()
                    .find_map(|(start, _, entry)| (entry == next).then_some(*start))
            })
            .or_else(|| {
                entries.iter().find_map(|(start, _, entry)| {
                    (entry != "$meta" && !fallback_by_key.contains_key(entry.as_str()))
                        .then_some(*start)
                })
            })
            .or_else(|| lines.iter().rposition(|line| line.trim() == "}"))
            .ok_or_else(|| format!("{label} must contain a JSON object"))?;
        if next_key.is_none() {
            while insertion > 0 && lines[insertion - 1].trim().is_empty() {
                insertion -= 1;
            }
        }
        lines.insert(
            insertion,
            format!(
                "  {}: {},",
                serde_json::to_string(key).expect("translation key is serializable"),
                serde_json::to_string(fallback).expect("fallback text is serializable")
            ),
        );
    }

    let entry_lines: Vec<usize> = top_level_entry_lines(&lines)
        .into_iter()
        .map(|(_, end, _)| end)
        .collect();
    for (position, index) in entry_lines.iter().enumerate() {
        let trimmed = lines[*index].trim_end_matches(',').to_string();
        lines[*index] = if position + 1 == entry_lines.len() {
            trimmed
        } else {
            format!("{trimmed},")
        };
    }
    let mut repaired = lines.join(newline);
    if had_final_newline {
        repaired.push_str(newline);
    }
    if repaired == text {
        return Ok(());
    }
    fs::write(path, repaired)
        .map_err(|error| format!("Could not add fallback entries to {label}: {error}"))
}

fn line_entry_key(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let end = trimmed.find("\":")?;
    serde_json::from_str(&trimmed[..=end]).ok()
}

// Returns only properties of the root JSON object. Tracking structural depth
// prevents nested metadata such as a pretty-printed `$meta` object from being
// mistaken for translation entries and having its commas rewritten.
fn top_level_entry_lines(lines: &[String]) -> Vec<(usize, usize, String)> {
    let mut entries = Vec::new();
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_entry: Option<(usize, String)> = None;

    for (index, line) in lines.iter().enumerate() {
        if depth == 1 && pending_entry.is_none() {
            if let Some(key) = line_entry_key(line) {
                pending_entry = Some((index, key));
            }
        }
        for character in line.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    in_string = false;
                }
                continue;
            }
            match character {
                '"' => in_string = true,
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => {}
            }
        }
        if depth == 1 {
            if let Some((start, key)) = pending_entry.take() {
                entries.push((start, index, key));
            }
        }
    }
    entries
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
    fallback_entries: Vec<(String, String)>,
) -> Result<HashMap<String, String>, String> {
    let id = canonical_language_id(&id);
    let user_dir = user_translations_dir(&app)?;
    let bundled_dir = bundled_translations_dir();
    if id != "en" {
        let bundled_base = bundled_dir
            .as_ref()
            .map(|dir| (dir.join(&id).join("base.json"), "packaged"));
        let user_base = (user_dir.join(&id).join("base.json"), "user");
        for (path, source) in bundled_base.into_iter().chain([user_base]) {
            if !path.is_file() {
                continue;
            }
            // Development/package and user translations both receive missing
            // keys when writable. Installed packaged files may be read-only;
            // failure is non-fatal because runtime English fallback still works.
            if let Err(error) = add_missing_fallback_entries(
                &path,
                &format!("{source} {id}/base.json"),
                &fallback_entries,
            ) {
                eprintln!("Translation fallback merge skipped: {error}");
            }
        }
    }
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
    use super::{
        add_missing_fallback_entries, canonical_language_id, load_translation_from_roots,
    };
    use serde_json::Value;
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
    fn adds_only_missing_fallbacks_without_overwriting_translations() {
        let root = temp_root("translation-missing-keys");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("base.json");
        fs::write(
            &path,
            "{\n  \"$meta\": {\n    \"name\": \"Test\",\n    \"direction\": \"ltr\"\n  },\n\n  \"existing\": \"已翻譯\"\n}\n",
        )
        .unwrap();

        add_missing_fallback_entries(
            &path,
            "test/base.json",
            &[
                ("existing".to_string(), "English".to_string()),
                ("new.key".to_string(), "New fallback".to_string()),
            ],
        )
        .unwrap();

        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["$meta"]["name"], "Test");
        assert_eq!(parsed["$meta"]["direction"], "ltr");
        assert_eq!(parsed["existing"], "已翻譯");
        assert_eq!(parsed["new.key"], "New fallback");
        fs::remove_dir_all(root).unwrap();
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

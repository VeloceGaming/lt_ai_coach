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

fn add_missing_fallback_entries(
    path: &std::path::Path,
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
    // as champion names. Remove only those English-valued entries so they can
    // be placed back into the canonical UI-key order below.
    let first_extra = lines.iter().position(|line| {
        line_entry_key(line).is_some_and(|key| key != "$meta" && !fallback_by_key.contains_key(key.as_str()))
    });
    if let Some(first_extra) = first_extra {
        lines = lines
            .into_iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let should_move = index > first_extra
                    && line_entry_key(&line).is_some_and(|key| {
                        fallback_by_key.get(key.as_str()).is_some_and(|fallback| {
                            object.get(&key).and_then(Value::as_str) == Some(*fallback)
                        })
                    });
                (!should_move).then_some(line)
            })
            .collect();
    }

    for (fallback_index, (key, fallback)) in fallback_entries.iter().enumerate() {
        if lines.iter().any(|line| line_entry_key(line).as_deref() == Some(key)) {
            continue;
        }
        let next_key = fallback_entries[fallback_index + 1..]
            .iter()
            .map(|(next, _)| next)
            .find(|next| lines.iter().any(|line| line_entry_key(line).as_deref() == Some(next)));
        let mut insertion = next_key
            .and_then(|next| lines.iter().position(|line| line_entry_key(line).as_deref() == Some(next)))
            .or_else(|| {
                lines.iter().position(|line| {
                    line_entry_key(line)
                        .is_some_and(|entry| entry != "$meta" && !fallback_by_key.contains_key(entry.as_str()))
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

    let entry_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line_entry_key(line).map(|_| index))
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

#[tauri::command]
pub fn load_translation(
    app: tauri::AppHandle,
    id: String,
    fallback_entries: Vec<(String, String)>,
) -> Result<HashMap<String, String>, String> {
    let dir = translations_dir(&app)?;
    let id = canonical_language_id(&id);
    let language_dir = dir.join(&id);
    let base_path = language_dir.join("base.json");
    let mod_path = language_dir.join("mod.json");
    let mut out = HashMap::new();

    if base_path.is_file() {
        if id != "en" {
            // A read-only installed translation should still load; the frontend
            // will continue using the same English fallback if repair cannot write.
            if let Err(error) = add_missing_fallback_entries(
                &base_path,
                &format!("{id}/base.json"),
                &fallback_entries,
            ) {
                eprintln!("Translation fallback repair skipped: {error}");
            }
        }
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
    use super::{add_missing_fallback_entries, canonical_language_id};
    use serde_json::Value;
    use std::fs;

    #[test]
    fn canonicalizes_game_and_legacy_chinese_language_ids() {
        assert_eq!(canonical_language_id("zh-TW"), "zh-hant");
        assert_eq!(canonical_language_id("zh-hant"), "zh-hant");
        assert_eq!(canonical_language_id("zh-CN"), "zh-hans");
        assert_eq!(canonical_language_id("ja"), "ja");
    }

    #[test]
    fn adds_only_missing_fallbacks_without_overwriting_translations() {
        let path = std::env::temp_dir().join(format!(
            "lt-ai-coach-translation-repair-{}.json",
            std::process::id()
        ));
        fs::write(
            &path,
            "{\n  \"$meta\": { \"name\": \"Test\", \"direction\": \"ltr\" },\n\n  \"existing\": \"translated\"\n}\n",
        )
        .unwrap();
        let entries = vec![
            ("existing".to_string(), "English".to_string()),
            ("new.key".to_string(), "New fallback".to_string()),
        ];

        add_missing_fallback_entries(&path, "test/base.json", &entries).unwrap();

        let repaired = fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["existing"], "translated");
        assert_eq!(parsed["new.key"], "New fallback");
        assert!(repaired.contains("\n\n  \"existing\""));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn moves_previously_appended_fallbacks_before_language_specific_keys() {
        let path = std::env::temp_dir().join(format!(
            "lt-ai-coach-translation-order-{}.json",
            std::process::id()
        ));
        fs::write(
            &path,
            "{\n  \"$meta\": { \"name\": \"Test\", \"direction\": \"ltr\" },\n\n  \"first\": \"翻譯\",\n\n  \"champion.extra\": \"角色\",\n  \"third\": \"Third\",\n  \"second\": \"Second\"\n}\n",
        )
        .unwrap();
        let entries = vec![
            ("first".to_string(), "First".to_string()),
            ("second".to_string(), "Second".to_string()),
            ("third".to_string(), "Third".to_string()),
        ];

        add_missing_fallback_entries(&path, "test/base.json", &entries).unwrap();

        let repaired = fs::read_to_string(&path).unwrap();
        let first = repaired.find("\"first\"").unwrap();
        let second = repaired.find("\"second\"").unwrap();
        let third = repaired.find("\"third\"").unwrap();
        let champion = repaired.find("\"champion.extra\"").unwrap();
        assert!(first < second && second < third && third < champion);
        assert!(repaired.contains("\"first\": \"翻譯\""));
        assert!(repaired.contains("\n\n  \"champion.extra\""));
        let _: Value = serde_json::from_str(&repaired).unwrap();
        let _ = fs::remove_file(path);
    }
}

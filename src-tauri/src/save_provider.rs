//! Game-data import for LT AI Coach.
//!
//! Data comes from the `lt_ai_coach_exporter` mod, which reads the live game
//! Database through the official SDK and writes it into the coach data folder.
//! This is probe-free and always matches the current game version.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use tauri::{AppHandle, Manager};

use crate::database;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitProbeSummary {
    pub generated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub output_path: String,
}

/// Filename the coach drops to ask the running mod to export the current game.
const REQUEST_FILE: &str = "request.txt";
const PORTRAIT_REQUEST_FILE: &str = "portrait_request.txt";
/// Exporter files that belong to one save snapshot. Clear them before requesting
/// a fresh export so optional files from an older save cannot be reused.
const EXPORT_FILES: &[&str] = &[
    "manifest.tsv",
    "champions.tsv",
    "champion_metadata.json",
    "players.tsv",
    "teams.tsv",
    "champion_balance_history.json",
    "match_replays.json",
    "pre_patch_data.json",
    "solo_rank_matches.json",
    "performance_export.tsv",
];
/// How long to wait for the mod to produce a fresh export before giving up.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(12);

/// Import the data the LT AI Coach Exporter mod wrote into the coach data
/// folder (`%LOCALAPPDATA%/com.lttools.lt-ai-coach/exporter/`).
pub async fn import_from_exporter(app: AppHandle) -> Result<database::ImportSummary, String> {
    let data_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let exporter_dir = data_root.join("exporter");
        fs::create_dir_all(&exporter_dir)
            .map_err(|error| format!("Could not create the export folder: {error}"))?;
        let manifest = exporter_dir.join("manifest.tsv");

        // Ask the running mod to export the currently-loaded game right now.
        let requested_at = SystemTime::now();
        let wait_started = Instant::now();
        clear_export_snapshot_files(&exporter_dir);
        let request = exporter_dir.join(REQUEST_FILE);
        fs::write(&request, b"export")
            .map_err(|error| format!("Could not request a game export: {error}"))?;

        // Wait for the mod to write a fresh export (manifest newer than our
        // request). If nothing happens, the game/mod isn't running.
        let mut waited = Duration::ZERO;
        let step = Duration::from_millis(150);
        let fresh = loop {
            sleep(step);
            waited += step;
            let is_fresh = fs::metadata(&manifest)
                .and_then(|meta| meta.modified())
                .map(|modified| modified > requested_at)
                .unwrap_or(false);
            if is_fresh {
                break true;
            }
            if waited >= EXPORT_TIMEOUT {
                break false;
            }
        };
        if !fresh {
            crate::performance::duration(
                "coach",
                "export_wait",
                wait_started.elapsed(),
                serde_json::json!({ "status": "timeout" }),
            );
            let _ = fs::remove_file(&request);
            return Err(
                "Couldn't get fresh data from the game. Make sure Teamfight Manager 2 is \
                 running with the LT AI Coach Exporter mod enabled and a save loaded, then \
                 click Import from Game again."
                    .to_string(),
            );
        }
        crate::performance::duration(
            "coach",
            "export_wait",
            wait_started.elapsed(),
            serde_json::json!({ "status": "ok" }),
        );
        crate::performance::ingest_exporter_trace(&exporter_dir.join("performance_export.tsv"));
        let import_started = Instant::now();
        let result = database::import_exporter_output(
            data_root.join("lt-ai-coach.sqlite3"),
            &exporter_dir,
            None,
        );
        crate::performance::duration(
            "coach",
            "database_import",
            import_started.elapsed(),
            serde_json::json!({ "status": if result.is_ok() { "ok" } else { "error" } }),
        );
        result
    })
    .await
    .map_err(|error| format!("Import task failed: {error}"))?
}

pub async fn probe_portraits_from_game(
    app: AppHandle,
    language_id: String,
) -> Result<PortraitProbeSummary, String> {
    let translations_dir = crate::i18n::user_translations_dir(&app)?;
    let data_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let exporter_dir = data_root.join("exporter");
        fs::create_dir_all(&exporter_dir)
            .map_err(|error| format!("Could not create the export folder: {error}"))?;
        let output = exporter_dir.join("portrait_probe.json");
        let request = exporter_dir.join(PORTRAIT_REQUEST_FILE);
        let _ = fs::remove_file(&output);
        let requested_at = SystemTime::now();
        fs::write(&request, b"repair-missing-portraits")
            .map_err(|error| format!("Could not request portrait repair: {error}"))?;
        let mut waited = Duration::ZERO;
        let step = Duration::from_millis(150);
        loop {
            sleep(step);
            waited += step;
            let fresh = fs::metadata(&output).and_then(|meta| meta.modified())
                .map(|modified| modified > requested_at).unwrap_or(false);
            if fresh { break; }
            if waited >= EXPORT_TIMEOUT {
                let _ = fs::remove_file(&request);
                return Err("Couldn't inspect portraits. Keep Teamfight Manager 2 running with a save loaded and the LT AI Coach Exporter enabled, then try again.".to_string());
            }
        }
        let document: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&output).map_err(|error| format!("Could not read portrait results: {error}"))?
        ).map_err(|error| format!("Could not parse portrait results: {error}"))?;
        let champions = document.get("champions").and_then(|value| value.as_array()).cloned().unwrap_or_default();
        let portrait_dir = crate::mod_portrait_dir()
            .ok_or_else(|| "Could not locate the app folder for the portrait cache.".to_string())?;
        fs::create_dir_all(&portrait_dir)
            .map_err(|error| format!("Could not create the portrait cache: {error}"))?;
        let mut generated = 0;
        let mut skipped = 0;
        let mut failed = 0;
        for entry in champions.iter().filter(|entry| {
            entry.get("status").and_then(|value| value.as_str()) == Some("resolved")
                && !entry.get("asset").and_then(|value| value.as_str()).unwrap_or_default().starts_with("asset/base/")
        }) {
            match cache_repaired_portrait(&exporter_dir, &portrait_dir, entry) {
                Ok(true) => generated += 1,
                Ok(false) => skipped += 1,
                Err(_) => failed += 1,
            }
        }
        cache_mod_champion_names(&translations_dir, &champions, &language_id)?;
        Ok(PortraitProbeSummary {
            generated,
            skipped,
            failed,
            output_path: output.to_string_lossy().to_string(),
        })
    }).await.map_err(|error| format!("Portrait inspection task failed: {error}"))?
}

fn cache_mod_champion_names(
    translations_dir: &Path,
    champions: &[serde_json::Value],
    language_id: &str,
) -> Result<(), String> {
    let language_id = crate::i18n::canonical_language_id(language_id);
    let mut translations: BTreeMap<String, String> = BTreeMap::new();
    for entry in champions.iter().filter(|entry| {
        !entry
            .get("asset")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .starts_with("asset/base/")
    }) {
        let Some(champion_id) = entry.get("championId").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(names) = entry.get("names").and_then(|value| value.as_object()) else {
            continue;
        };
        let name = names
            .iter()
            .find(|(language, _)| language.eq_ignore_ascii_case(&language_id))
            .and_then(|(_, name)| name.as_str())
            .filter(|name| !name.trim().is_empty());
        if let Some(name) = name {
            translations.insert(format!("champion.{champion_id}"), name.to_string());
        }
    }
    let text = serde_json::to_string_pretty(&translations)
        .map_err(|error| format!("Could not serialize mod champion names: {error}"))?;
    let language_dir = translations_dir.join(&language_id);
    fs::create_dir_all(&language_dir)
        .map_err(|error| format!("Could not create {language_id} translation folder: {error}"))?;
    fs::write(language_dir.join("mod.json"), text)
        .map_err(|error| format!("Could not cache mod champion names: {error}"))
}

fn cache_repaired_portrait(
    exporter_dir: &Path,
    portrait_dir: &Path,
    entry: &serde_json::Value,
) -> Result<bool, String> {
    let champion_id = entry
        .get("championId")
        .and_then(|value| value.as_str())
        .filter(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .ok_or_else(|| "Portrait result has an unsafe champion ID.".to_string())?;
    let relative_png = entry
        .get("pngFile")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .filter(|path| {
            !path.is_absolute()
                && !path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
        })
        .ok_or_else(|| format!("Portrait result for {champion_id} has an unsafe PNG path."))?;
    let frame = entry
        .pointer("/animations/anims/idle/frames/0/data")
        .or_else(|| entry.pointer("/animations/anims/stand/frames/0/data"))
        .ok_or_else(|| format!("Portrait result for {champion_id} has no idle frame."))?;
    let coordinate = |name: &str| -> Result<u32, String> {
        let value = frame
            .get(name)
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite() && *value >= 0.0 && value.fract().abs() < 0.001)
            .ok_or_else(|| format!("Portrait frame for {champion_id} has an invalid {name}."))?;
        u32::try_from(value as u64)
            .map_err(|_| format!("Portrait frame for {champion_id} is too large."))
    };
    let (x, y, width, height) = (
        coordinate("x")?,
        coordinate("y")?,
        coordinate("w")?,
        coordinate("h")?,
    );
    if width == 0 || height == 0 {
        return Err(format!("Portrait frame for {champion_id} is empty."));
    }
    let sheet_path = exporter_dir.join(relative_png);
    let sheet = image::open(&sheet_path).map_err(|error| {
        format!("Could not decode repaired portrait for {champion_id}: {error}")
    })?;
    if x.checked_add(width)
        .is_none_or(|right| right > sheet.width())
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > sheet.height())
    {
        return Err(format!(
            "Portrait frame for {champion_id} is outside its sheet."
        ));
    }
    let cropped = trim_transparent_portrait_padding(sheet.crop_imm(x, y, width, height));
    let mut png = Vec::new();
    cropped
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| {
            format!("Could not encode repaired portrait for {champion_id}: {error}")
        })?;
    let target = portrait_dir.join(format!("{champion_id}.png"));
    if fs::read(&target).ok().as_deref() == Some(png.as_slice()) {
        return Ok(false);
    }
    fs::write(&target, png)
        .map_err(|error| format!("Could not cache repaired portrait for {champion_id}: {error}"))?;
    Ok(true)
}

fn trim_transparent_portrait_padding(image: image::DynamicImage) -> image::DynamicImage {
    let (width, height) = (image.width(), image.height());
    let rgba = image.to_rgba8();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_y = 0;
    let mut max_x = 0;
    let mut has_visible_pixel = false;
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        has_visible_pixel = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !has_visible_pixel {
        return image;
    }
    let visible_width = max_x - min_x + 1;
    let visible_height = max_y - min_y + 1;
    if min_x == 0 && min_y == 0 && visible_width == width && visible_height == height {
        return image;
    }
    image.crop_imm(min_x, min_y, visible_width, visible_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn repaired_portrait_is_cropped_and_then_skipped_when_unchanged() {
        let root = std::env::temp_dir().join(format!(
            "lt-ai-coach-portrait-cache-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let exporter = root.join("exporter");
        let cache = root.join("portraits");
        fs::create_dir_all(exporter.join("portrait_probe")).unwrap();
        fs::create_dir_all(&cache).unwrap();
        RgbaImage::from_pixel(12, 8, Rgba([20, 40, 60, 255]))
            .save(exporter.join("portrait_probe/test.png"))
            .unwrap();
        let entry = serde_json::json!({
            "championId": "test_champion",
            "pngFile": "portrait_probe/test.png",
            "animations": { "anims": { "idle": { "frames": [{
                "data": { "x": 2.0, "y": 1.0, "w": 5.0, "h": 6.0 }
            }] } } }
        });
        assert!(cache_repaired_portrait(&exporter, &cache, &entry).unwrap());
        assert_eq!(
            image::image_dimensions(cache.join("test_champion.png")).unwrap(),
            (5, 6)
        );
        assert!(!cache_repaired_portrait(&exporter, &cache, &entry).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transparent_padding_is_trimmed_to_visible_sprite_bounds() {
        let mut image = RgbaImage::new(79, 147);
        for y in 53..83 {
            for x in 25..58 {
                image.put_pixel(x, y, Rgba([20, 40, 60, 255]));
            }
        }

        let normalized =
            trim_transparent_portrait_padding(image::DynamicImage::ImageRgba8(image));

        assert_eq!((normalized.width(), normalized.height()), (33, 30));
        let normalized = normalized.to_rgba8();
        assert_eq!(normalized.get_pixel(0, 0)[3], 255);
        assert_eq!(normalized.get_pixel(32, 29)[3], 255);
    }

    #[test]
    fn mod_names_are_written_only_for_the_requested_language() {
        let root = std::env::temp_dir().join(format!(
            "lt-ai-coach-mod-i18n-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let champions = vec![serde_json::json!({
            "championId": "harpy",
            "asset": "workshop/harpy.aseprite",
            "status": "resolved",
            "names": { "en": "Harpy", "zh-hant": "鷹身女妖" }
        })];

        cache_mod_champion_names(&root, &champions, "zh-TW").unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("zh-hant/mod.json")).unwrap())
                .unwrap();
        assert_eq!(value["champion.harpy"], "鷹身女妖");
        assert!(value.get("en").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn english_mod_names_are_written() {
        let root = std::env::temp_dir().join(format!(
            "lt-ai-coach-english-mod-i18n-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let champions = vec![serde_json::json!({
            "championId": "harpy",
            "asset": "workshop/harpy.aseprite",
            "status": "resolved",
            "names": { "en": "Harpy", "zh-hant": "鷹身女妖" }
        })];

        cache_mod_champion_names(&root, &champions, "en").unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("en/mod.json")).unwrap()).unwrap();
        assert_eq!(value["champion.harpy"], "Harpy");
        fs::remove_dir_all(root).unwrap();
    }
}

fn clear_export_snapshot_files(exporter_dir: &Path) {
    for file in EXPORT_FILES {
        let _ = fs::remove_file(exporter_dir.join(file));
    }

    // Do not clear `balance_snapshots/<save_id>/`: the exporter selects the
    // active campaign's ledger using the stable `save_id` in the loaded game.
    // Removing these ledgers here used to discard patch history whenever the
    // user imported after switching saves.
}

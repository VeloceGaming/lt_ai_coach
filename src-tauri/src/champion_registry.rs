use crate::{
    draft::{DraftCatalog, DraftChampion},
    statistics::{humanize_id, ChampionPortrait},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::ImageReader;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortraitCatalog {
    portraits: BTreeMap<String, PortraitEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortraitEntry {
    #[serde(default)]
    sheet: Option<String>,
    sheet_width: Option<usize>,
    sheet_height: Option<usize>,
    frame: Option<PortraitFrame>,
    #[serde(default)]
    face_offset: PortraitOffset,
    #[serde(default)]
    center_offset: PortraitOffset,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortraitFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Default, Deserialize)]
struct PortraitOffset {
    x: i32,
    y: i32,
}

#[derive(Clone, Default)]
struct CatalogMetadata {
    portrait: Option<ChampionPortrait>,
    role_fit: BTreeMap<String, f64>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionOverrides {
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    #[serde(default)]
    pub portraits: BTreeMap<String, String>,
}

pub fn load_draft_catalog(database_path: &Path) -> Result<DraftCatalog, String> {
    if !database_path.is_file() {
        return Err("No imported database is available. Load a save first.".to_string());
    }

    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open draft database: {error}"))?;
    let catalog = catalog_metadata(Some(database_path), database_path.parent())?;
    let ids = runtime_champion_ids(&connection)?;
    let overrides = database_path
        .parent()
        .map(load_overrides)
        .transpose()?
        .unwrap_or_default();
    let champions = champions_for_ids(ids, &catalog, &overrides);

    Ok(DraftCatalog { champions })
}

pub fn bridge_champions(
    database_path: Option<&Path>,
    overrides_root: Option<&Path>,
    ids: impl IntoIterator<Item = String>,
) -> Vec<DraftChampion> {
    let catalog = catalog_metadata(database_path, overrides_root).unwrap_or_default();
    let overrides = overrides_root
        .and_then(|root| load_overrides(root).ok())
        .unwrap_or_default();
    champions_for_ids(
        ids.into_iter().collect::<BTreeSet<_>>(),
        &catalog,
        &overrides,
    )
}

pub fn load_overrides(root: &Path) -> Result<ChampionOverrides, String> {
    let path = overrides_path(root);
    if !path.is_file() {
        return Ok(ChampionOverrides::default());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read champion overrides: {error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse champion overrides: {error}"))
}

pub fn save_override(
    root: &Path,
    champion_id: &str,
    name: Option<String>,
    portrait_path: Option<String>,
    name_changed: bool,
    portrait_path_changed: bool,
) -> Result<DraftChampion, String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not create app data directory: {error}"))?;
    let mut overrides = load_overrides(root)?;
    if name_changed {
        set_optional_override(&mut overrides.names, champion_id, name);
    }
    if portrait_path_changed {
        set_optional_override(&mut overrides.portraits, champion_id, portrait_path);
    }
    let text = serde_json::to_string_pretty(&overrides)
        .map_err(|error| format!("Could not serialize champion overrides: {error}"))?;
    fs::write(overrides_path(root), text)
        .map_err(|error| format!("Could not write champion overrides: {error}"))?;

    let database_path = root.join("lt-ai-coach.sqlite3");
    let catalog = catalog_metadata(Some(&database_path), Some(root)).unwrap_or_default();
    champions_for_ids(
        BTreeSet::from([champion_id.to_string()]),
        &catalog,
        &overrides,
    )
    .into_iter()
    .next()
    .ok_or_else(|| "Could not resolve champion override.".to_string())
}

fn set_optional_override(
    map: &mut BTreeMap<String, String>,
    champion_id: &str,
    value: Option<String>,
) {
    match value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            map.insert(champion_id.to_string(), value);
        }
        None => {
            map.remove(champion_id);
        }
    }
}

fn overrides_path(root: &Path) -> PathBuf {
    root.join("champion-overrides.json")
}

fn runtime_champion_ids(connection: &Connection) -> Result<BTreeSet<String>, String> {
    let mut ids = BTreeSet::new();
    for (table, message) in [
        ("enabled_champions", "enabled champion"),
        ("picks", "picked champion"),
        ("bans", "banned champion"),
    ] {
        let sql = format!("SELECT DISTINCT champion_id FROM {table}");
        let mut statement = match connection.prepare(&sql) {
            Ok(statement) => statement,
            Err(error) if table == "enabled_champions" => {
                return Err(format!("Could not prepare enabled champion query: {error}"));
            }
            Err(_) => continue,
        };
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("Could not query {message}s: {error}"))?;
        for row in rows {
            ids.insert(row.map_err(|error| format!("Could not read {message}: {error}"))?);
        }
    }
    Ok(ids)
}

fn catalog_metadata(
    database_path: Option<&Path>,
    runtime_root: Option<&Path>,
) -> Result<BTreeMap<String, CatalogMetadata>, String> {
    let mut metadata = database_metadata(database_path)?;
    let overlays: PortraitCatalog =
        serde_json::from_str(crate::BASE_PORTRAIT_CATALOG).map_err(|error| error.to_string())?;
    for (id, overlay) in overlays.portraits {
        let Some(frame) = overlay.frame else { continue };
        let current = metadata.entry(id).or_default();
        let Some(path) = overlay.sheet.map(|path| {
            format!(
                "/{}",
                path.strip_prefix("assets/")
                    .unwrap_or(&path)
                    .replace('\\', "/")
            )
        }) else {
            continue;
        };
        current.portrait = Some(ChampionPortrait {
            path,
            sheet_width: overlay.sheet_width.unwrap_or(0),
            sheet_height: overlay.sheet_height.unwrap_or(0),
            x: frame.x as usize,
            y: frame.y as usize,
            width: frame.width as usize,
            height: frame.height as usize,
            face_offset_x: overlay.face_offset.x,
            face_offset_y: overlay.face_offset.y,
            center_offset_x: overlay.center_offset.x,
            center_offset_y: overlay.center_offset.y,
        });
    }
    if runtime_root.is_some() {
        for (id, portrait) in runtime_portraits()? {
            metadata.entry(id).or_default().portrait = Some(portrait);
        }
    }
    Ok(metadata)
}

fn database_metadata(
    database_path: Option<&Path>,
) -> Result<BTreeMap<String, CatalogMetadata>, String> {
    let Some(database_path) = database_path.filter(|path| path.is_file()) else {
        return Ok(BTreeMap::new());
    };
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open champion metadata database: {error}"))?;
    let mut statement = match connection
        .prepare("SELECT champion_id, role_fit_json FROM champion_metadata ORDER BY champion_id")
    {
        Ok(statement) => statement,
        Err(_) => return Ok(BTreeMap::new()),
    };
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("Could not query champion metadata: {error}"))?;
    let mut metadata = BTreeMap::new();
    for row in rows {
        let (id, role_fit_json) =
            row.map_err(|error| format!("Could not read champion metadata: {error}"))?;
        let role_fit = serde_json::from_str(&role_fit_json).unwrap_or_default();
        metadata.insert(
            id,
            CatalogMetadata {
                portrait: None,
                role_fit,
            },
        );
    }
    Ok(metadata)
}

pub fn imported_champion_tags(
    database_path: &Path,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    if !database_path.is_file() {
        return Ok(BTreeMap::new());
    }
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open champion metadata database: {error}"))?;
    let mut statement = match connection
        .prepare("SELECT champion_id, raw_tags_json FROM champion_metadata ORDER BY champion_id")
    {
        Ok(statement) => statement,
        Err(_) => return Ok(BTreeMap::new()),
    };
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("Could not query champion tags: {error}"))?;
    let mut tags = BTreeMap::new();
    for row in rows {
        let (id, raw_tags_json) =
            row.map_err(|error| format!("Could not read champion tags: {error}"))?;
        tags.insert(id, serde_json::from_str(&raw_tags_json).unwrap_or_default());
    }
    Ok(tags)
}

// Root of the bundled `assets/` tree used to resolve manual override portraits
// (champion-overrides.json). In a source checkout this is the project's assets
// folder; packaged builds without that folder simply resolve no override
// portrait and fall back to the built-in catalog art.
fn default_assets_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("assets")
}

pub(crate) fn resolved_catalog_portraits(
    runtime_root: Option<&Path>,
) -> Result<BTreeMap<String, ChampionPortrait>, String> {
    Ok(catalog_metadata(None, runtime_root)?
        .into_iter()
        .filter_map(|(id, metadata)| metadata.portrait.map(|portrait| (id, portrait)))
        .collect())
}

fn runtime_portraits() -> Result<BTreeMap<String, ChampionPortrait>, String> {
    let Some(directory) = crate::mod_portrait_dir() else {
        return Ok(BTreeMap::new());
    };
    if !directory.is_dir() {
        return Ok(BTreeMap::new());
    }
    let entries = fs::read_dir(&directory)
        .map_err(|error| format!("Could not read the repaired portrait cache: {error}"))?;
    let mut portraits = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("png") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let dimensions = match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        {
            Ok(image) => (image.width() as usize, image.height() as usize),
            Err(_) => continue,
        };
        if dimensions.0 == 0 || dimensions.1 == 0 {
            continue;
        }
        portraits.insert(
            id.to_string(),
            ChampionPortrait {
                path: format!("data:image/png;base64,{}", BASE64.encode(&bytes)),
                sheet_width: dimensions.0,
                sheet_height: dimensions.1,
                x: 0,
                y: 0,
                width: dimensions.0,
                height: dimensions.1,
                face_offset_x: 0,
                face_offset_y: 0,
                center_offset_x: 0,
                center_offset_y: 0,
            },
        );
    }
    Ok(portraits)
}

fn champions_for_ids(
    ids: BTreeSet<String>,
    catalog: &BTreeMap<String, CatalogMetadata>,
    overrides: &ChampionOverrides,
) -> Vec<DraftChampion> {
    let assets_root = default_assets_root();
    ids.into_iter()
        .map(|id| {
            let metadata = catalog.get(&id).cloned().unwrap_or_default();
            let override_portrait = overrides
                .portraits
                .get(&id)
                .and_then(|path| portrait_from_relative_path(&assets_root, path));
            let portrait = override_portrait.or(metadata.portrait);
            DraftChampion {
                name: overrides
                    .names
                    .get(&id)
                    .filter(|name| !name.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| humanize_id(&id)),
                portrait,
                role_fit: metadata.role_fit,
                id,
            }
        })
        .collect()
}

fn portrait_from_relative_path(
    assets_root: &Path,
    relative_path: &str,
) -> Option<ChampionPortrait> {
    let normalized = relative_path
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/");
    if normalized.is_empty() || normalized.contains("..") {
        return None;
    }
    let relative = PathBuf::from(normalized);
    let path = assets_root.join(&relative);
    let reader = ImageReader::open(&path).ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let side = width.min(height) as usize;
    Some(ChampionPortrait {
        path: format!("/{}", relative.to_string_lossy().replace('\\', "/")),
        sheet_width: width as usize,
        sheet_height: height as usize,
        x: 0,
        y: 0,
        width: side,
        height: side,
        face_offset_x: 0,
        face_offset_y: 0,
        center_offset_x: 0,
        center_offset_y: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_database(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "lt-ai-coach-{name}-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ))
    }

    #[test]
    fn base_portrait_and_imported_role_fit_are_combined() {
        let path = temp_database("registry-crop");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE enabled_champions (champion_id TEXT PRIMARY KEY);
                 INSERT INTO enabled_champions VALUES ('wind_mage');
                 CREATE TABLE champion_metadata (
                    champion_id TEXT PRIMARY KEY,
                    role_fit_json TEXT NOT NULL
                 );
                 INSERT INTO champion_metadata VALUES ('wind_mage', '{\"mid\":100}');",
            )
            .unwrap();
        drop(connection);

        let catalog = load_draft_catalog(&path).unwrap();
        let wind = catalog
            .champions
            .iter()
            .find(|champion| champion.id == "wind_mage")
            .unwrap();
        let portrait = wind.portrait.as_ref().unwrap();
        assert_eq!(portrait.x, 26);
        assert_eq!(portrait.width, 23);
        assert_eq!(wind.role_fit.get("mid"), Some(&100.0));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn banned_only_champions_are_runtime_champions() {
        let path = temp_database("registry-bans");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE enabled_champions (champion_id TEXT PRIMARY KEY);
                 INSERT INTO enabled_champions VALUES ('swordman');
                 CREATE TABLE bans (champion_id TEXT NOT NULL);
                 INSERT INTO bans VALUES ('wind_mage');",
            )
            .unwrap();
        drop(connection);

        let catalog = load_draft_catalog(&path).unwrap();
        let ids = catalog
            .champions
            .iter()
            .map(|champion| champion.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["swordman", "wind_mage"]);
        fs::remove_file(path).unwrap();
    }
}

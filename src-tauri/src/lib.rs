use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};
use tauri::Manager;

mod athletes;
mod champion_registry;
mod database;
mod draft;
mod draft_bridge;
mod i18n;
mod interactions;
mod manual_tiers;
mod patch;
mod pilot;
mod recommendation;
mod save_provider;
mod statistics;

const CHAMPION_CATALOG: &str = include_str!("../../data/catalog/champions.json");
pub(crate) const BASE_PORTRAIT_CATALOG: &str = include_str!("../../data/catalog/portraits.json");
pub(crate) const MOD_PORTRAIT_CATALOG: &str = include_str!("../../data/catalog/mod-portraits.json");

/// Folder where repaired mod-champion portraits are cached at runtime:
/// `generated/mod-portraits` next to the LT AI Coach executable. Keeping the
/// generated art beside the app (rather than hidden in per-user app data) makes
/// it visible and portable with the rest of the package.
pub(crate) fn mod_portrait_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("generated").join("mod-portraits"))
}

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, PartialEq, Eq)]
struct DatabaseFingerprint {
    database: Option<FileStamp>,
    wal: Option<FileStamp>,
}

struct RecommendationData {
    database_path: PathBuf,
    fingerprint: DatabaseFingerprint,
    catalog: Arc<draft::DraftCatalog>,
    statistics: Arc<statistics::RoleStatistics>,
    interactions: Arc<interactions::InteractionEvidence>,
    manual_tiers: Arc<std::collections::BTreeMap<String, manual_tiers::ManualTier>>,
    athletes: Arc<athletes::AthleteIndex>,
}

type CachedRecommendationData = (
    Arc<draft::DraftCatalog>,
    Arc<statistics::RoleStatistics>,
    Arc<interactions::InteractionEvidence>,
    Arc<std::collections::BTreeMap<String, manual_tiers::ManualTier>>,
    Arc<athletes::AthleteIndex>,
);

static RECOMMENDATION_DATA_CACHE: OnceLock<Mutex<Option<RecommendationData>>> = OnceLock::new();

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn database_fingerprint(database_path: &Path) -> DatabaseFingerprint {
    let mut wal_path = database_path.as_os_str().to_os_string();
    wal_path.push("-wal");
    let wal_path = PathBuf::from(wal_path);
    DatabaseFingerprint {
        database: file_stamp(database_path),
        wal: file_stamp(&wal_path),
    }
}

fn load_recommendation_data(database_path: &Path) -> Result<CachedRecommendationData, String> {
    let cache = RECOMMENDATION_DATA_CACHE.get_or_init(|| Mutex::new(None));
    let fingerprint = database_fingerprint(database_path);
    let mut cached = cache
        .lock()
        .map_err(|_| "Recommendation data cache is unavailable.".to_string())?;
    if let Some(data) = cached
        .as_ref()
        .filter(|data| data.database_path == database_path && data.fingerprint == fingerprint)
    {
        return Ok((
            Arc::clone(&data.catalog),
            Arc::clone(&data.statistics),
            Arc::clone(&data.interactions),
            Arc::clone(&data.manual_tiers),
            Arc::clone(&data.athletes),
        ));
    }

    let catalog = Arc::new(draft::load_draft_catalog(database_path, CHAMPION_CATALOG)?);
    let statistics = Arc::new(statistics::query_role_statistics(
        database_path,
        CHAMPION_CATALOG,
    )?);
    let interactions = Arc::new(interactions::query_interactions(database_path)?);
    let manual_tiers = Arc::new(manual_tiers::query_manual_tiers(database_path)?);
    let athletes = Arc::new(athletes::load_athlete_index(database_path)?);
    *cached = Some(RecommendationData {
        database_path: database_path.to_path_buf(),
        fingerprint: database_fingerprint(database_path),
        catalog: Arc::clone(&catalog),
        statistics: Arc::clone(&statistics),
        interactions: Arc::clone(&interactions),
        manual_tiers: Arc::clone(&manual_tiers),
        athletes: Arc::clone(&athletes),
    });
    Ok((catalog, statistics, interactions, manual_tiers, athletes))
}

fn invalidate_recommendation_data() {
    if let Some(cache) = RECOMMENDATION_DATA_CACHE.get() {
        if let Ok(mut cached) = cache.lock() {
            *cached = None;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    backend: &'static str,
    phase: &'static str,
    catalog_champions: usize,
    database_ready: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveRecommendationOptions {
    mode: String,
    #[serde(default)]
    weights: recommendation::ScoringWeights,
    #[serde(default)]
    tuning: recommendation::DraftTuning,
    #[serde(default = "default_minimum_interaction_games")]
    minimum_interaction_games: usize,
    #[serde(default)]
    role_overrides: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveRecommendationResponse {
    source_revision: u64,
    source_context_revision: u64,
    shortlist: recommendation::RecommendationShortlist,
}

fn default_minimum_interaction_games() -> usize {
    3
}

#[derive(serde::Deserialize)]
struct ChampionCatalog {
    champions: Vec<serde_json::Value>,
}

#[tauri::command]
fn get_app_status(app: tauri::AppHandle) -> Result<AppStatus, String> {
    let catalog: ChampionCatalog =
        serde_json::from_str(CHAMPION_CATALOG).map_err(|error| error.to_string())?;
    let database_ready = app
        .path()
        .app_local_data_dir()
        .map(|path| path.join("lt-ai-coach.sqlite3").is_file())
        .unwrap_or(false);

    Ok(AppStatus {
        backend: "Tauri 2 + Rust",
        phase: "Patch-aware local coach",
        catalog_champions: catalog.champions.len(),
        database_ready,
    })
}

#[tauri::command]
async fn import_from_game_export(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, draft_bridge::DraftBridge>,
) -> Result<database::ImportSummary, String> {
    let result = save_provider::import_from_exporter(app).await?;
    let player_team_id = result
        .player_team_id
        .map(|id| {
            usize::try_from(id).map_err(|_| "Imported player team ID is invalid.".to_string())
        })
        .transpose()?;
    bridge.set_player_team_id(player_team_id);
    invalidate_recommendation_data();
    Ok(result)
}

#[tauri::command]
async fn probe_game_portraits(
    app: tauri::AppHandle,
    language_id: String,
) -> Result<save_provider::PortraitProbeSummary, String> {
    let result = save_provider::probe_portraits_from_game(app, language_id).await?;
    invalidate_recommendation_data();
    Ok(result)
}

#[tauri::command]
async fn get_role_statistics(app: tauri::AppHandle) -> Result<statistics::RoleStatistics, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        statistics::query_role_statistics(&database_path, CHAMPION_CATALOG)
    })
    .await
    .map_err(|error| format!("Statistics task failed: {error}"))?
}

#[tauri::command]
async fn get_patch_history(
    app: tauri::AppHandle,
) -> Result<Vec<statistics::PatchHistoryEntry>, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || statistics::query_patch_history(&database_path))
        .await
        .map_err(|error| format!("Patch history task failed: {error}"))?
}

#[tauri::command]
async fn get_draft_catalog(app: tauri::AppHandle) -> Result<draft::DraftCatalog, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        draft::load_draft_catalog(&database_path, CHAMPION_CATALOG)
    })
    .await
    .map_err(|error| format!("Draft catalog task failed: {error}"))?
}

#[tauri::command]
async fn get_athletes(app: tauri::AppHandle) -> Result<Vec<athletes::AthleteSummary>, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || athletes::query_athletes(&database_path))
        .await
        .map_err(|error| format!("Athlete list task failed: {error}"))?
}

#[tauri::command]
async fn get_athlete_detail(
    app: tauri::AppHandle,
    athlete_id: i64,
) -> Result<Option<athletes::AthleteDetail>, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        athletes::query_athlete_detail(&database_path, athlete_id)
    })
    .await
    .map_err(|error| format!("Athlete detail task failed: {error}"))?
}

#[tauri::command]
async fn get_athlete_mastery(
    app: tauri::AppHandle,
    athlete_id: i64,
    champion_id: String,
) -> Result<Option<athletes::AthleteChampionLookup>, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        athletes::query_mastery(&database_path, athlete_id, &champion_id)
    })
    .await
    .map_err(|error| format!("Athlete mastery task failed: {error}"))?
}

#[tauri::command]
async fn prepare_recommendation_cache(
    app: tauri::AppHandle,
) -> Result<statistics::RoleStatistics, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        let (_, statistics, _, _, _) = load_recommendation_data(&database_path)?;
        Ok(statistics.as_ref().clone())
    })
    .await
    .map_err(|error| format!("Coach preparation task failed: {error}"))?
}

#[tauri::command]
async fn get_recommendations(
    app: tauri::AppHandle,
    request: recommendation::RecommendationRequest,
) -> Result<recommendation::RecommendationShortlist, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        let (catalog, statistics, interactions, manual_tiers, athletes) =
            load_recommendation_data(&database_path)?;
        Ok::<_, String>(recommendation::build_shortlist_with_athletes(
            &request,
            catalog.as_ref(),
            statistics.as_ref(),
            interactions.as_ref(),
            manual_tiers.as_ref(),
            athletes.as_ref(),
        ))
    })
    .await
    .map_err(|error| format!("Recommendation task failed: {error}"))?
}

#[tauri::command]
async fn get_live_recommendations(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, draft_bridge::DraftBridge>,
    options: LiveRecommendationOptions,
) -> Result<LiveRecommendationResponse, String> {
    let snapshot = bridge.snapshot();
    let request = live_recommendation_request(&snapshot, options)?;
    let source_revision = snapshot.revision;
    let source_context_revision = snapshot.context_revision;
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    let shortlist = tauri::async_runtime::spawn_blocking(move || {
        let (catalog, statistics, interactions, manual_tiers, athletes) =
            load_recommendation_data(&database_path)?;
        Ok::<_, String>(recommendation::build_shortlist_with_athletes(
            &request,
            catalog.as_ref(),
            statistics.as_ref(),
            interactions.as_ref(),
            manual_tiers.as_ref(),
            athletes.as_ref(),
        ))
    })
    .await
    .map_err(|error| format!("Live recommendation task failed: {error}"))??;
    Ok(LiveRecommendationResponse {
        source_revision,
        source_context_revision,
        shortlist,
    })
}

fn live_recommendation_request(
    snapshot: &draft_bridge::BridgeState,
    options: LiveRecommendationOptions,
) -> Result<recommendation::RecommendationRequest, String> {
    let side = snapshot
        .user_side
        .as_deref()
        .filter(|side| matches!(*side, "blue" | "red"))
        .ok_or_else(|| "Waiting for live user-team context.".to_string())?;
    let set_number = snapshot
        .set_number
        .ok_or_else(|| "Waiting for live match context.".to_string())?;
    let history = |blue: bool| {
        snapshot
            .completed_games
            .iter()
            .filter(|game| game.game_number < set_number)
            .flat_map(|game| {
                if blue {
                    game.blue_picks.iter()
                } else {
                    game.red_picks.iter()
                }
            })
            .cloned()
            .collect()
    };
    Ok(recommendation::RecommendationRequest {
        mode: options.mode,
        side: side.to_string(),
        blue_bans: snapshot.blue_bans.clone(),
        red_bans: snapshot.red_bans.clone(),
        blue_picks: snapshot.blue_picks.clone(),
        red_picks: snapshot.red_picks.clone(),
        history_blue: history(true),
        history_red: history(false),
        weights: options.weights,
        tuning: options.tuning,
        minimum_interaction_games: options.minimum_interaction_games,
        blue_lineup: draft_lineup(&snapshot.blue_starters),
        red_lineup: draft_lineup(&snapshot.red_starters),
        role_overrides: options.role_overrides,
    })
}

fn draft_lineup(starters: &[usize]) -> Option<recommendation::DraftLineup> {
    let [top, jungle, mid, bot, support] = starters else {
        return None;
    };
    Some(recommendation::DraftLineup {
        top: Some(*top as i64),
        jungle: Some(*jungle as i64),
        mid: Some(*mid as i64),
        bot: Some(*bot as i64),
        support: Some(*support as i64),
    })
}

// Current manual tier flags, as champion_id -> single-letter code, for the UI
// to render the per-champion selectors.
#[tauri::command]
async fn get_manual_tiers(
    app: tauri::AppHandle,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        let tiers = manual_tiers::query_manual_tiers(&database_path)?;
        Ok(tiers
            .into_iter()
            .map(|(champion_id, tier)| (champion_id, tier.code().to_string()))
            .collect())
    })
    .await
    .map_err(|error| format!("Tier query task failed: {error}"))?
}

// Set or clear a champion's manual tier. A `tier` of None / "" / an unknown
// code clears the flag. Invalidates the recommendation cache so the next
// request reflects the change.
#[tauri::command]
async fn set_champion_tier(
    app: tauri::AppHandle,
    champion_id: String,
    tier: Option<String>,
) -> Result<(), String> {
    let database_path = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?
        .join("lt-ai-coach.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        manual_tiers::set_manual_tier(&database_path, &champion_id, tier.as_deref())
    })
    .await
    .map_err(|error| format!("Tier update task failed: {error}"))??;
    invalidate_recommendation_data();
    Ok(())
}

#[tauri::command]
async fn set_champion_override(
    app: tauri::AppHandle,
    champion_id: String,
    name: Option<String>,
    portrait_path: Option<String>,
    name_changed: bool,
    portrait_path_changed: bool,
) -> Result<draft::DraftChampion, String> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        champion_registry::save_override(
            &root,
            &champion_id,
            name,
            portrait_path,
            name_changed,
            portrait_path_changed,
        )
    })
    .await
    .map_err(|error| format!("Champion override task failed: {error}"))?
}

// Live ban state from the lt_ai_coach_bridge game mod (UDP). Returns the latest
// snapshot; `connected` is false when no draft packets have arrived recently.
#[tauri::command]
fn get_draft_bridge(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, draft_bridge::DraftBridge>,
) -> draft_bridge::BridgeState {
    let mut snapshot = bridge.snapshot();
    let ids = snapshot
        .blue_bans
        .iter()
        .chain(&snapshot.red_bans)
        .chain(&snapshot.blue_picks)
        .chain(&snapshot.red_picks)
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let overrides_root = app.path().app_local_data_dir().ok();
    snapshot.champions =
        champion_registry::bridge_champions(CHAMPION_CATALOG, overrides_root.as_deref(), ids);
    snapshot
}

// Champion id -> game-native tags, as sent live by the bridge mod (empty until a
// LTAC2TAGS packet arrives). The frontend prefers these over the bundled catalog
// for comp analysis, so modded champions get tags with no re-export.
#[tauri::command]
fn get_champion_tags(
    bridge: tauri::State<'_, draft_bridge::DraftBridge>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    bridge.champion_tags()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(draft_bridge::DraftBridge::start(app.handle().clone()));
            Ok(())
        })
        .on_window_event(|window, event| {
            // The overlay is a second, normally hidden window. Without an
            // explicit application exit it keeps the process alive after the
            // user closes the main window.
            if window.label() == "main"
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                if let Some(overlay) = window.app_handle().get_webview_window("overlay") {
                    let _ = overlay.destroy();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            import_from_game_export,
            probe_game_portraits,
            get_role_statistics,
            get_draft_catalog,
            get_athletes,
            get_athlete_detail,
            get_athlete_mastery,
            get_patch_history,
            prepare_recommendation_cache,
            get_recommendations,
            get_live_recommendations,
            get_draft_bridge,
            get_champion_tags,
            get_manual_tiers,
            set_champion_tier,
            set_champion_override,
            i18n::list_translations,
            i18n::load_translation,
            i18n::export_translation_template,
            i18n::open_translations_folder
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LT AI Coach");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_status_reports_embedded_catalog() {
        let catalog: ChampionCatalog =
            serde_json::from_str(CHAMPION_CATALOG).expect("catalog should deserialize");
        assert_eq!(catalog.champions.len(), 72);
    }

    #[test]
    #[ignore = "profiles the recommendation data cache against the live database"]
    fn profiles_live_recommendation_cache() {
        use std::time::Instant;

        let path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("lt-ai-coach.sqlite3");
        invalidate_recommendation_data();

        let started = Instant::now();
        let first = load_recommendation_data(&path).unwrap();
        let cold_elapsed = started.elapsed();

        let started = Instant::now();
        let second = load_recommendation_data(&path).unwrap();
        let warm_elapsed = started.elapsed();

        assert!(Arc::ptr_eq(&first.0, &second.0));
        assert!(Arc::ptr_eq(&first.1, &second.1));
        assert!(Arc::ptr_eq(&first.2, &second.2));
        eprintln!("cold={cold_elapsed:?} warm={warm_elapsed:?}");
    }
}

//! Receives live ban/pick state from the `lt_ai_coach_bridge` game mod over UDP
//! loopback (port 32145). The mod reads confirmed bans and picks straight from
//! the game's ban/pick UI and sends `LTAC2|<blue bans>|<red bans>|<blue picks>
//! |<red picks>` (champion ids, comma-separated, slot order) about once a
//! second while a draft is on screen. It also sends `LTAC2PHASE|<phase>` for
//! non-draft UI phases the app cares about, such as the stadium entrance.
//! `LTAC2RULES|<bans per side>` carries the live 1–5 ban format.
//! `LTAC2MODE|<mode>` carries the cached live draft mode.
//! `LTAC2CTX|...` packets add the current match/set plus resolved blue/red
//! database team IDs and fixed-role starter athlete IDs.

use crate::draft::DraftChampion;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    net::UdpSocket,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

const LISTEN_ADDRESS: &str = "127.0.0.1:32145";
const SESSION_TIMEOUT_MS: u128 = 5_000;
const PICKS_PER_SIDE: usize = 5;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeState {
    /// True while packets are arriving (a draft is live in-game).
    pub connected: bool,
    /// Bumped whenever the draft set changes, so the UI applies only on change.
    pub revision: u64,
    /// Current bridge-observed UI phase. Phase packets do not affect `connected`;
    /// draft packets still own the live-draft heartbeat.
    pub phase: String,
    pub phase_revision: u64,
    pub blue_bans: Vec<String>,
    pub red_bans: Vec<String>,
    pub blue_picks: Vec<String>,
    pub red_picks: Vec<String>,
    /// Authoritative ban-slot count reported by the game bridge (1..=5).
    /// None keeps compatibility with older bridge versions.
    pub bans_per_side: Option<usize>,
    /// Authoritative live mode reported by the bridge, when available.
    pub draft_mode: Option<String>,
    pub context_revision: u64,
    pub match_id: Option<usize>,
    pub set_number: Option<usize>,
    pub blue_team_id: Option<usize>,
    pub red_team_id: Option<usize>,
    pub blue_starters: Vec<usize>,
    pub red_starters: Vec<usize>,
    pub user_side: Option<String>,
    pub completed_games: Vec<BridgeGame>,
    #[serde(default)]
    pub champions: Vec<DraftChampion>,
    #[serde(skip)]
    received_at_unix_ms: u128,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeGame {
    pub game_number: usize,
    pub blue_picks: Vec<String>,
    pub red_picks: Vec<String>,
}

#[derive(Clone, Default)]
pub struct DraftBridge {
    state: Arc<Mutex<BridgeState>>,
    player_team_id: Arc<Mutex<Option<usize>>>,
    // Champion id -> game-native tags, sent by the mod's LTAC2TAGS packet. Static
    // per save, so it just holds the latest snapshot. Used for comp analysis.
    tags: Arc<Mutex<BTreeMap<String, Vec<String>>>>,
}

impl DraftBridge {
    pub fn start(app: tauri::AppHandle) -> Self {
        let bridge = Self::default();
        let player_team_id = app
            .path()
            .app_local_data_dir()
            .ok()
            .and_then(|root| {
                crate::database::query_player_team_id(&root.join("lt-ai-coach.sqlite3")).ok()
            })
            .flatten();
        bridge.set_player_team_id(player_team_id);
        let state = Arc::clone(&bridge.state);
        let tags = Arc::clone(&bridge.tags);
        let player_team_id = Arc::clone(&bridge.player_team_id);
        thread::spawn(move || listen(state, tags, player_team_id, app));
        bridge
    }

    pub fn snapshot(&self) -> BridgeState {
        let mut snapshot = self
            .state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default();
        snapshot.connected = snapshot.received_at_unix_ms > 0
            && now_unix_ms().saturating_sub(snapshot.received_at_unix_ms) < SESSION_TIMEOUT_MS;
        snapshot
    }

    pub fn champion_tags(&self) -> BTreeMap<String, Vec<String>> {
        self.tags
            .lock()
            .map(|tags| tags.clone())
            .unwrap_or_default()
    }

    pub fn set_player_team_id(&self, player_team_id: Option<usize>) {
        if let Ok(mut known) = self.player_team_id.lock() {
            *known = player_team_id;
        }
        if let Ok(mut current) = self.state.lock() {
            let user_side =
                derive_user_side(player_team_id, current.blue_team_id, current.red_team_id);
            if current.user_side != user_side {
                current.context_revision = current.context_revision.wrapping_add(1);
                current.user_side = user_side;
            }
        }
    }
}

fn listen(
    state: Arc<Mutex<BridgeState>>,
    tags: Arc<Mutex<BTreeMap<String, Vec<String>>>>,
    player_team_id: Arc<Mutex<Option<usize>>>,
    app: tauri::AppHandle,
) {
    let socket = loop {
        match UdpSocket::bind(LISTEN_ADDRESS) {
            Ok(socket) => break socket,
            Err(_) => thread::sleep(Duration::from_secs(1)),
        }
    };
    // Roomy enough for the full champion->tags table (many champions + mods).
    let mut buffer = [0u8; 65536];
    loop {
        let Ok((length, _)) = socket.recv_from(&mut buffer) else {
            continue;
        };
        let Ok(packet) = std::str::from_utf8(&buffer[..length]) else {
            continue;
        };
        if let Some((action, duration_us, action_count)) = parse_performance_packet(packet) {
            crate::performance::duration(
                "bridge",
                action,
                Duration::from_micros(duration_us),
                serde_json::json!({ "actionCount": action_count }),
            );
            continue;
        }
        // Champion-tags snapshot (separate from the draft packet; doesn't affect
        // the live-draft `connected` heartbeat).
        if let Some(parsed_tags) = parse_tags_packet(packet) {
            if let Ok(mut current) = tags.lock() {
                *current = parsed_tags;
            }
            continue;
        }
        if let Some(phase) = parse_phase_packet(packet) {
            let mut entered_stadium_entrance = false;
            if let Ok(mut current) = state.lock() {
                entered_stadium_entrance =
                    current.phase != "stadiumEntrance" && phase == "stadiumEntrance";
                set_phase(&mut current, phase);
            }
            // Draft state lives independently in the main and overlay webviews.
            // Broadcast this transition from the single UDP listener so each
            // window resets its own local series state at the same moment.
            if entered_stadium_entrance {
                let _ = app.emit("draft-series-reset", ());
            }
            continue;
        }
        if let Some(bans_per_side) = parse_rules_packet(packet) {
            if let Ok(mut current) = state.lock() {
                if current.bans_per_side != Some(bans_per_side) {
                    current.bans_per_side = Some(bans_per_side);
                    current.context_revision = current.context_revision.wrapping_add(1);
                }
            }
            continue;
        }
        if let Some(draft_mode) = parse_mode_packet(packet) {
            if let Ok(mut current) = state.lock() {
                if current.draft_mode.as_deref() != Some(draft_mode) {
                    current.draft_mode = Some(draft_mode.to_string());
                    current.context_revision = current.context_revision.wrapping_add(1);
                }
            }
            continue;
        }
        if let Some(parsed_context) = parse_context_packet(packet) {
            if let Ok(mut current) = state.lock() {
                let match_changed = current.match_id != Some(parsed_context.match_id);
                let set_changed = current.set_number != Some(parsed_context.set_number);
                let history_changed = if match_changed {
                    let changed = !current.completed_games.is_empty();
                    current.completed_games.clear();
                    changed
                } else if set_changed {
                    seal_completed_game(&mut current)
                } else {
                    false
                };
                let user_side = player_team_id
                    .lock()
                    .ok()
                    .and_then(|known| *known)
                    .and_then(|team_id| {
                        derive_user_side(
                            Some(team_id),
                            Some(parsed_context.blue_team_id),
                            Some(parsed_context.red_team_id),
                        )
                    });
                if match_changed
                    || set_changed
                    || current.blue_team_id != Some(parsed_context.blue_team_id)
                    || current.red_team_id != Some(parsed_context.red_team_id)
                    || current.blue_starters != parsed_context.blue_starters
                    || current.red_starters != parsed_context.red_starters
                    || current.user_side != user_side
                    || history_changed
                {
                    current.context_revision = current.context_revision.wrapping_add(1);
                    current.match_id = Some(parsed_context.match_id);
                    current.set_number = Some(parsed_context.set_number);
                    current.blue_team_id = Some(parsed_context.blue_team_id);
                    current.red_team_id = Some(parsed_context.red_team_id);
                    current.blue_starters = parsed_context.blue_starters;
                    current.red_starters = parsed_context.red_starters;
                    current.user_side = user_side;
                }
            }
            continue;
        }
        let Some(parsed) = parse_packet(packet) else {
            continue;
        };
        if let Ok(mut current) = state.lock() {
            let is_fresh_draft = parsed.blue_bans.is_empty() && parsed.red_bans.is_empty();
            let history_changed = is_fresh_draft && seal_completed_game(&mut current);
            set_phase(&mut current, "draft");
            let merged = retain_through_swap(&current, &parsed);
            if current.blue_bans != merged.blue_bans
                || current.red_bans != merged.red_bans
                || current.blue_picks != merged.blue_picks
                || current.red_picks != merged.red_picks
            {
                current.revision = current.revision.wrapping_add(1);
                current.blue_bans = merged.blue_bans;
                current.red_bans = merged.red_bans;
                current.blue_picks = merged.blue_picks;
                current.red_picks = merged.red_picks;
                crate::performance::event(
                    "coach",
                    "draft_action_received",
                    serde_json::json!({
                        "revision": current.revision,
                        "actionCount": current.blue_bans.len() + current.red_bans.len()
                            + current.blue_picks.len() + current.red_picks.len(),
                    }),
                );
            }
            if history_changed {
                current.context_revision = current.context_revision.wrapping_add(1);
            }
            current.received_at_unix_ms = now_unix_ms();
        }
    }
}

fn seal_completed_game(current: &mut BridgeState) -> bool {
    let Some(game_number) = current.set_number else {
        return false;
    };
    if current.blue_picks.len() != PICKS_PER_SIDE || current.red_picks.len() != PICKS_PER_SIDE {
        return false;
    }
    let next = BridgeGame {
        game_number,
        blue_picks: current.blue_picks.clone(),
        red_picks: current.red_picks.clone(),
    };
    let changed = current
        .completed_games
        .iter()
        .find(|game| game.game_number == game_number)
        .map(|game| game.blue_picks != next.blue_picks || game.red_picks != next.red_picks)
        .unwrap_or(true);
    if !changed {
        return false;
    }
    current
        .completed_games
        .retain(|game| game.game_number != game_number);
    current.completed_games.push(next);
    current.completed_games.sort_by_key(|game| game.game_number);
    true
}

fn derive_user_side(
    player_team_id: Option<usize>,
    blue_team_id: Option<usize>,
    red_team_id: Option<usize>,
) -> Option<String> {
    let player_team_id = player_team_id?;
    if blue_team_id == Some(player_team_id) {
        Some("blue".to_string())
    } else if red_team_id == Some(player_team_id) {
        Some("red".to_string())
    } else {
        None
    }
}

fn set_phase(current: &mut BridgeState, phase: &str) {
    if current.phase != phase {
        current.phase = phase.to_string();
        current.phase_revision = current.phase_revision.wrapping_add(1);
    }
}

struct ParsedDraft {
    blue_bans: Vec<String>,
    red_bans: Vec<String>,
    blue_picks: Vec<String>,
    red_picks: Vec<String>,
}

struct ParsedContext {
    match_id: usize,
    set_number: usize,
    blue_team_id: usize,
    red_team_id: usize,
    blue_starters: Vec<usize>,
    red_starters: Vec<usize>,
}

fn parse_packet(packet: &str) -> Option<ParsedDraft> {
    let fields: Vec<&str> = packet.trim().split('|').collect();
    if fields[0] != "LTAC2" {
        return None;
    }
    let parse = |field: &str| -> Vec<String> {
        field
            .split(',')
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect()
    };
    // Accept the old 3-field shape (bans only) for forward compatibility, and
    // the current 5-field shape (bans + picks).
    match fields.len() {
        3 => Some(ParsedDraft {
            blue_bans: parse(fields[1]),
            red_bans: parse(fields[2]),
            blue_picks: Vec::new(),
            red_picks: Vec::new(),
        }),
        5 => Some(ParsedDraft {
            blue_bans: parse(fields[1]),
            red_bans: parse(fields[2]),
            blue_picks: parse(fields[3]),
            red_picks: parse(fields[4]),
        }),
        _ => None,
    }
}

fn parse_context_packet(packet: &str) -> Option<ParsedContext> {
    let fields: Vec<&str> = packet.trim().split('|').collect();
    if fields.len() != 7 || fields[0] != "LTAC2CTX" {
        return None;
    }
    let parse_starters = |field: &str| -> Option<Vec<usize>> {
        let values: Option<Vec<usize>> = field
            .split(',')
            .filter(|value| !value.is_empty())
            .map(|value| value.parse::<usize>().ok())
            .collect();
        let values = values?;
        (values.len() == 5).then_some(values)
    };
    Some(ParsedContext {
        match_id: fields[1].parse().ok()?,
        set_number: fields[2].parse().ok()?,
        blue_team_id: fields[3].parse().ok()?,
        red_team_id: fields[4].parse().ok()?,
        blue_starters: parse_starters(fields[5])?,
        red_starters: parse_starters(fields[6])?,
    })
}

fn parse_phase_packet(packet: &str) -> Option<&str> {
    let phase = packet.trim().strip_prefix("LTAC2PHASE|")?;
    match phase {
        "stadiumEntrance" | "draft" | "unknown" => Some(phase),
        _ => None,
    }
}

fn parse_rules_packet(packet: &str) -> Option<usize> {
    let bans_per_side = packet.trim().strip_prefix("LTAC2RULES|")?.parse().ok()?;
    (1..=5).contains(&bans_per_side).then_some(bans_per_side)
}

fn parse_mode_packet(packet: &str) -> Option<&str> {
    let mode = packet.trim().strip_prefix("LTAC2MODE|")?;
    matches!(mode, "normal" | "fearless" | "fearless-hard").then_some(mode)
}

fn parse_performance_packet(packet: &str) -> Option<(&str, u64, usize)> {
    let mut fields = packet.trim().split('|');
    if fields.next()? != "LTAC2PERF" {
        return None;
    }
    let action = fields.next()?;
    if action.is_empty() || fields.clone().count() != 2 {
        return None;
    }
    Some((action, fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
}

// Champion-tags packet: `LTAC2TAGS|<id>:<tag>,<tag>;<id>:<tag>...`. Returns the
// parsed map, or None if this isn't a tags packet.
fn parse_tags_packet(packet: &str) -> Option<BTreeMap<String, Vec<String>>> {
    let body = packet.trim().strip_prefix("LTAC2TAGS|")?;
    let mut map = BTreeMap::new();
    for entry in body.split(';').filter(|entry| !entry.is_empty()) {
        let (id, tags) = entry.split_once(':')?;
        if id.is_empty() {
            continue;
        }
        let list = tags
            .split(',')
            .filter(|tag| !tag.is_empty())
            .map(|tag| tag.to_string())
            .collect();
        map.insert(id.to_string(), list);
    }
    Some(map)
}

/// During the swap stage the game hides a side's picks (they read back as `?`),
/// so the mod sends that side's picks as empty. Without this, the board would
/// blank out a comp that is, in fact, still locked — swaps only change roles,
/// never which champions are on a team. So within one draft we let each slot
/// list only grow, keeping the last-known longer list when a packet comes back
/// shorter. A draft always opens with no bans and bans are never hidden, so two
/// empty ban lists uniquely mark a fresh draft and clear anything we were holding.
fn retain_through_swap(current: &BridgeState, parsed: &ParsedDraft) -> ParsedDraft {
    if parsed.blue_bans.is_empty() && parsed.red_bans.is_empty() {
        return ParsedDraft {
            blue_bans: parsed.blue_bans.clone(),
            red_bans: parsed.red_bans.clone(),
            blue_picks: parsed.blue_picks.clone(),
            red_picks: parsed.red_picks.clone(),
        };
    }
    let keep = |held: &[String], incoming: &[String]| {
        if incoming.len() >= held.len() {
            incoming.to_vec()
        } else {
            held.to_vec()
        }
    };
    ParsedDraft {
        blue_bans: keep(&current.blue_bans, &parsed.blue_bans),
        red_bans: keep(&current.red_bans, &parsed.red_bans),
        blue_picks: keep(&current.blue_picks, &parsed.blue_picks),
        red_picks: keep(&current.red_picks, &parsed.red_picks),
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ltac2_bans_and_picks() {
        let parsed = parse_packet(
            "LTAC2|necromancer,ogre,hunter|android,whip_master,gunner|exorcist,shield_bearer|barrier_magician,archer,ninja",
        )
        .expect("packet should parse");
        assert_eq!(parsed.blue_bans, vec!["necromancer", "ogre", "hunter"]);
        assert_eq!(parsed.red_bans, vec!["android", "whip_master", "gunner"]);
        assert_eq!(parsed.blue_picks, vec!["exorcist", "shield_bearer"]);
        assert_eq!(
            parsed.red_picks,
            vec!["barrier_magician", "archer", "ninja"]
        );
    }

    #[test]
    fn preserves_custom_champion_ids() {
        let parsed = parse_packet("LTAC2|||test_mod_fiddlesticks|")
            .expect("custom champion packet should parse");
        assert_eq!(parsed.blue_picks, vec!["test_mod_fiddlesticks"]);
    }

    #[test]
    fn accepts_legacy_bans_only_packet() {
        let parsed = parse_packet("LTAC2|necromancer|android").expect("legacy packet");
        assert_eq!(parsed.blue_bans, vec!["necromancer"]);
        assert_eq!(parsed.red_bans, vec!["android"]);
        assert!(parsed.blue_picks.is_empty());
        assert!(parsed.red_picks.is_empty());
    }

    #[test]
    fn parses_empty_and_rejects_other_packets() {
        let empty = parse_packet("LTAC2||||").expect("empty draft");
        assert!(empty.blue_bans.is_empty() && empty.red_picks.is_empty());
        assert!(parse_packet("LTAC1|pick|1|2|3|4").is_none());
        assert!(parse_packet("garbage").is_none());
    }

    #[test]
    fn parses_phase_packets() {
        assert_eq!(
            parse_phase_packet("LTAC2PHASE|stadiumEntrance"),
            Some("stadiumEntrance")
        );
        assert_eq!(parse_phase_packet("LTAC2PHASE|draft"), Some("draft"));
        assert_eq!(parse_phase_packet("LTAC2PHASE|nonsense"), None);
    }

    #[test]
    fn parses_ban_count_rules() {
        assert_eq!(parse_rules_packet("LTAC2RULES|1"), Some(1));
        assert_eq!(parse_rules_packet("LTAC2RULES|5"), Some(5));
        assert_eq!(parse_rules_packet("LTAC2RULES|0"), None);
        assert_eq!(parse_rules_packet("LTAC2RULES|6"), None);
        assert_eq!(parse_rules_packet("LTAC2RULES|three"), None);
    }

    #[test]
    fn parses_live_draft_modes() {
        assert_eq!(parse_mode_packet("LTAC2MODE|normal"), Some("normal"));
        assert_eq!(parse_mode_packet("LTAC2MODE|fearless"), Some("fearless"));
        assert_eq!(parse_mode_packet("LTAC2MODE|fearless-hard"), Some("fearless-hard"));
        assert_eq!(parse_mode_packet("LTAC2MODE|custom"), None);
    }

    #[test]
    fn parses_bridge_performance_events() {
        assert_eq!(
            parse_performance_packet("LTAC2PERF|draft_action|1250|4"),
            Some(("draft_action", 1250, 4))
        );
        assert_eq!(parse_performance_packet("LTAC2PERF|draft_action|bad|4"), None);
    }

    #[test]
    fn parses_live_match_team_and_starter_context() {
        let parsed = parse_context_packet("LTAC2CTX|0|2|0|4|0,1,2,3,4|36,31,35,37,34")
            .expect("context packet should parse");
        assert_eq!(parsed.match_id, 0);
        assert_eq!(parsed.set_number, 2);
        assert_eq!(parsed.blue_team_id, 0);
        assert_eq!(parsed.red_team_id, 4);
        assert_eq!(parsed.blue_starters, vec![0, 1, 2, 3, 4]);
        assert_eq!(parsed.red_starters, vec![36, 31, 35, 37, 34]);
    }

    #[test]
    fn rejects_incomplete_or_invalid_context() {
        assert!(parse_context_packet("LTAC2CTX|0|2|0|4|0,1|36,31,35,37,34").is_none());
        assert!(parse_context_packet("LTAC2CTX|x|2|0|4|0,1,2,3,4|36,31,35,37,34").is_none());
        assert!(parse_context_packet("LTAC2CTX|0|2|0|4|0,1,2,3,4").is_none());
    }

    #[test]
    fn derives_user_side_from_the_imported_team_id() {
        assert_eq!(
            derive_user_side(Some(42), Some(42), Some(7)).as_deref(),
            Some("blue")
        );
        assert_eq!(
            derive_user_side(Some(42), Some(7), Some(42)).as_deref(),
            Some("red")
        );
        assert_eq!(derive_user_side(Some(42), Some(7), Some(9)), None);
        assert_eq!(derive_user_side(None, Some(42), Some(7)), None);
    }

    #[test]
    fn seals_each_completed_set_once_for_canonical_history() {
        let mut state = BridgeState {
            set_number: Some(1),
            blue_picks: ids(&["a", "b", "c", "d", "e"]),
            red_picks: ids(&["f", "g", "h", "i", "j"]),
            ..BridgeState::default()
        };

        assert!(seal_completed_game(&mut state));
        assert!(!seal_completed_game(&mut state));
        assert_eq!(state.completed_games.len(), 1);
        assert_eq!(state.completed_games[0].game_number, 1);
        assert_eq!(
            state.completed_games[0].blue_picks,
            ids(&["a", "b", "c", "d", "e"])
        );
        assert_eq!(
            state.completed_games[0].red_picks,
            ids(&["f", "g", "h", "i", "j"])
        );
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn held(
        blue_bans: &[&str],
        red_bans: &[&str],
        blue_picks: &[&str],
        red_picks: &[&str],
    ) -> BridgeState {
        BridgeState {
            blue_bans: ids(blue_bans),
            red_bans: ids(red_bans),
            blue_picks: ids(blue_picks),
            red_picks: ids(red_picks),
            ..BridgeState::default()
        }
    }

    fn packet(
        blue_bans: &[&str],
        red_bans: &[&str],
        blue_picks: &[&str],
        red_picks: &[&str],
    ) -> ParsedDraft {
        ParsedDraft {
            blue_bans: ids(blue_bans),
            red_bans: ids(red_bans),
            blue_picks: ids(blue_picks),
            red_picks: ids(red_picks),
        }
    }

    #[test]
    fn growing_picks_are_taken() {
        let current = held(&["a", "b", "c"], &["d", "e", "f"], &["g"], &["h"]);
        let next = packet(&["a", "b", "c"], &["d", "e", "f"], &["g", "i"], &["h", "j"]);
        let merged = retain_through_swap(&current, &next);
        assert_eq!(merged.blue_picks, ids(&["g", "i"]));
        assert_eq!(merged.red_picks, ids(&["h", "j"]));
    }

    #[test]
    fn swap_hidden_picks_are_retained() {
        // Full draft, then the enemy (red) picks read back empty during swap.
        let current = held(
            &["a", "b", "c"],
            &["d", "e", "f"],
            &["g", "h", "i", "j", "k"],
            &["l", "m", "n", "o", "p"],
        );
        let swap = packet(
            &["a", "b", "c"],
            &["d", "e", "f"],
            &["g", "h", "i", "j", "k"],
            &[],
        );
        let merged = retain_through_swap(&current, &swap);
        assert_eq!(merged.red_picks, ids(&["l", "m", "n", "o", "p"]));
        assert_eq!(merged.blue_picks, ids(&["g", "h", "i", "j", "k"]));
    }

    #[test]
    fn empty_bans_reset_everything() {
        let current = held(&["a", "b", "c"], &["d", "e", "f"], &["g", "h"], &["i", "j"]);
        let fresh = packet(&[], &[], &[], &[]);
        let merged = retain_through_swap(&current, &fresh);
        assert!(merged.blue_bans.is_empty() && merged.red_bans.is_empty());
        assert!(merged.blue_picks.is_empty() && merged.red_picks.is_empty());
    }

    #[test]
    fn transient_short_read_does_not_shrink_board() {
        // A one-frame glitch that drops a ban must not erase it.
        let current = held(&["a", "b", "c"], &["d", "e"], &[], &[]);
        let glitch = packet(&["a", "b"], &["d", "e"], &[], &[]);
        let merged = retain_through_swap(&current, &glitch);
        assert_eq!(merged.blue_bans, ids(&["a", "b", "c"]));
    }
}

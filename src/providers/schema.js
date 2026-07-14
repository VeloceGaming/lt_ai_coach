/**
 * Normalized save-data schema (schemaVersion 1).
 *
 * All SaveDataProvider implementations must return an object matching this
 * shape. Downstream modules (statistics, draft, ui) depend only on this
 * contract — never on raw probe output files.
 *
 * Required fields are documented below with their types.
 * Optional fields that may be absent when the probe cannot extract them are
 * marked with a trailing "// optional" comment.
 */

"use strict";

/**
 * @typedef {Object} SaveData
 *
 * Top-level container returned by any SaveDataProvider.
 *
 * @property {number}   schemaVersion        Always 1 for this prototype.
 * @property {string}   saveFile             Absolute path of the source save.
 * @property {string}   extractedAt          ISO-8601 timestamp of extraction.
 * @property {string[]} available_champions  Champion IDs enabled in this save.
 * @property {Player[]} players              Roster of known players.
 * @property {Match[]}  matches              All extracted match records.
 */

/**
 * @typedef {Object} Player
 *
 * @property {string}   id              Internal player identifier.
 * @property {string}   name            Display name.
 * @property {string[]} [favoredRoles]  Roles this player is rated highly for.  // optional
 * @property {Object}   [champStats]    Map of champion id → PlayerChampStat.   // optional
 */

/**
 * @typedef {Object} PlayerChampStat
 *
 * Per-player, per-champion aggregated statistics.
 *
 * @property {number} games
 * @property {number} wins
 * @property {string} [primaryRole]     Role most frequently played.            // optional
 * @property {number} [avgKills]                                                // optional
 * @property {number} [avgDeaths]                                               // optional
 * @property {number} [avgAssists]                                              // optional
 */

/**
 * @typedef {Object} Match
 *
 * A single game record (tournament or solo).
 *
 * @property {string}   id              Unique match identifier.
 * @property {string}   [patch]         Game patch string.                      // optional
 * @property {string}   [matchType]     "tournament" | "solo" | "unknown".      // optional
 * @property {string}   winner          "blue" | "red" | "unknown".
 * @property {TeamDraft} blue           Blue side draft + performance.
 * @property {TeamDraft} red            Red side draft + performance.
 */

/**
 * @typedef {Object} TeamDraft
 *
 * One team's draft choices and performance data for a single match.
 *
 * @property {string[]}       bans       Champion IDs banned (up to 5).
 * @property {PickRecord[]}   picks      Five pick records in draft order.
 */

/**
 * @typedef {Object} PickRecord
 *
 * @property {string}  championId   Champion selected.
 * @property {string}  role         "top" | "jungle" | "mid" | "bot" | "support".
 * @property {string}  [playerId]   Player assigned to this pick.              // optional
 * @property {number}  [kills]                                                 // optional
 * @property {number}  [deaths]                                                // optional
 * @property {number}  [assists]                                               // optional
 * @property {number}  [damage]                                                // optional
 * @property {number}  [tanking]                                               // optional
 * @property {number}  [healing]                                               // optional
 * @property {number}  [gold]                                                  // optional
 * @property {string}  [lane]       Lane assignment when distinct from role.    // optional
 */

/**
 * Construct a minimal valid SaveData shell (useful for testing / fallback).
 *
 * @param {string} saveFile
 * @returns {SaveData}
 */
function emptySaveData(saveFile) {
  return {
    schemaVersion: 1,
    saveFile: saveFile || "",
    extractedAt: new Date().toISOString(),
    available_champions: [],
    players: [],
    matches: [],
  };
}

module.exports = { emptySaveData };

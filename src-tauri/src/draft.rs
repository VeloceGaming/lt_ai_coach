use crate::statistics::ChampionPortrait;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftCatalog {
    pub champions: Vec<DraftChampion>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftChampion {
    pub id: String,
    pub name: String,
    pub portrait: Option<ChampionPortrait>,
    pub role_fit: BTreeMap<String, f64>,
}

pub fn load_draft_catalog(
    database_path: &Path,
    catalog_json: &str,
) -> Result<DraftCatalog, String> {
    crate::champion_registry::load_draft_catalog(database_path, catalog_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn loads_only_enabled_champions() {
        let path = std::env::temp_dir().join(format!(
            "lt-ai-coach-draft-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE enabled_champions (champion_id TEXT PRIMARY KEY);
                 INSERT INTO enabled_champions VALUES ('swordman'), ('archer');",
            )
            .unwrap();
        drop(connection);

        let catalog = load_draft_catalog(
            &path,
            r#"{"champions":[{"id":"swordman"},{"id":"archer"},{"id":"ghost"}]}"#,
        )
        .unwrap();
        assert_eq!(catalog.champions.len(), 2);
        assert_eq!(catalog.champions[0].name, "Archer");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn includes_played_champions_not_in_enabled_list() {
        let path = std::env::temp_dir().join(format!(
            "lt-ai-coach-draft-union-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE enabled_champions (champion_id TEXT PRIMARY KEY);
                 INSERT INTO enabled_champions VALUES ('swordman');
                 CREATE TABLE picks (champion_id TEXT NOT NULL);
                 INSERT INTO picks VALUES ('swordman'), ('dual_blader'), ('dual_blader');",
            )
            .unwrap();
        drop(connection);

        let catalog = load_draft_catalog(
            &path,
            r#"{"champions":[{"id":"swordman"},{"id":"dual_blader"}]}"#,
        )
        .unwrap();
        // dual_blader is only in picks, never in enabled_champions, but is played
        // — so it must still appear in the draft pool.
        let ids: Vec<&str> = catalog.champions.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["dual_blader", "swordman"]);
        fs::remove_file(path).unwrap();
    }
}

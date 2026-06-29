use directories::ProjectDirs;
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};

const VOICE_VAULT_DB_FILE: &str = "voice_vault.db";

pub struct VoiceVaultDb {
    path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceVaultError {
    #[error("cannot resolve app data directory")]
    AppData,
    #[error("failed to create voice vault directory: {0}")]
    CreateDir(std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl VoiceVaultDb {
    pub fn new() -> Result<Self, VoiceVaultError> {
        let proj_dirs =
            ProjectDirs::from("com", "voicewave", "localcore").ok_or(VoiceVaultError::AppData)?;
        Ok(Self::from_path(
            proj_dirs.data_dir().join(VOICE_VAULT_DB_FILE),
        ))
    }

    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn initialize_schema(&self) -> Result<(), VoiceVaultError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(VoiceVaultError::CreateDir)?;
        }

        let connection = Connection::open(&self.path)?;
        connection.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS voice_vault_logs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              timestamp TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              audio_file_path TEXT,
              processing_mode TEXT NOT NULL DEFAULT 'Planning',
              raw_text TEXT NOT NULL DEFAULT '',
              cleaned_text TEXT NOT NULL DEFAULT '',
              transformed_text TEXT NOT NULL DEFAULT '',
              final_edited_text TEXT NOT NULL DEFAULT '',
              transaction_status TEXT NOT NULL DEFAULT 'Accepted'
            );

            PRAGMA user_version = 1;
            "#,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, PartialEq, Eq)]
    struct ColumnInfo {
        name: String,
        column_type: String,
        not_null: bool,
        default_value: Option<String>,
        primary_key: bool,
    }

    fn temp_db_path() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("voice-vault-schema-{ts}.db"))
    }

    fn read_columns(connection: &Connection) -> Vec<ColumnInfo> {
        let mut statement = connection
            .prepare("PRAGMA table_info(voice_vault_logs)")
            .expect("table info should prepare");

        statement
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get(1)?,
                    column_type: row.get(2)?,
                    not_null: row.get::<_, i64>(3)? == 1,
                    default_value: row.get(4)?,
                    primary_key: row.get::<_, i64>(5)? == 1,
                })
            })
            .expect("table info should query")
            .collect::<Result<Vec<_>, _>>()
            .expect("table info should collect")
    }

    #[test]
    fn initialize_schema_creates_voice_vault_logs_table() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);

        vault.initialize_schema().expect("schema should initialize");

        let connection = Connection::open(&db_path).expect("database should open");
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'voice_vault_logs'",
                [],
                |row| row.get(0),
            )
            .expect("table count should query");

        assert_eq!(table_count, 1);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn initialize_schema_is_idempotent_and_has_expected_columns() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);

        vault
            .initialize_schema()
            .expect("first schema initialization should succeed");
        vault
            .initialize_schema()
            .expect("second schema initialization should succeed");

        let connection = Connection::open(&db_path).expect("database should open");
        let columns = read_columns(&connection);

        assert_eq!(
            columns,
            vec![
                ColumnInfo {
                    name: "id".to_string(),
                    column_type: "INTEGER".to_string(),
                    not_null: false,
                    default_value: None,
                    primary_key: true,
                },
                ColumnInfo {
                    name: "timestamp".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("CURRENT_TIMESTAMP".to_string()),
                    primary_key: false,
                },
                ColumnInfo {
                    name: "audio_file_path".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: false,
                    default_value: None,
                    primary_key: false,
                },
                ColumnInfo {
                    name: "processing_mode".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("'Planning'".to_string()),
                    primary_key: false,
                },
                ColumnInfo {
                    name: "raw_text".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("''".to_string()),
                    primary_key: false,
                },
                ColumnInfo {
                    name: "cleaned_text".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("''".to_string()),
                    primary_key: false,
                },
                ColumnInfo {
                    name: "transformed_text".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("''".to_string()),
                    primary_key: false,
                },
                ColumnInfo {
                    name: "final_edited_text".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("''".to_string()),
                    primary_key: false,
                },
                ColumnInfo {
                    name: "transaction_status".to_string(),
                    column_type: "TEXT".to_string(),
                    not_null: true,
                    default_value: Some("'Accepted'".to_string()),
                    primary_key: false,
                },
            ]
        );

        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user version should query");
        assert_eq!(user_version, 1);

        let _ = std::fs::remove_file(&db_path);
    }
}

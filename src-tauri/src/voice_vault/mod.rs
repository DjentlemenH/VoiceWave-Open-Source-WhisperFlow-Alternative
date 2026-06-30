use directories::ProjectDirs;
use rusqlite::{params, Connection};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const VOICE_VAULT_DB_FILE: &str = "voice_vault.db";

#[derive(Debug, Clone)]
pub struct VoiceVaultDb {
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceVaultLogEntry {
    pub audio_file_path: Option<String>,
    pub processing_mode: String,
    pub raw_text: String,
    pub cleaned_text: String,
    pub transformed_text: String,
    pub final_edited_text: String,
    pub transaction_status: String,
}

impl VoiceVaultLogEntry {
    pub fn fallback(raw_text: impl Into<String>, processing_mode: impl Into<String>) -> Self {
        let raw_text = raw_text.into();
        Self {
            audio_file_path: None,
            processing_mode: processing_mode.into(),
            raw_text: raw_text.clone(),
            cleaned_text: raw_text.clone(),
            transformed_text: String::new(),
            final_edited_text: raw_text,
            transaction_status: "Accepted".to_string(),
        }
    }
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

        let connection = self.open_connection()?;
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

    pub fn insert_log(&self, entry: &VoiceVaultLogEntry) -> Result<i64, VoiceVaultError> {
        let connection = self.open_connection()?;
        connection.execute(
            r#"
            INSERT INTO voice_vault_logs (
              audio_file_path,
              processing_mode,
              raw_text,
              cleaned_text,
              transformed_text,
              final_edited_text,
              transaction_status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                normalize_optional_text(entry.audio_file_path.as_deref()),
                entry.processing_mode.trim(),
                &entry.raw_text,
                &entry.cleaned_text,
                &entry.transformed_text,
                &entry.final_edited_text,
                entry.transaction_status.trim(),
            ],
        )?;

        Ok(connection.last_insert_rowid())
    }

    pub fn update_log(&self, id: i64, entry: &VoiceVaultLogEntry) -> Result<(), VoiceVaultError> {
        let connection = self.open_connection()?;
        connection.execute(
            r#"
            UPDATE voice_vault_logs
            SET
              audio_file_path = ?1,
              processing_mode = ?2,
              raw_text = ?3,
              cleaned_text = ?4,
              transformed_text = ?5,
              final_edited_text = ?6,
              transaction_status = ?7
            WHERE id = ?8
            "#,
            params![
                normalize_optional_text(entry.audio_file_path.as_deref()),
                entry.processing_mode.trim(),
                &entry.raw_text,
                &entry.cleaned_text,
                &entry.transformed_text,
                &entry.final_edited_text,
                entry.transaction_status.trim(),
                id,
            ],
        )?;

        Ok(())
    }

    pub fn update_audio_file_path(
        &self,
        id: i64,
        audio_file_path: &str,
    ) -> Result<(), VoiceVaultError> {
        let connection = self.open_connection()?;
        connection.execute(
            "UPDATE voice_vault_logs SET audio_file_path = ?1 WHERE id = ?2",
            params![normalize_optional_text(Some(audio_file_path)), id],
        )?;
        Ok(())
    }

    pub fn clear_audio_file_path(&self, id: i64) -> Result<(), VoiceVaultError> {
        let connection = self.open_connection()?;
        connection.execute(
            "UPDATE voice_vault_logs SET audio_file_path = NULL WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn rejected_audio_paths_older_than_hours(
        &self,
        hours: i64,
    ) -> Result<Vec<(i64, String)>, VoiceVaultError> {
        let connection = self.open_connection()?;
        let threshold = format!("-{} hours", hours.max(0));
        let mut statement = connection.prepare(
            r#"
            SELECT id, audio_file_path
            FROM voice_vault_logs
            WHERE transaction_status = 'Rejected'
              AND audio_file_path IS NOT NULL
              AND TRIM(audio_file_path) <> ''
              AND timestamp <= datetime('now', ?1)
            "#,
        )?;
        let rows = statement.query_map([threshold], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(VoiceVaultError::Sqlite)
    }

    pub fn get_log(&self, id: i64) -> Result<Option<VoiceVaultLogEntry>, VoiceVaultError> {
        let connection = self.open_connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT
              audio_file_path,
              processing_mode,
              raw_text,
              cleaned_text,
              transformed_text,
              final_edited_text,
              transaction_status
            FROM voice_vault_logs
            WHERE id = ?1
            "#,
        )?;
        let mut rows = statement.query([id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        Ok(Some(VoiceVaultLogEntry {
            audio_file_path: row.get(0)?,
            processing_mode: row.get(1)?,
            raw_text: row.get(2)?,
            cleaned_text: row.get(3)?,
            transformed_text: row.get(4)?,
            final_edited_text: row.get(5)?,
            transaction_status: row.get(6)?,
        }))
    }

    fn open_connection(&self) -> Result<Connection, VoiceVaultError> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Ok(connection)
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
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

    #[test]
    fn insert_log_maps_refinement_fields_to_phase0_columns() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        let id = vault
            .insert_log(&VoiceVaultLogEntry {
                audio_file_path: Some("  H:\\clips\\one.wav  ".to_string()),
                processing_mode: "Code".to_string(),
                raw_text: "raw".to_string(),
                cleaned_text: "cleaned".to_string(),
                transformed_text: "transformed".to_string(),
                final_edited_text: "final".to_string(),
                transaction_status: "Accepted".to_string(),
            })
            .expect("log should insert");

        let connection = Connection::open(&db_path).expect("database should open");
        let row = connection
            .query_row(
                r#"
                SELECT
                  id,
                  audio_file_path,
                  processing_mode,
                  raw_text,
                  cleaned_text,
                  transformed_text,
                  final_edited_text,
                  transaction_status
                FROM voice_vault_logs
                WHERE id = ?1
                "#,
                [id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .expect("row should query");

        assert_eq!(
            row,
            (
                id,
                Some("H:\\clips\\one.wav".to_string()),
                "Code".to_string(),
                "raw".to_string(),
                "cleaned".to_string(),
                "transformed".to_string(),
                "final".to_string(),
                "Accepted".to_string(),
            )
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn get_log_returns_inserted_entry_for_retry() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");
        let entry = VoiceVaultLogEntry {
            audio_file_path: Some("H:\\clips\\retry.wav".to_string()),
            processing_mode: "Planning".to_string(),
            raw_text: "raw plan".to_string(),
            cleaned_text: "clean plan".to_string(),
            transformed_text: "structured plan".to_string(),
            final_edited_text: "structured plan".to_string(),
            transaction_status: "Pending".to_string(),
        };
        let id = vault.insert_log(&entry).expect("entry should insert");

        let loaded = vault
            .get_log(id)
            .expect("entry should query")
            .expect("entry should exist");

        assert_eq!(loaded, entry);
        assert_eq!(
            vault.get_log(id + 1).expect("missing query should work"),
            None
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn fallback_log_insertions_are_repeatable_without_schema_changes() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        let first = vault
            .insert_log(&VoiceVaultLogEntry::fallback("hello\nworld", "Raw"))
            .expect("first fallback should insert");
        let second = vault
            .insert_log(&VoiceVaultLogEntry::fallback("hello\nworld", "Raw"))
            .expect("second fallback should insert");

        assert_ne!(first, second);

        let connection = Connection::open(&db_path).expect("database should open");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM voice_vault_logs", [], |row| {
                row.get(0)
            })
            .expect("count should query");
        assert_eq!(count, 2);

        let _ = std::fs::remove_file(&db_path);
    }
}

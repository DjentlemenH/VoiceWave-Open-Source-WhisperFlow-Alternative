use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const VOICE_VAULT_DB_FILE: &str = "voice_vault.db";

pub struct VoiceVaultDb {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProcessingMode {
    Code,
    Planning,
    Reply,
}

impl Default for ProcessingMode {
    fn default() -> Self {
        Self::Planning
    }
}

impl ProcessingMode {
    fn as_db_str(&self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Planning => "Planning",
            Self::Reply => "Reply",
        }
    }

    fn from_db_str(value: String) -> Result<Self, VoiceVaultError> {
        match value.as_str() {
            "Code" => Ok(Self::Code),
            "Planning" => Ok(Self::Planning),
            "Reply" => Ok(Self::Reply),
            _ => Err(VoiceVaultError::InvalidProcessingMode(value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TransactionStatus {
    Accepted,
    Edited,
    Rejected,
}

impl Default for TransactionStatus {
    fn default() -> Self {
        Self::Accepted
    }
}

impl TransactionStatus {
    fn as_db_str(&self) -> &'static str {
        match self {
            Self::Accepted => "Accepted",
            Self::Edited => "Edited",
            Self::Rejected => "Rejected",
        }
    }

    fn from_db_str(value: String) -> Result<Self, VoiceVaultError> {
        match value.as_str() {
            "Accepted" => Ok(Self::Accepted),
            "Edited" => Ok(Self::Edited),
            "Rejected" => Ok(Self::Rejected),
            _ => Err(VoiceVaultError::InvalidTransactionStatus(value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceVaultLog {
    pub id: i64,
    pub timestamp: String,
    pub audio_file_path: Option<String>,
    pub processing_mode: ProcessingMode,
    pub raw_text: String,
    pub cleaned_text: String,
    pub transformed_text: String,
    pub final_edited_text: String,
    pub transaction_status: TransactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateVoiceVaultLogRequest {
    pub audio_file_path: Option<String>,
    pub processing_mode: Option<ProcessingMode>,
    pub raw_text: Option<String>,
    pub cleaned_text: Option<String>,
    pub transformed_text: Option<String>,
    pub final_edited_text: Option<String>,
    pub transaction_status: Option<TransactionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVoiceVaultLogRequest {
    pub cleaned_text: Option<String>,
    pub transformed_text: Option<String>,
    pub final_edited_text: Option<String>,
    pub transaction_status: Option<TransactionStatus>,
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceVaultError {
    #[error("cannot resolve app data directory")]
    AppData,
    #[error("failed to create voice vault directory: {0}")]
    CreateDir(std::io::Error),
    #[error("voice vault log not found: {0}")]
    LogNotFound(i64),
    #[error("invalid processing mode in database: {0}")]
    InvalidProcessingMode(String),
    #[error("invalid transaction status in database: {0}")]
    InvalidTransactionStatus(String),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

type VoiceVaultLogRow = (
    i64,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    String,
);

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

    pub fn create_log(
        &self,
        request: CreateVoiceVaultLogRequest,
    ) -> Result<VoiceVaultLog, VoiceVaultError> {
        let connection = self.open_connection()?;
        let processing_mode = request.processing_mode.unwrap_or_default();
        let transaction_status = request.transaction_status.unwrap_or_default();
        let audio_file_path = normalize_optional_text(request.audio_file_path);

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
                audio_file_path,
                processing_mode.as_db_str(),
                request.raw_text.unwrap_or_default(),
                request.cleaned_text.unwrap_or_default(),
                request.transformed_text.unwrap_or_default(),
                request.final_edited_text.unwrap_or_default(),
                transaction_status.as_db_str(),
            ],
        )?;

        self.get_log_with_connection(&connection, connection.last_insert_rowid())
    }

    pub fn update_log(
        &self,
        id: i64,
        request: UpdateVoiceVaultLogRequest,
    ) -> Result<VoiceVaultLog, VoiceVaultError> {
        let connection = self.open_connection()?;
        let transaction_status = request
            .transaction_status
            .as_ref()
            .map(TransactionStatus::as_db_str);

        let changed_rows = connection.execute(
            r#"
            UPDATE voice_vault_logs
            SET
              cleaned_text = COALESCE(?1, cleaned_text),
              transformed_text = COALESCE(?2, transformed_text),
              final_edited_text = COALESCE(?3, final_edited_text),
              transaction_status = COALESCE(?4, transaction_status)
            WHERE id = ?5
            "#,
            params![
                request.cleaned_text.as_deref(),
                request.transformed_text.as_deref(),
                request.final_edited_text.as_deref(),
                transaction_status,
                id,
            ],
        )?;
        if changed_rows == 0 {
            return Err(VoiceVaultError::LogNotFound(id));
        }

        self.get_log_with_connection(&connection, id)
    }

    pub fn get_log(&self, id: i64) -> Result<VoiceVaultLog, VoiceVaultError> {
        let connection = self.open_connection()?;
        self.get_log_with_connection(&connection, id)
    }

    pub fn list_logs(&self, limit: Option<usize>) -> Result<Vec<VoiceVaultLog>, VoiceVaultError> {
        let connection = self.open_connection()?;
        let limit = limit.unwrap_or(50).clamp(1, 200) as i64;
        let mut statement = connection.prepare(
            r#"
            SELECT
              id,
              timestamp,
              audio_file_path,
              processing_mode,
              raw_text,
              cleaned_text,
              transformed_text,
              final_edited_text,
              transaction_status
            FROM voice_vault_logs
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )?;

        let rows = statement
            .query_map(params![limit], read_log_row)?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter().map(log_from_row).collect()
    }

    fn open_connection(&self) -> Result<Connection, VoiceVaultError> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Ok(connection)
    }

    fn get_log_with_connection(
        &self,
        connection: &Connection,
        id: i64,
    ) -> Result<VoiceVaultLog, VoiceVaultError> {
        let row = connection
            .query_row(
                r#"
                SELECT
                  id,
                  timestamp,
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
                params![id],
                read_log_row,
            )
            .optional()?
            .ok_or(VoiceVaultError::LogNotFound(id))?;

        log_from_row(row)
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn read_log_row(row: &Row<'_>) -> rusqlite::Result<VoiceVaultLogRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn log_from_row(row: VoiceVaultLogRow) -> Result<VoiceVaultLog, VoiceVaultError> {
    Ok(VoiceVaultLog {
        id: row.0,
        timestamp: row.1,
        audio_file_path: row.2,
        processing_mode: ProcessingMode::from_db_str(row.3)?,
        raw_text: row.4,
        cleaned_text: row.5,
        transformed_text: row.6,
        final_edited_text: row.7,
        transaction_status: TransactionStatus::from_db_str(row.8)?,
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
    fn create_log_uses_schema_defaults_and_normalizes_empty_audio_path() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        let log = vault
            .create_log(CreateVoiceVaultLogRequest {
                audio_file_path: Some("   ".to_string()),
                ..CreateVoiceVaultLogRequest::default()
            })
            .expect("log should create");

        assert_eq!(log.id, 1);
        assert!(!log.timestamp.is_empty());
        assert_eq!(log.audio_file_path, None);
        assert_eq!(log.processing_mode, ProcessingMode::Planning);
        assert_eq!(log.raw_text, "");
        assert_eq!(log.cleaned_text, "");
        assert_eq!(log.transformed_text, "");
        assert_eq!(log.final_edited_text, "");
        assert_eq!(log.transaction_status, TransactionStatus::Accepted);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn create_log_accepts_all_stage_fields() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        let log = vault
            .create_log(CreateVoiceVaultLogRequest {
                audio_file_path: Some("  H:\\clips\\one.wav  ".to_string()),
                processing_mode: Some(ProcessingMode::Code),
                raw_text: Some("raw".to_string()),
                cleaned_text: Some("cleaned".to_string()),
                transformed_text: Some("transformed".to_string()),
                final_edited_text: Some("final".to_string()),
                transaction_status: Some(TransactionStatus::Edited),
            })
            .expect("log should create");

        assert_eq!(log.audio_file_path, Some("H:\\clips\\one.wav".to_string()));
        assert_eq!(log.processing_mode, ProcessingMode::Code);
        assert_eq!(log.raw_text, "raw");
        assert_eq!(log.cleaned_text, "cleaned");
        assert_eq!(log.transformed_text, "transformed");
        assert_eq!(log.final_edited_text, "final");
        assert_eq!(log.transaction_status, TransactionStatus::Edited);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn update_log_patches_editable_fields_without_implicit_status_change() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");
        let log = vault
            .create_log(CreateVoiceVaultLogRequest {
                raw_text: Some("raw".to_string()),
                cleaned_text: Some("old cleaned".to_string()),
                transformed_text: Some("old transformed".to_string()),
                final_edited_text: Some("old final".to_string()),
                transaction_status: Some(TransactionStatus::Accepted),
                ..CreateVoiceVaultLogRequest::default()
            })
            .expect("log should create");

        let edited = vault
            .update_log(
                log.id,
                UpdateVoiceVaultLogRequest {
                    cleaned_text: Some("new cleaned".to_string()),
                    final_edited_text: Some("new final".to_string()),
                    ..UpdateVoiceVaultLogRequest::default()
                },
            )
            .expect("log should update");

        assert_eq!(edited.raw_text, "raw");
        assert_eq!(edited.cleaned_text, "new cleaned");
        assert_eq!(edited.transformed_text, "old transformed");
        assert_eq!(edited.final_edited_text, "new final");
        assert_eq!(edited.transaction_status, TransactionStatus::Accepted);

        let rejected = vault
            .update_log(
                log.id,
                UpdateVoiceVaultLogRequest {
                    transaction_status: Some(TransactionStatus::Rejected),
                    ..UpdateVoiceVaultLogRequest::default()
                },
            )
            .expect("status should update");
        assert_eq!(rejected.transaction_status, TransactionStatus::Rejected);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn get_log_returns_not_found_for_missing_id() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        let result = vault.get_log(404);

        assert!(matches!(result, Err(VoiceVaultError::LogNotFound(404))));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn update_log_returns_not_found_for_missing_id() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        let result = vault.update_log(
            404,
            UpdateVoiceVaultLogRequest {
                cleaned_text: Some("missing".to_string()),
                ..UpdateVoiceVaultLogRequest::default()
            },
        );

        assert!(matches!(result, Err(VoiceVaultError::LogNotFound(404))));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn list_logs_orders_newest_first_and_clamps_limit() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");

        for i in 0..205 {
            vault
                .create_log(CreateVoiceVaultLogRequest {
                    raw_text: Some(format!("raw {i}")),
                    ..CreateVoiceVaultLogRequest::default()
                })
                .expect("log should create");
        }

        let newest_two = vault.list_logs(Some(2)).expect("logs should list");
        assert_eq!(newest_two.len(), 2);
        assert_eq!(newest_two[0].id, 205);
        assert_eq!(newest_two[1].id, 204);

        let clamped_min = vault.list_logs(Some(0)).expect("logs should list");
        assert_eq!(clamped_min.len(), 1);

        let clamped_max = vault.list_logs(Some(usize::MAX)).expect("logs should list");
        assert_eq!(clamped_max.len(), 200);

        let default_limit = vault.list_logs(None).expect("logs should list");
        assert_eq!(default_limit.len(), 50);

        let _ = std::fs::remove_file(&db_path);
    }
}

use crate::voice_vault::{VoiceVaultDb, VoiceVaultError};
use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DEFAULT_ARCHIVE_ROOT: &str = r"H:\VoiceVault";
const AUDIO_DIR_NAME: &str = "Audio";
const REJECTED_RETENTION_HOURS: i64 = 24;

#[derive(Debug, Clone)]
pub struct AudioArchive {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveAudioPlan {
    pub relative_path: String,
    pub absolute_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupSummary {
    pub deleted_files: usize,
    pub cleared_logs: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioArchiveError {
    #[error("audio archive path must be relative and confined to the archive root")]
    UnsafePath,
    #[error("failed to create audio archive directory: {0}")]
    CreateDir(io::Error),
    #[error("failed to write audio archive file: {0}")]
    Write(io::Error),
    #[error("voice vault error: {0}")]
    VoiceVault(#[from] VoiceVaultError),
}

impl AudioArchive {
    pub fn new() -> Self {
        Self::from_root(default_archive_root())
    }

    pub fn from_root(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn plan_for_session(&self, session_id: u64, timestamp_utc_ms: u64) -> ArchiveAudioPlan {
        let file_name = format!("voicewave-{session_id}-{timestamp_utc_ms}.wav");
        let relative_path = format!("{AUDIO_DIR_NAME}\\{file_name}");
        ArchiveAudioPlan {
            absolute_path: self.root.join(AUDIO_DIR_NAME).join(file_name),
            relative_path,
        }
    }

    pub fn absolute_path_for_relative(
        &self,
        relative_path: &str,
    ) -> Result<PathBuf, AudioArchiveError> {
        let relative = Path::new(relative_path);
        if relative.is_absolute() {
            return Err(AudioArchiveError::UnsafePath);
        }
        let mut out = self.root.clone();
        for component in relative.components() {
            match component {
                Component::Normal(part) => out.push(part),
                Component::CurDir => {}
                _ => return Err(AudioArchiveError::UnsafePath),
            }
        }
        Ok(out)
    }

    pub fn write_wav_file(
        &self,
        plan: &ArchiveAudioPlan,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<(), AudioArchiveError> {
        if let Some(parent) = plan.absolute_path.parent() {
            fs::create_dir_all(parent).map_err(AudioArchiveError::CreateDir)?;
        }
        write_pcm16_wav(&plan.absolute_path, samples, sample_rate).map_err(AudioArchiveError::Write)
    }

    pub fn cleanup_rejected_audio(
        &self,
        voice_vault: &VoiceVaultDb,
    ) -> Result<CleanupSummary, AudioArchiveError> {
        let rejected_paths =
            voice_vault.rejected_audio_paths_older_than_hours(REJECTED_RETENTION_HOURS)?;
        let mut deleted_files = 0usize;
        let mut cleared_logs = 0usize;

        for (log_id, relative_path) in rejected_paths {
            let absolute_path = match self.absolute_path_for_relative(&relative_path) {
                Ok(path) => path,
                Err(err) => {
                    eprintln!(
                        "voicewave: skipped unsafe archived audio path for log {log_id}: {err}"
                    );
                    continue;
                }
            };

            match fs::remove_file(&absolute_path) {
                Ok(()) => {
                    deleted_files += 1;
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => {
                    eprintln!(
                        "voicewave: failed to delete rejected archived audio {:?}: {err}",
                        absolute_path
                    );
                    continue;
                }
            }
            voice_vault.clear_audio_file_path(log_id)?;
            cleared_logs += 1;
        }

        Ok(CleanupSummary {
            deleted_files,
            cleared_logs,
        })
    }
}

impl Default for AudioArchive {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn_archive_audio_for_log(
    archive: AudioArchive,
    voice_vault: VoiceVaultDb,
    log_id: i64,
    plan: ArchiveAudioPlan,
    samples: Vec<f32>,
    sample_rate: u32,
) {
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(err) =
            archive_audio_for_log(&archive, &voice_vault, log_id, &plan, &samples, sample_rate)
        {
            eprintln!("voicewave: audio archive failed for log {log_id}: {err}");
        }
    });
}

fn archive_audio_for_log(
    archive: &AudioArchive,
    voice_vault: &VoiceVaultDb,
    log_id: i64,
    plan: &ArchiveAudioPlan,
    samples: &[f32],
    sample_rate: u32,
) -> Result<(), AudioArchiveError> {
    let result = archive
        .write_wav_file(plan, samples, sample_rate)
        .and_then(|()| {
            voice_vault
                .update_audio_file_path(log_id, &plan.relative_path)
                .map_err(AudioArchiveError::VoiceVault)
        });

    if result.is_err() {
        let _ = voice_vault.update_transaction_status(log_id, "ArchiveFailed");
    }

    result
}

pub fn spawn_rejected_audio_cleanup(archive: AudioArchive, voice_vault: VoiceVaultDb) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let _ = handle.spawn_blocking(move || match archive.cleanup_rejected_audio(&voice_vault) {
            Ok(summary) if summary.deleted_files > 0 || summary.cleared_logs > 0 => {
                eprintln!(
                    "voicewave: cleaned {} rejected archived audio files and {} log links",
                    summary.deleted_files, summary.cleared_logs
                );
            }
            Ok(_) => {}
            Err(err) => eprintln!("voicewave: rejected audio cleanup failed: {err}"),
        });
    }
}

pub fn now_utc_ms_for_archive() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

fn default_archive_root() -> PathBuf {
    PathBuf::from(DEFAULT_ARCHIVE_ROOT)
}

fn write_pcm16_wav(path: &Path, samples: &[f32], sample_rate: u32) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    let data_bytes = samples.len().saturating_mul(2) as u32;
    let chunk_size = 36u32.saturating_add(data_bytes);

    file.write_all(b"RIFF")?;
    file.write_all(&chunk_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&sample_rate.saturating_mul(2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;

    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        file.write_all(&pcm.to_le_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_vault::VoiceVaultLogEntry;
    use rusqlite::Connection;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("voicewave-audio-archive-{}", unique_test_suffix()))
    }

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "voicewave-audio-archive-{}.db",
            unique_test_suffix()
        ))
    }

    fn unique_test_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos()
    }

    #[test]
    fn plan_uses_expected_h_drive_audio_root() {
        let archive = AudioArchive::new();
        let plan = archive.plan_for_session(42, 1_782_836_000_000);
        assert_eq!(
            plan.absolute_path,
            PathBuf::from(r"H:\VoiceVault\Audio\voicewave-42-1782836000000.wav")
        );
        assert_eq!(plan.relative_path, r"Audio\voicewave-42-1782836000000.wav");
    }

    #[test]
    fn writes_pcm16_wav_to_planned_audio_path() {
        let root = temp_root();
        let archive = AudioArchive::from_root(&root);
        let plan = archive.plan_for_session(7, 1234);

        archive
            .write_wav_file(&plan, &[0.0, 0.5, -0.5], 16_000)
            .expect("wav should write");

        let bytes = fs::read(&plan.absolute_path).expect("wav should be readable");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sqlite_audio_file_path_stores_relative_archive_path() {
        let db_path = temp_db_path();
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");
        let log_id = vault
            .insert_log(&VoiceVaultLogEntry::fallback("hello", "Raw"))
            .expect("log should insert");

        vault
            .update_audio_file_path(log_id, r"Audio\voicewave-1-2.wav")
            .expect("relative path should update");

        let connection = Connection::open(&db_path).expect("db should open");
        let stored: String = connection
            .query_row(
                "SELECT audio_file_path FROM voice_vault_logs WHERE id = ?1",
                [log_id],
                |row| row.get(0),
            )
            .expect("path should exist");
        assert_eq!(stored, r"Audio\voicewave-1-2.wav");
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn cleanup_removes_old_rejected_audio_and_clears_log_path() {
        let root = temp_root();
        let db_path = temp_db_path();
        let archive = AudioArchive::from_root(&root);
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");
        let plan = archive.plan_for_session(8, 5678);
        archive
            .write_wav_file(&plan, &[0.25, -0.25], 16_000)
            .expect("wav should write");

        let mut entry = VoiceVaultLogEntry::fallback("rejected text", "Raw");
        entry.audio_file_path = Some(plan.relative_path.clone());
        entry.transaction_status = "Rejected".to_string();
        let log_id = vault.insert_log(&entry).expect("log should insert");
        let connection = Connection::open(&db_path).expect("db should open");
        connection
            .execute(
                "UPDATE voice_vault_logs SET timestamp = '2000-01-01 00:00:00' WHERE id = ?1",
                [log_id],
            )
            .expect("timestamp should update");

        let summary = archive
            .cleanup_rejected_audio(&vault)
            .expect("cleanup should succeed");

        assert_eq!(summary.deleted_files, 1);
        assert_eq!(summary.cleared_logs, 1);
        assert!(!plan.absolute_path.exists());
        let stored: Option<String> = connection
            .query_row(
                "SELECT audio_file_path FROM voice_vault_logs WHERE id = ?1",
                [log_id],
                |row| row.get(0),
            )
            .expect("path should read");
        assert_eq!(stored, None);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn archive_failure_marks_log_status_without_panicking() {
        let root = temp_root();
        fs::write(&root, b"not a directory").expect("root file should write");
        let db_path = temp_db_path();
        let archive = AudioArchive::from_root(&root);
        let vault = VoiceVaultDb::from_path(&db_path);
        vault.initialize_schema().expect("schema should initialize");
        let log_id = vault
            .insert_log(&VoiceVaultLogEntry::fallback("hello", "Raw"))
            .expect("log should insert");
        let plan = archive.plan_for_session(9, 9012);

        let result = archive_audio_for_log(&archive, &vault, log_id, &plan, &[0.0], 16_000);

        assert!(result.is_err());
        let entry = vault
            .get_log(log_id)
            .expect("log should read")
            .expect("log should exist");
        assert_eq!(entry.transaction_status, "ArchiveFailed");
        assert_eq!(entry.audio_file_path, None);
        let _ = fs::remove_file(root);
        let _ = fs::remove_file(db_path);
    }
}

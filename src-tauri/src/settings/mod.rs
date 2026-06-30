use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DecodeMode {
    #[default]
    Balanced,
    Fast,
    Quality,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FormatProfile {
    #[default]
    Default,
    Academic,
    Technical,
    Concise,
    CodeDoc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DomainPackId {
    Coding,
    Student,
    Productivity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppTargetClass {
    #[default]
    Editor,
    Browser,
    Collab,
    Desktop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppProfileBehavior {
    pub punctuation_aggressiveness: u8,
    pub sentence_compactness: u8,
    pub auto_list_formatting: bool,
}

impl Default for AppProfileBehavior {
    fn default() -> Self {
        Self {
            punctuation_aggressiveness: 1,
            sentence_compactness: 1,
            auto_list_formatting: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppProfileOverrides {
    pub active_target: AppTargetClass,
    pub editor: AppProfileBehavior,
    pub browser: AppProfileBehavior,
    pub collab: AppProfileBehavior,
    pub desktop: AppProfileBehavior,
}

impl Default for AppProfileOverrides {
    fn default() -> Self {
        Self {
            active_target: AppTargetClass::Editor,
            editor: AppProfileBehavior {
                punctuation_aggressiveness: 2,
                sentence_compactness: 1,
                auto_list_formatting: true,
            },
            browser: AppProfileBehavior {
                punctuation_aggressiveness: 1,
                sentence_compactness: 1,
                auto_list_formatting: false,
            },
            collab: AppProfileBehavior {
                punctuation_aggressiveness: 1,
                sentence_compactness: 2,
                auto_list_formatting: true,
            },
            desktop: AppProfileBehavior::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum CodeCasingStyle {
    #[default]
    Preserve,
    CamelCase,
    SnakeCase,
    PascalCase,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct CodeModeSettings {
    pub enabled: bool,
    pub spoken_symbols: bool,
    pub preferred_casing: CodeCasingStyle,
    pub wrap_in_fenced_block: bool,
}

impl Default for CodeModeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            spoken_symbols: true,
            preferred_casing: CodeCasingStyle::Preserve,
            wrap_in_fenced_block: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptRefinementProviderKind {
    #[default]
    Disabled,
    Ollama,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptRefinementMode {
    #[default]
    Raw,
    Clean,
    Planning,
    Code,
    Reply,
    DetailedNotes,
}

impl TranscriptRefinementMode {
    pub fn processing_mode(self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Clean => "Clean",
            Self::Planning => "Planning",
            Self::Code => "Code",
            Self::Reply => "Clean Reply",
            Self::DetailedNotes => "Detailed Notes",
        }
    }

    pub fn from_workflow_key(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace(['_', '-'], " ");
        match normalized
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_str()
        {
            "raw" => Some(Self::Raw),
            "clean" | "clean reply" | "reply" => Some(Self::Reply),
            "planning" | "plan" => Some(Self::Planning),
            "code" | "agent prompt" | "code agent prompt" => Some(Self::Code),
            "detailed notes" | "notes" | "detailed" => Some(Self::DetailedNotes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkflowPromptPreset {
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_ms: u64,
}

impl WorkflowPromptPreset {
    fn new(system_prompt: &str, temperature: f32, max_tokens: u32, timeout_ms: u64) -> Self {
        Self {
            system_prompt: system_prompt.to_string(),
            temperature,
            max_tokens,
            timeout_ms,
        }
    }
}

impl Default for WorkflowPromptPreset {
    fn default() -> Self {
        Self::new(CLEAN_REPLY_SYSTEM_PROMPT, 0.2, 768, 2_500)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkflowPromptPresets {
    pub clean_reply: WorkflowPromptPreset,
    pub planning: WorkflowPromptPreset,
    pub code: WorkflowPromptPreset,
    pub detailed_notes: WorkflowPromptPreset,
}

impl Default for WorkflowPromptPresets {
    fn default() -> Self {
        Self {
            clean_reply: WorkflowPromptPreset::new(CLEAN_REPLY_SYSTEM_PROMPT, 0.2, 768, 2_500),
            planning: WorkflowPromptPreset::new(PLANNING_SYSTEM_PROMPT, 0.2, 1_024, 3_500),
            code: WorkflowPromptPreset::new(CODE_SYSTEM_PROMPT, 0.1, 1_024, 3_500),
            detailed_notes: WorkflowPromptPreset::new(
                DETAILED_NOTES_SYSTEM_PROMPT,
                0.2,
                1_536,
                4_000,
            ),
        }
    }
}

impl WorkflowPromptPresets {
    pub fn preset_for_mode(&self, mode: TranscriptRefinementMode) -> Option<&WorkflowPromptPreset> {
        match mode {
            TranscriptRefinementMode::Raw => None,
            TranscriptRefinementMode::Clean | TranscriptRefinementMode::Reply => {
                Some(&self.clean_reply)
            }
            TranscriptRefinementMode::Planning => Some(&self.planning),
            TranscriptRefinementMode::Code => Some(&self.code),
            TranscriptRefinementMode::DetailedNotes => Some(&self.detailed_notes),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TranscriptRefinementSettings {
    pub enabled: bool,
    pub provider: TranscriptRefinementProviderKind,
    pub mode: TranscriptRefinementMode,
    pub automatic_profile_switching_enabled: bool,
    pub requires_manual_approval: bool,
    pub endpoint_url: String,
    pub model: String,
    pub timeout_ms: u64,
    pub system_prompt: Option<String>,
    pub workflow_presets: WorkflowPromptPresets,
}

impl Default for TranscriptRefinementSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: TranscriptRefinementProviderKind::Disabled,
            mode: TranscriptRefinementMode::Raw,
            automatic_profile_switching_enabled: false,
            requires_manual_approval: false,
            endpoint_url: "http://127.0.0.1:11434/v1/chat/completions".to_string(),
            model: "llama3.2:3b".to_string(),
            timeout_ms: 2_500,
            system_prompt: None,
            workflow_presets: WorkflowPromptPresets::default(),
        }
    }
}

pub const CLEAN_REPLY_SYSTEM_PROMPT: &str = "You are refining local dictation for a clean reply. Fix punctuation, remove verbal stutters and filler words, and correct obvious text anomalies. Strictly preserve slang, raw terminology, profanity, and Hunter's personal conversational tone. Never summarize. Return only valid JSON with keys cleaned_text, transformed_text, and final_edited_text.";

pub const PLANNING_SYSTEM_PROMPT: &str = "You are refining local dictation into a planning note. Convert spoken thought streams into a clean structured markdown block with exactly these headers: Objective, Context, Decisions, Tasks, Unresolved Questions, Next Action. Preserve all substantive details and do not invent decisions. Return only valid JSON with keys cleaned_text, transformed_text, and final_edited_text.";

pub const CODE_SYSTEM_PROMPT: &str = "You are refining local dictation into an actionable code or agent prompt. Protect literal Windows file paths, command flags, versions, identifiers, quoted strings, and precise technical jargon exactly as spoken. Do not normalize or reinterpret technical tokens. Return only valid JSON with keys cleaned_text, transformed_text, and final_edited_text.";

pub const DETAILED_NOTES_SYSTEM_PROMPT: &str = "You are refining local dictation into detailed notes. Organize dense information into structured headings and clean text blocks while completely retaining all original substantive content. Do not summarize away details. Return only valid JSON with keys cleaned_text, transformed_text, and final_edited_text.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceWaveSettings {
    pub input_device: Option<String>,
    pub active_model: String,
    pub show_floating_hud: bool,
    pub vad_threshold: f32,
    pub max_utterance_ms: u64,
    pub release_tail_ms: u64,
    pub decode_mode: DecodeMode,
    pub diagnostics_opt_in: bool,
    pub toggle_hotkey: String,
    pub push_to_talk_hotkey: String,
    pub prefer_clipboard_fallback: bool,
    pub format_profile: FormatProfile,
    pub active_domain_packs: Vec<DomainPackId>,
    pub app_profile_overrides: AppProfileOverrides,
    pub code_mode: CodeModeSettings,
    pub pro_post_processing_enabled: bool,
    pub prefer_clipboard_only_for_terminals: bool,
    pub transcript_refinement: TranscriptRefinementSettings,
}

impl Default for VoiceWaveSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            active_model: "fw-small.en".to_string(),
            show_floating_hud: true,
            vad_threshold: 0.014,
            max_utterance_ms: 120_000,
            release_tail_ms: 500,
            decode_mode: DecodeMode::Balanced,
            diagnostics_opt_in: false,
            toggle_hotkey: LOCKED_TOGGLE_HOTKEY.to_string(),
            push_to_talk_hotkey: LOCKED_PUSH_TO_TALK_HOTKEY.to_string(),
            prefer_clipboard_fallback: false,
            format_profile: FormatProfile::Default,
            active_domain_packs: Vec::new(),
            app_profile_overrides: AppProfileOverrides::default(),
            code_mode: CodeModeSettings::default(),
            pro_post_processing_enabled: true,
            prefer_clipboard_only_for_terminals: true,
            transcript_refinement: TranscriptRefinementSettings::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("failed to read settings file: {0}")]
    Read(std::io::Error),
    #[error("failed to write settings file: {0}")]
    Write(std::io::Error),
    #[error("failed to parse settings JSON: {0}")]
    Parse(serde_json::Error),
    #[error("cannot resolve app data directory")]
    AppData,
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new() -> Result<Self, SettingsError> {
        let proj_dirs =
            ProjectDirs::from("com", "voicewave", "localcore").ok_or(SettingsError::AppData)?;
        let path = proj_dirs.config_dir().join("settings.json");
        Ok(Self { path })
    }

    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self) -> Result<VoiceWaveSettings, SettingsError> {
        if !self.path.exists() {
            return Ok(VoiceWaveSettings::default());
        }
        let raw = fs::read_to_string(&self.path).map_err(SettingsError::Read)?;
        let normalized = raw.trim_start_matches('\u{feff}').trim();
        if normalized.is_empty() {
            return Ok(VoiceWaveSettings::default());
        }
        let mut settings: VoiceWaveSettings =
            serde_json::from_str(normalized).map_err(SettingsError::Parse)?;
        settings.active_model = normalize_active_model_id(&settings.active_model);
        normalize_hotkey_bindings(&mut settings);
        normalize_pro_settings(&mut settings);
        Ok(settings)
    }

    pub fn save(&self, settings: &VoiceWaveSettings) -> Result<(), SettingsError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(SettingsError::Write)?;
        }
        let raw = serde_json::to_string_pretty(settings).map_err(SettingsError::Parse)?;
        fs::write(&self.path, raw).map_err(SettingsError::Write)?;
        Ok(())
    }
}

fn normalize_active_model_id(active_model: &str) -> String {
    match active_model.trim() {
        // Every installable catalog model must be listed here, otherwise the
        // user's selection is silently reset to fw-small.en on settings load.
        "fw-small.en"
        | "fw-large-v3"
        | "fw-large-v3-turbo"
        | "wcpp-small.en"
        | "wcpp-large-v3-turbo" => active_model.trim().to_string(),
        "tiny.en" | "base.en" | "small.en" | "medium.en" => "fw-small.en".to_string(),
        _ => "fw-small.en".to_string(),
    }
}

fn normalize_behavior(behavior: &mut AppProfileBehavior) {
    behavior.punctuation_aggressiveness = behavior.punctuation_aggressiveness.min(2);
    behavior.sentence_compactness = behavior.sentence_compactness.min(2);
}

pub fn normalize_pro_settings(settings: &mut VoiceWaveSettings) {
    normalize_behavior(&mut settings.app_profile_overrides.editor);
    normalize_behavior(&mut settings.app_profile_overrides.browser);
    normalize_behavior(&mut settings.app_profile_overrides.collab);
    normalize_behavior(&mut settings.app_profile_overrides.desktop);

    let mut seen = std::collections::HashSet::new();
    settings
        .active_domain_packs
        .retain(|pack| seen.insert(*pack));

    settings.transcript_refinement.timeout_ms =
        settings.transcript_refinement.timeout_ms.clamp(250, 30_000);
    if settings
        .transcript_refinement
        .endpoint_url
        .trim()
        .is_empty()
    {
        settings.transcript_refinement.endpoint_url =
            TranscriptRefinementSettings::default().endpoint_url;
    }
    if settings.transcript_refinement.model.trim().is_empty() {
        settings.transcript_refinement.model = TranscriptRefinementSettings::default().model;
    }
    normalize_workflow_presets(&mut settings.transcript_refinement);
}

fn normalize_workflow_presets(settings: &mut TranscriptRefinementSettings) {
    let defaults = WorkflowPromptPresets::default();
    normalize_workflow_preset(
        &mut settings.workflow_presets.clean_reply,
        &defaults.clean_reply,
    );
    normalize_workflow_preset(&mut settings.workflow_presets.planning, &defaults.planning);
    normalize_workflow_preset(&mut settings.workflow_presets.code, &defaults.code);
    normalize_workflow_preset(
        &mut settings.workflow_presets.detailed_notes,
        &defaults.detailed_notes,
    );
}

fn normalize_workflow_preset(preset: &mut WorkflowPromptPreset, default: &WorkflowPromptPreset) {
    if preset.system_prompt.trim().is_empty() {
        preset.system_prompt = default.system_prompt.clone();
    }
    if !preset.temperature.is_finite() {
        preset.temperature = default.temperature;
    }
    preset.temperature = preset.temperature.clamp(0.0, 2.0);
    if preset.max_tokens == 0 {
        preset.max_tokens = default.max_tokens;
    }
    preset.max_tokens = preset.max_tokens.clamp(1, 32_768);
    preset.timeout_ms = preset.timeout_ms.clamp(250, 30_000);
}

pub const LOCKED_TOGGLE_HOTKEY: &str = "Ctrl+Alt+X";
pub const LOCKED_PUSH_TO_TALK_HOTKEY: &str = "Ctrl+Windows";

pub fn normalize_hotkey_bindings(settings: &mut VoiceWaveSettings) {
    settings.toggle_hotkey = LOCKED_TOGGLE_HOTKEY.to_string();
    settings.push_to_talk_hotkey = LOCKED_PUSH_TO_TALK_HOTKEY.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings_path() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("voicewave-settings-{ts}.json"))
    }

    #[test]
    fn load_returns_default_if_missing() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path);
        let loaded = store.load().expect("load should succeed");
        assert_eq!(loaded.active_model, "fw-small.en");
    }

    #[test]
    fn save_then_load_round_trip() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let settings = VoiceWaveSettings {
            active_model: "fw-large-v3".to_string(),
            vad_threshold: 0.025,
            max_utterance_ms: 22_000,
            release_tail_ms: 300,
            decode_mode: DecodeMode::Fast,
            diagnostics_opt_in: true,
            toggle_hotkey: "Ctrl+Alt+X".to_string(),
            push_to_talk_hotkey: "Ctrl+Windows".to_string(),
            prefer_clipboard_fallback: true,
            format_profile: FormatProfile::Technical,
            active_domain_packs: vec![DomainPackId::Coding, DomainPackId::Student],
            code_mode: CodeModeSettings {
                enabled: true,
                spoken_symbols: true,
                preferred_casing: CodeCasingStyle::SnakeCase,
                wrap_in_fenced_block: false,
            },
            pro_post_processing_enabled: true,
            ..VoiceWaveSettings::default()
        };

        store.save(&settings).expect("save should succeed");
        let loaded = store.load().expect("load should succeed");

        assert_eq!(loaded.active_model, "fw-large-v3");
        assert!((loaded.vad_threshold - 0.025).abs() < 1e-6);
        assert_eq!(loaded.max_utterance_ms, 22_000);
        assert_eq!(loaded.release_tail_ms, 300);
        assert_eq!(loaded.decode_mode, DecodeMode::Fast);
        assert!(loaded.diagnostics_opt_in);
        assert!(loaded.prefer_clipboard_fallback);
        assert_eq!(loaded.format_profile, FormatProfile::Technical);
        assert!(loaded.pro_post_processing_enabled);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_enforces_locked_hotkey_pair() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let raw = r#"{"toggleHotkey":"Ctrl+Shift+Space","pushToTalkHotkey":"Ctrl+Alt+Space"}"#;
        std::fs::write(&path, raw).expect("write should succeed");

        let loaded = store.load().expect("load should succeed");
        assert_eq!(loaded.toggle_hotkey, LOCKED_TOGGLE_HOTKEY);
        assert_eq!(loaded.push_to_talk_hotkey, LOCKED_PUSH_TO_TALK_HOTKEY);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_accepts_utf8_bom_prefixed_json() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let raw = "\u{feff}{\"activeModel\":\"tiny.en\"}";
        std::fs::write(&path, raw).expect("write should succeed");

        let loaded = store.load().expect("load should succeed");
        assert_eq!(loaded.active_model, "fw-small.en");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_returns_default_for_whitespace_only_json() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        std::fs::write(&path, " \n\t  ").expect("write should succeed");

        let loaded = store.load().expect("load should succeed");
        assert_eq!(loaded.active_model, "fw-small.en");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalize_pro_settings_clamps_behavior_and_deduplicates_domain_packs() {
        let mut settings = VoiceWaveSettings::default();
        settings
            .app_profile_overrides
            .editor
            .punctuation_aggressiveness = 9;
        settings.app_profile_overrides.browser.sentence_compactness = 7;
        settings.active_domain_packs = vec![
            DomainPackId::Coding,
            DomainPackId::Student,
            DomainPackId::Coding,
            DomainPackId::Productivity,
            DomainPackId::Student,
        ];

        normalize_pro_settings(&mut settings);

        assert_eq!(
            settings
                .app_profile_overrides
                .editor
                .punctuation_aggressiveness,
            2
        );
        assert_eq!(
            settings.app_profile_overrides.browser.sentence_compactness,
            2
        );
        assert_eq!(
            settings.active_domain_packs,
            vec![
                DomainPackId::Coding,
                DomainPackId::Student,
                DomainPackId::Productivity
            ]
        );
    }

    #[test]
    fn prefer_clipboard_only_for_terminals_defaults_to_true() {
        let settings = VoiceWaveSettings::default();
        assert!(settings.prefer_clipboard_only_for_terminals);
    }

    #[test]
    fn prefer_clipboard_only_for_terminals_round_trips() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let mut settings = VoiceWaveSettings::default();
        settings.prefer_clipboard_only_for_terminals = false;
        store.save(&settings).expect("save should succeed");

        let loaded = store.load().expect("load should succeed");
        assert!(!loaded.prefer_clipboard_only_for_terminals);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn backward_compat_missing_field_defaults_to_true() {
        // Old settings JSON without preferClipboardOnlyForTerminals
        // should deserialize with the default (true).
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let raw = r#"{"activeModel":"fw-small.en"}"#;
        std::fs::write(&path, raw).expect("write should succeed");

        let loaded = store.load().expect("load should succeed");
        assert!(loaded.prefer_clipboard_only_for_terminals);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn disabled_refinement_preserves_selected_provider() {
        let mut settings = VoiceWaveSettings::default();
        settings.transcript_refinement.enabled = false;
        settings.transcript_refinement.provider = TranscriptRefinementProviderKind::Ollama;

        normalize_pro_settings(&mut settings);

        assert_eq!(
            settings.transcript_refinement.provider,
            TranscriptRefinementProviderKind::Ollama
        );
    }

    #[test]
    fn workflow_presets_default_to_four_strict_backend_templates() {
        let presets = WorkflowPromptPresets::default();

        assert!(presets
            .clean_reply
            .system_prompt
            .contains("Never summarize"));
        assert!(presets
            .planning
            .system_prompt
            .contains("Objective, Context, Decisions, Tasks, Unresolved Questions, Next Action"));
        assert!(presets.code.system_prompt.contains("Windows file paths"));
        assert!(presets
            .detailed_notes
            .system_prompt
            .contains("retaining all original substantive content"));
    }

    #[test]
    fn workflow_preset_for_mode_maps_reply_and_clean_to_clean_reply() {
        let presets = WorkflowPromptPresets::default();

        assert_eq!(
            presets
                .preset_for_mode(TranscriptRefinementMode::Clean)
                .expect("clean preset")
                .system_prompt,
            presets.clean_reply.system_prompt
        );
        assert_eq!(
            presets
                .preset_for_mode(TranscriptRefinementMode::Reply)
                .expect("reply preset")
                .system_prompt,
            presets.clean_reply.system_prompt
        );
        assert!(presets
            .preset_for_mode(TranscriptRefinementMode::Raw)
            .is_none());
    }

    #[test]
    fn workflow_key_parser_accepts_user_facing_names() {
        assert_eq!(
            TranscriptRefinementMode::from_workflow_key("Clean Reply"),
            Some(TranscriptRefinementMode::Reply)
        );
        assert_eq!(
            TranscriptRefinementMode::from_workflow_key("agent-prompt"),
            Some(TranscriptRefinementMode::Code)
        );
        assert_eq!(
            TranscriptRefinementMode::from_workflow_key("detailed_notes"),
            Some(TranscriptRefinementMode::DetailedNotes)
        );
        assert_eq!(TranscriptRefinementMode::from_workflow_key("unknown"), None);
    }

    #[test]
    fn staging_and_auto_profile_flags_default_to_fail_open_false() {
        let settings = VoiceWaveSettings::default();

        assert!(
            !settings
                .transcript_refinement
                .automatic_profile_switching_enabled
        );
        assert!(!settings.transcript_refinement.requires_manual_approval);
    }

    #[test]
    fn backward_compat_missing_workflow_presets_loads_defaults() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let raw = r#"{
            "activeModel": "fw-small.en",
            "transcriptRefinement": {
                "enabled": true,
                "provider": "ollama",
                "mode": "planning",
                "endpointUrl": "http://127.0.0.1:11434/v1/chat/completions",
                "model": "llama3.2:3b",
                "timeoutMs": 2500
            }
        }"#;
        std::fs::write(&path, raw).expect("write should succeed");

        let loaded = store.load().expect("load should succeed");

        assert_eq!(
            loaded
                .transcript_refinement
                .workflow_presets
                .planning
                .system_prompt,
            PLANNING_SYSTEM_PROMPT
        );
        assert!(
            !loaded
                .transcript_refinement
                .automatic_profile_switching_enabled
        );
        assert!(!loaded.transcript_refinement.requires_manual_approval);
        assert_eq!(
            loaded
                .transcript_refinement
                .workflow_presets
                .code
                .max_tokens,
            1_024
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn workflow_preset_tuning_is_clamped_on_load() {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let raw = r#"{
            "transcriptRefinement": {
                "workflowPresets": {
                    "planning": {
                        "systemPrompt": "",
                        "temperature": 9.5,
                        "maxTokens": 0,
                        "timeoutMs": 50
                    },
                    "code": {
                        "systemPrompt": "Keep C:\\Users\\Hunter\\repo and --flag literal.",
                        "temperature": -1.0,
                        "maxTokens": 999999,
                        "timeoutMs": 999999
                    }
                }
            }
        }"#;
        std::fs::write(&path, raw).expect("write should succeed");

        let loaded = store.load().expect("load should succeed");
        let planning = &loaded.transcript_refinement.workflow_presets.planning;
        let code = &loaded.transcript_refinement.workflow_presets.code;

        assert_eq!(planning.system_prompt, PLANNING_SYSTEM_PROMPT);
        assert_eq!(planning.temperature, 2.0);
        assert_eq!(planning.max_tokens, 1_024);
        assert_eq!(planning.timeout_ms, 250);
        assert_eq!(code.temperature, 0.0);
        assert_eq!(code.max_tokens, 32_768);
        assert_eq!(code.timeout_ms, 30_000);
        assert!(code.system_prompt.contains("C:\\Users\\Hunter\\repo"));

        let _ = std::fs::remove_file(path);
    }
}

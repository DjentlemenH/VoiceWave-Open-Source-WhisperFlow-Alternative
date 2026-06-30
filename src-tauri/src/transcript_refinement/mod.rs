use crate::{
    settings::{
        TranscriptRefinementMode, TranscriptRefinementProviderKind, TranscriptRefinementSettings,
    },
    voice_vault::VoiceVaultLogEntry,
};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, time::Duration};
use tokio_util::sync::CancellationToken;

pub type RefinementFuture<'a> = Pin<
    Box<dyn Future<Output = Result<TranscriptRefinementOutput, TextRefinementError>> + Send + 'a>,
>;

pub trait TextRefinementProvider: Send + Sync {
    fn refine<'a>(&'a self, request: TranscriptRefinementRequest) -> RefinementFuture<'a>;
}

#[derive(Debug, Clone)]
pub struct TranscriptRefinementRequest {
    pub raw_text: String,
    pub mode: TranscriptRefinementMode,
    pub cancellation_token: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRefinementOutput {
    pub cleaned_text: String,
    pub transformed_text: String,
    pub final_edited_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRefinementOutcome {
    pub raw_text: String,
    pub cleaned_text: String,
    pub transformed_text: String,
    pub final_edited_text: String,
    pub processing_mode: String,
    pub transaction_status: String,
    pub error_message: Option<String>,
}

impl TranscriptRefinementOutcome {
    pub fn to_voice_vault_entry(&self, audio_file_path: Option<String>) -> VoiceVaultLogEntry {
        VoiceVaultLogEntry {
            audio_file_path,
            processing_mode: self.processing_mode.clone(),
            raw_text: self.raw_text.clone(),
            cleaned_text: self.cleaned_text.clone(),
            transformed_text: self.transformed_text.clone(),
            final_edited_text: self.final_edited_text.clone(),
            transaction_status: self.transaction_status.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TextRefinementError {
    #[error("refinement disabled")]
    Disabled,
    #[error("refinement timed out after {0} ms")]
    Timeout(u64),
    #[error("provider request failed: {0}")]
    Provider(String),
    #[error("provider endpoint must be local: {0}")]
    NonLocalEndpoint(String),
    #[error("provider response was invalid: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, Default)]
pub struct DisabledProvider;

impl TextRefinementProvider for DisabledProvider {
    fn refine<'a>(&'a self, request: TranscriptRefinementRequest) -> RefinementFuture<'a> {
        Box::pin(async move {
            Ok(TranscriptRefinementOutput {
                cleaned_text: request.raw_text.clone(),
                transformed_text: String::new(),
                final_edited_text: request.raw_text,
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    endpoint_url: String,
    model: String,
    system_prompt: Option<String>,
    request_timeout_ms: u64,
}

impl OpenAiCompatibleProvider {
    pub fn from_settings(settings: &TranscriptRefinementSettings) -> Self {
        Self {
            endpoint_url: settings.endpoint_url.clone(),
            model: settings.model.clone(),
            system_prompt: settings.system_prompt.clone(),
            request_timeout_ms: settings.timeout_ms.clamp(250, 30_000),
        }
    }

    fn refine_blocking(
        endpoint_url: String,
        model: String,
        system_prompt: Option<String>,
        request_timeout_ms: u64,
        request: TranscriptRefinementRequest,
    ) -> Result<TranscriptRefinementOutput, TextRefinementError> {
        if request.cancellation_token.is_cancelled() {
            return Err(TextRefinementError::Provider(
                "request cancelled before dispatch".to_string(),
            ));
        }
        if !is_local_endpoint(&endpoint_url) {
            return Err(TextRefinementError::NonLocalEndpoint(endpoint_url));
        }

        let payload = serde_json::json!({
            "model": model,
            "temperature": 0.2,
            "stream": false,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt.unwrap_or_else(default_system_prompt),
                },
                {
                    "role": "user",
                    "content": user_prompt(&request.raw_text, request.mode),
                }
            ]
        });

        let timeout = Duration::from_millis(request_timeout_ms);
        let connect_timeout = timeout.min(Duration::from_millis(1_000));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(connect_timeout)
            .timeout_read(timeout)
            .build();

        let response = agent
            .post(&endpoint_url)
            .set("Content-Type", "application/json")
            .send_string(&payload.to_string())
            .map_err(|err| TextRefinementError::Provider(err.to_string()))?;
        let body = response
            .into_string()
            .map_err(|err| TextRefinementError::Provider(err.to_string()))?;

        if request.cancellation_token.is_cancelled() {
            return Err(TextRefinementError::Provider(
                "request cancelled after provider response".to_string(),
            ));
        }

        parse_chat_completion(&body, &request)
    }
}

impl TextRefinementProvider for OpenAiCompatibleProvider {
    fn refine<'a>(&'a self, request: TranscriptRefinementRequest) -> RefinementFuture<'a> {
        let endpoint_url = self.endpoint_url.clone();
        let model = self.model.clone();
        let system_prompt = self.system_prompt.clone();
        let request_timeout_ms = self.request_timeout_ms;
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                Self::refine_blocking(
                    endpoint_url,
                    model,
                    system_prompt,
                    request_timeout_ms,
                    request,
                )
            })
            .await
            .map_err(|err| TextRefinementError::Provider(format!("provider task failed: {err}")))?
        })
    }
}

pub async fn refine_transcript(
    settings: &TranscriptRefinementSettings,
    raw_text: String,
) -> TranscriptRefinementOutcome {
    if !settings.enabled
        || settings.provider == TranscriptRefinementProviderKind::Disabled
        || settings.mode == TranscriptRefinementMode::Raw
    {
        return refine_transcript_with_provider(&DisabledProvider, settings, raw_text).await;
    }

    let provider = OpenAiCompatibleProvider::from_settings(settings);
    refine_transcript_with_provider(&provider, settings, raw_text).await
}

pub async fn refine_transcript_with_provider<P>(
    provider: &P,
    settings: &TranscriptRefinementSettings,
    raw_text: String,
) -> TranscriptRefinementOutcome
where
    P: TextRefinementProvider + ?Sized,
{
    let processing_mode = settings.mode.processing_mode().to_string();
    let cancellation_token = CancellationToken::new();
    let timeout_ms = settings.timeout_ms.clamp(250, 30_000);
    let request = TranscriptRefinementRequest {
        raw_text: raw_text.clone(),
        mode: settings.mode,
        cancellation_token: cancellation_token.clone(),
    };

    let timeout = tokio::time::sleep(Duration::from_millis(timeout_ms));
    tokio::pin!(timeout);

    let result = tokio::select! {
        result = provider.refine(request) => result,
        _ = &mut timeout => {
            cancellation_token.cancel();
            Err(TextRefinementError::Timeout(timeout_ms))
        }
    };

    match result {
        Ok(output) => successful_outcome(raw_text, processing_mode, output),
        Err(error) => fallback_outcome(raw_text, processing_mode, Some(error.to_string())),
    }
}

fn successful_outcome(
    raw_text: String,
    processing_mode: String,
    output: TranscriptRefinementOutput,
) -> TranscriptRefinementOutcome {
    let final_edited_text = if output.final_edited_text.trim().is_empty() {
        raw_text.clone()
    } else {
        output.final_edited_text
    };
    TranscriptRefinementOutcome {
        raw_text,
        cleaned_text: output.cleaned_text,
        transformed_text: output.transformed_text,
        final_edited_text,
        processing_mode,
        transaction_status: "Accepted".to_string(),
        error_message: None,
    }
}

fn fallback_outcome(
    raw_text: String,
    processing_mode: String,
    error_message: Option<String>,
) -> TranscriptRefinementOutcome {
    TranscriptRefinementOutcome {
        raw_text: raw_text.clone(),
        cleaned_text: raw_text.clone(),
        transformed_text: String::new(),
        final_edited_text: raw_text,
        processing_mode,
        transaction_status: "Rejected".to_string(),
        error_message,
    }
}

fn default_system_prompt() -> String {
    "You refine local dictation text for insertion into the user's active application. Return only valid JSON with keys cleaned_text, transformed_text, and final_edited_text. Do not add commentary.".to_string()
}

fn user_prompt(raw_text: &str, mode: TranscriptRefinementMode) -> String {
    format!(
        "Mode: {}\n\nRaw transcript:\n{}\n\nReturn JSON exactly like {{\"cleaned_text\":\"...\",\"transformed_text\":\"...\",\"final_edited_text\":\"...\"}}.",
        mode.processing_mode(),
        raw_text
    )
}

fn is_local_endpoint(endpoint_url: &str) -> bool {
    let trimmed = endpoint_url.trim().to_ascii_lowercase();
    let Some(authority) = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .and_then(|without_scheme| without_scheme.split('/').next())
    else {
        return false;
    };
    let host = authority.split('@').last().unwrap_or(authority);
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
        .unwrap_or_else(|| host.split(':').next().unwrap_or(host));

    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProviderJsonOutput {
    cleaned_text: String,
    transformed_text: String,
    final_edited_text: String,
}

fn parse_chat_completion(
    body: &str,
    request: &TranscriptRefinementRequest,
) -> Result<TranscriptRefinementOutput, TextRefinementError> {
    let response: ChatCompletionResponse = serde_json::from_str(body)
        .map_err(|err| TextRefinementError::InvalidResponse(err.to_string()))?;
    let content = response
        .choices
        .first()
        .map(|choice| choice.message.content.trim())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| {
            TextRefinementError::InvalidResponse("missing message content".to_string())
        })?;

    if let Ok(parsed) = parse_provider_json_output(content) {
        return Ok(TranscriptRefinementOutput {
            cleaned_text: parsed.cleaned_text,
            transformed_text: parsed.transformed_text,
            final_edited_text: parsed.final_edited_text,
        });
    }

    Ok(output_from_plain_content(
        request.raw_text.clone(),
        request.mode,
        content.to_string(),
    ))
}

fn parse_provider_json_output(content: &str) -> Result<ProviderJsonOutput, serde_json::Error> {
    let trimmed = content.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .and_then(|text| text.strip_suffix("```"))
        .or_else(|| {
            trimmed
                .strip_prefix("```")
                .and_then(|text| text.strip_suffix("```"))
        })
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(without_fence)
}

fn output_from_plain_content(
    raw_text: String,
    mode: TranscriptRefinementMode,
    content: String,
) -> TranscriptRefinementOutput {
    match mode {
        TranscriptRefinementMode::Clean => TranscriptRefinementOutput {
            cleaned_text: content.clone(),
            transformed_text: String::new(),
            final_edited_text: content,
        },
        TranscriptRefinementMode::Raw => TranscriptRefinementOutput {
            cleaned_text: raw_text.clone(),
            transformed_text: String::new(),
            final_edited_text: raw_text,
        },
        TranscriptRefinementMode::Planning
        | TranscriptRefinementMode::Code
        | TranscriptRefinementMode::Reply
        | TranscriptRefinementMode::DetailedNotes => TranscriptRefinementOutput {
            cleaned_text: raw_text,
            transformed_text: content.clone(),
            final_edited_text: content,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProvider {
        output: TranscriptRefinementOutput,
    }

    impl TextRefinementProvider for StaticProvider {
        fn refine<'a>(&'a self, _request: TranscriptRefinementRequest) -> RefinementFuture<'a> {
            let output = self.output.clone();
            Box::pin(async move { Ok(output) })
        }
    }

    struct FailingProvider;

    impl TextRefinementProvider for FailingProvider {
        fn refine<'a>(&'a self, _request: TranscriptRefinementRequest) -> RefinementFuture<'a> {
            Box::pin(async move { Err(TextRefinementError::Provider("offline".to_string())) })
        }
    }

    struct SlowProvider;

    impl TextRefinementProvider for SlowProvider {
        fn refine<'a>(&'a self, request: TranscriptRefinementRequest) -> RefinementFuture<'a> {
            Box::pin(async move {
                request.cancellation_token.cancelled().await;
                Err(TextRefinementError::Provider("cancelled".to_string()))
            })
        }
    }

    #[tokio::test]
    async fn disabled_provider_preserves_formatting_exactly() {
        let settings = TranscriptRefinementSettings::default();
        let raw = "line one\n  line two\twith spacing".to_string();

        let outcome =
            refine_transcript_with_provider(&DisabledProvider, &settings, raw.clone()).await;

        assert_eq!(outcome.raw_text, raw);
        assert_eq!(outcome.cleaned_text, raw);
        assert_eq!(outcome.transformed_text, "");
        assert_eq!(outcome.final_edited_text, raw);
        assert_eq!(outcome.transaction_status, "Accepted");
        assert_eq!(outcome.error_message, None);
    }

    #[tokio::test]
    async fn provider_success_maps_cleaned_transformed_and_final_text() {
        let settings = TranscriptRefinementSettings {
            enabled: true,
            provider: TranscriptRefinementProviderKind::OpenAiCompatible,
            mode: TranscriptRefinementMode::Planning,
            ..TranscriptRefinementSettings::default()
        };
        let provider = StaticProvider {
            output: TranscriptRefinementOutput {
                cleaned_text: "Cleaned".to_string(),
                transformed_text: "Plan bullets".to_string(),
                final_edited_text: "Plan bullets".to_string(),
            },
        };

        let outcome =
            refine_transcript_with_provider(&provider, &settings, "raw words".to_string()).await;

        assert_eq!(outcome.processing_mode, "Planning");
        assert_eq!(outcome.raw_text, "raw words");
        assert_eq!(outcome.cleaned_text, "Cleaned");
        assert_eq!(outcome.transformed_text, "Plan bullets");
        assert_eq!(outcome.final_edited_text, "Plan bullets");
        assert_eq!(outcome.transaction_status, "Accepted");
    }

    #[tokio::test]
    async fn provider_failure_falls_back_to_raw_text_without_panic() {
        let settings = TranscriptRefinementSettings {
            enabled: true,
            provider: TranscriptRefinementProviderKind::OpenAiCompatible,
            mode: TranscriptRefinementMode::Code,
            ..TranscriptRefinementSettings::default()
        };

        let outcome = refine_transcript_with_provider(
            &FailingProvider,
            &settings,
            "let x equal one".to_string(),
        )
        .await;

        assert_eq!(outcome.processing_mode, "Code");
        assert_eq!(outcome.cleaned_text, "let x equal one");
        assert_eq!(outcome.final_edited_text, "let x equal one");
        assert_eq!(outcome.transaction_status, "Rejected");
        assert!(outcome
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("offline"));
    }

    #[tokio::test]
    async fn timeout_cancels_provider_and_falls_back_to_raw_text() {
        let settings = TranscriptRefinementSettings {
            enabled: true,
            provider: TranscriptRefinementProviderKind::OpenAiCompatible,
            mode: TranscriptRefinementMode::Clean,
            timeout_ms: 250,
            ..TranscriptRefinementSettings::default()
        };

        let outcome = refine_transcript_with_provider(
            &SlowProvider,
            &settings,
            "still insert me".to_string(),
        )
        .await;

        assert_eq!(outcome.processing_mode, "Clean");
        assert_eq!(outcome.final_edited_text, "still insert me");
        assert_eq!(outcome.transaction_status, "Rejected");
        assert!(outcome
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("timed out"));
    }

    #[test]
    fn parses_openai_compatible_json_response() {
        let body = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": "{\"cleaned_text\":\"Clean\",\"transformed_text\":\"Transform\",\"final_edited_text\":\"Final\"}"
                    }
                }
            ]
        })
        .to_string();
        let request = TranscriptRefinementRequest {
            raw_text: "raw".to_string(),
            mode: TranscriptRefinementMode::Planning,
            cancellation_token: CancellationToken::new(),
        };

        let output = parse_chat_completion(&body, &request).expect("response should parse");

        assert_eq!(output.cleaned_text, "Clean");
        assert_eq!(output.transformed_text, "Transform");
        assert_eq!(output.final_edited_text, "Final");
    }

    #[test]
    fn endpoint_guard_allows_only_local_provider_urls() {
        assert!(is_local_endpoint(
            "http://127.0.0.1:11434/v1/chat/completions"
        ));
        assert!(is_local_endpoint(
            "http://localhost:1234/v1/chat/completions"
        ));
        assert!(is_local_endpoint("http://[::1]:8080/v1/chat/completions"));
        assert!(!is_local_endpoint(
            "https://api.openai.com/v1/chat/completions"
        ));
        assert!(!is_local_endpoint(
            "http://192.168.1.10:11434/v1/chat/completions"
        ));
    }
}

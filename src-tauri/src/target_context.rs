use crate::settings::{AppTargetClass, TranscriptRefinementMode};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetAppContext {
    pub process_exe: Option<String>,
    pub window_title: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TargetContextError {
    #[error("active foreground window is unavailable")]
    MissingForegroundWindow,
    #[error("active foreground process is unavailable")]
    MissingForegroundProcess,
    #[error("failed to inspect active foreground process")]
    ProcessQuery,
}

pub fn evaluate_default_mode(context: &TargetAppContext) -> Option<TranscriptRefinementMode> {
    let process = context
        .process_exe
        .as_deref()
        .map(normalize_process_name)
        .unwrap_or_default();

    match process.as_str() {
        "cmd.exe" | "powershell.exe" | "pwsh.exe" | "wt.exe" => Some(TranscriptRefinementMode::Raw),
        "code.exe" | "cursor.exe" | "zed.exe" => Some(TranscriptRefinementMode::Code),
        "chrome.exe" | "msedge.exe" | "discord.exe" => Some(TranscriptRefinementMode::Reply),
        "obsidian.exe" => Some(TranscriptRefinementMode::DetailedNotes),
        _ => None,
    }
}

pub fn evaluate_default_app_target_class(context: &TargetAppContext) -> Option<AppTargetClass> {
    let process = context
        .process_exe
        .as_deref()
        .map(normalize_process_name)
        .unwrap_or_default();

    match process.as_str() {
        "cmd.exe" | "powershell.exe" | "pwsh.exe" | "wt.exe" => Some(AppTargetClass::Desktop),
        "code.exe" | "cursor.exe" | "zed.exe" => Some(AppTargetClass::Editor),
        "chrome.exe" | "msedge.exe" => Some(AppTargetClass::Browser),
        "discord.exe" => Some(AppTargetClass::Collab),
        "obsidian.exe" => Some(AppTargetClass::Desktop),
        _ => None,
    }
}

fn normalize_process_name(value: &str) -> String {
    let trimmed = value.trim().trim_matches('"');
    Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

#[cfg(target_os = "windows")]
pub fn active_target_context() -> Result<TargetAppContext, TargetContextError> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return Err(TargetContextError::MissingForegroundWindow);
        }

        let mut process_id = 0_u32;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 {
            return Err(TargetContextError::MissingForegroundProcess);
        }

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if handle.is_null() {
            return Err(TargetContextError::ProcessQuery);
        }

        let mut process_buffer = vec![0_u16; 32_768];
        let mut process_len = process_buffer.len() as u32;
        let process_exe = if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            process_buffer.as_mut_ptr(),
            &mut process_len,
        ) == 0
        {
            CloseHandle(handle);
            return Err(TargetContextError::ProcessQuery);
        } else {
            Some(String::from_utf16_lossy(
                &process_buffer[..process_len as usize],
            ))
        };
        CloseHandle(handle);

        Ok(TargetAppContext {
            process_exe,
            window_title: foreground_window_title(hwnd),
        })
    }
}

#[cfg(target_os = "windows")]
unsafe fn foreground_window_title(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0_u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

#[cfg(not(target_os = "windows"))]
pub fn active_target_context() -> Result<TargetAppContext, TargetContextError> {
    Err(TargetContextError::MissingForegroundWindow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(process_exe: &str) -> TargetAppContext {
        TargetAppContext {
            process_exe: Some(process_exe.to_string()),
            window_title: None,
        }
    }

    #[test]
    fn terminal_processes_default_to_raw_mode() {
        for process in ["cmd.exe", "powershell.exe", "wt.exe"] {
            assert_eq!(
                evaluate_default_mode(&context(process)),
                Some(TranscriptRefinementMode::Raw)
            );
        }
    }

    #[test]
    fn development_processes_default_to_code_mode() {
        for process in ["code.exe", "cursor.exe", "zed.exe"] {
            assert_eq!(
                evaluate_default_mode(&context(process)),
                Some(TranscriptRefinementMode::Code)
            );
        }
    }

    #[test]
    fn process_context_maps_to_app_target_classes() {
        assert_eq!(
            evaluate_default_app_target_class(&context("cmd.exe")),
            Some(AppTargetClass::Desktop)
        );
        assert_eq!(
            evaluate_default_app_target_class(&context("code.exe")),
            Some(AppTargetClass::Editor)
        );
        assert_eq!(
            evaluate_default_app_target_class(&context("chrome.exe")),
            Some(AppTargetClass::Browser)
        );
        assert_eq!(
            evaluate_default_app_target_class(&context("discord.exe")),
            Some(AppTargetClass::Collab)
        );
        assert_eq!(
            evaluate_default_app_target_class(&context("notepad.exe")),
            None
        );
    }

    #[test]
    fn browser_and_chat_processes_default_to_clean_reply_mode() {
        for process in ["chrome.exe", "msedge.exe", "discord.exe"] {
            assert_eq!(
                evaluate_default_mode(&context(process)),
                Some(TranscriptRefinementMode::Reply)
            );
        }
    }

    #[test]
    fn obsidian_defaults_to_detailed_notes_mode() {
        assert_eq!(
            evaluate_default_mode(&context(
                "C:\\Users\\Hunter\\AppData\\Local\\Obsidian\\obsidian.exe"
            )),
            Some(TranscriptRefinementMode::DetailedNotes)
        );
    }

    #[test]
    fn unknown_or_missing_process_has_no_override() {
        assert_eq!(evaluate_default_mode(&context("notepad.exe")), None);
        assert_eq!(
            evaluate_default_mode(&TargetAppContext {
                process_exe: None,
                window_title: Some("Untitled".to_string()),
            }),
            None
        );
    }
}

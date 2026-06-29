# Voice Vault Phase 1 Backend Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the local Windows build prerequisite, verify the existing Voice Vault schema scaffold, and add backend-only create/update/get/list APIs for Voice Vault logs.

**Architecture:** Keep `voice_vault` as the focused SQLite data-access module. Store a `VoiceVaultDb` handle on `VoiceWaveController`, expose controller methods in `state.rs`, and add thin Tauri command wrappers in `lib.rs`. Do not touch frontend UI, live dictation flow, audio archiving, or transcript history behavior in this phase.

**Tech Stack:** Rust 2021, Tauri 2, `rusqlite` with bundled SQLite, Windows PowerShell, winget-managed LLVM/CMake prerequisites.

---

## Task 1: Repair Local Build Prerequisites

**Files:**
- No repo file edits.

- [ ] **Step 1: Install LLVM if libclang is missing**

Run in PowerShell:

```powershell
if (-not (Test-Path 'C:\Program Files\LLVM\bin\libclang.dll')) {
  winget install --id LLVM.LLVM --source winget --accept-package-agreements --accept-source-agreements --silent
}
```

Expected: `C:\Program Files\LLVM\bin\libclang.dll` exists.

- [ ] **Step 2: Install CMake if missing**

Run:

```powershell
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
  winget install --id Kitware.CMake --source winget --accept-package-agreements --accept-source-agreements --silent
}
```

Expected: `cmake --version` works in a fresh shell or after adding CMake to PATH.

- [ ] **Step 3: Set LIBCLANG_PATH for this shell**

Run:

```powershell
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:PATH = 'C:\Program Files\LLVM\bin;' + $env:PATH
```

Expected: `Test-Path "$env:LIBCLANG_PATH\libclang.dll"` returns `True`.

- [ ] **Step 4: Persist LIBCLANG_PATH**

Run:

```powershell
setx LIBCLANG_PATH "C:\Program Files\LLVM\bin"
```

Expected: future shells inherit `LIBCLANG_PATH`.

- [ ] **Step 5: Validate prerequisite discovery**

Run:

```powershell
where.exe clang
Test-Path "$env:LIBCLANG_PATH\libclang.dll"
```

Expected: `clang.exe` is found and the `Test-Path` result is `True`.

## Task 2: Verify Existing Phase 0 Schema

**Files:**
- Modify only if validation exposes an existing build defect: `vendor/whisper-rs-sys/build.rs`

- [ ] **Step 1: Run the targeted schema tests**

Run from `H:\Repositories\VoiceWave-VoiceVault`:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml voice_vault --lib --no-default-features
```

Expected: the `voice_vault` tests pass. The previous failure message `Unable to find libclang` must not appear.

- [ ] **Step 2: If Windows bindgen still fails, force stable bundled bindings**

If validation gets past libclang but fails in `whisper-rs-sys` generated layout assertions, patch `vendor/whisper-rs-sys/build.rs` so Windows uses the vendored `src/bindings.rs` and strips bindgen layout-test const blocks before writing `OUT_DIR\bindings.rs`.

Expected: Windows build no longer depends on regenerating opaque `whisper_full_params` bindings and no longer fails on stale C runtime layout assertions.

- [ ] **Step 3: Run the existing backend check wrapper**

Run:

```powershell
npm run tauri:check
```

Expected: cargo check completes. If it fails for a non-Voice-Vault existing project issue, record the exact error in Beads and continue only if the Phase 1 code can still be tested directly.

## Task 3: Add Voice Vault Data Models And DB Operations

**Files:**
- Modify: `src-tauri/src/voice_vault/mod.rs`

- [ ] **Step 1: Add public request/response structs and enums**

Add serializable Rust types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProcessingMode {
    Code,
    Planning,
    Reply,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TransactionStatus {
    Accepted,
    Edited,
    Rejected,
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
```

Expected: API JSON uses camelCase while SQLite stores exact title-case status/mode strings.

- [ ] **Step 2: Add DB error for missing rows**

Extend `VoiceVaultError` with:

```rust
#[error("voice vault log not found: {0}")]
LogNotFound(i64),
```

- [ ] **Step 3: Add DB methods**

Implement these methods on `VoiceVaultDb`:

```rust
pub fn create_log(&self, request: CreateVoiceVaultLogRequest) -> Result<VoiceVaultLog, VoiceVaultError>;
pub fn update_log(&self, id: i64, request: UpdateVoiceVaultLogRequest) -> Result<VoiceVaultLog, VoiceVaultError>;
pub fn get_log(&self, id: i64) -> Result<VoiceVaultLog, VoiceVaultError>;
pub fn list_logs(&self, limit: Option<usize>) -> Result<Vec<VoiceVaultLog>, VoiceVaultError>;
```

Behavior:
- `create_log` inserts defaults for omitted fields: `Planning`, empty strings, and `Accepted`.
- `timestamp` remains SQLite UTC `CURRENT_TIMESTAMP` text in `YYYY-MM-DD HH:MM:SS` shape.
- Empty or whitespace-only `audio_file_path` values are stored as `NULL`.
- `update_log` only patches the four editable post-processing fields: `cleaned_text`, `transformed_text`, `final_edited_text`, `transaction_status`.
- Text patches do not implicitly change `transaction_status`; callers must pass `Edited` explicitly.
- `get_log` returns `LogNotFound(id)` when no row exists.
- `list_logs` orders by `id DESC`, defaults to `50`, and clamps to the inclusive range `1..=200`.
- Plain local SQLite is intentional for Phase 1. Encryption or SQLCipher is deferred to a privacy hardening phase.

- [ ] **Step 4: Add focused DB tests**

Add tests for:
- create with defaults
- create with all fields
- update editable fields
- get missing row
- list default/max limit behavior

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml voice_vault --lib --no-default-features
```

Expected: all `voice_vault` tests pass.

- [ ] **Step 5: Commit DB API work**

Run:

```powershell
git add src-tauri/src/voice_vault/mod.rs
git commit -m "Add voice vault backend data operations"
```

## Task 4: Wire VoiceVaultDb Into Controller And Tauri Commands

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Store the DB handle on the controller**

Change `VoiceWaveController` so it owns:

```rust
voice_vault: VoiceVaultDb,
```

In `VoiceWaveController::new()`, create the DB once:

```rust
let voice_vault = VoiceVaultDb::new()?;
voice_vault.initialize_schema()?;
```

Then store `voice_vault` on the controller.

Startup remains fail-fast: if the vault DB cannot initialize, app startup returns `ControllerError::VoiceVault`.

- [ ] **Step 2: Add controller methods**

Add async methods that call the DB synchronously:

```rust
pub async fn create_voice_vault_log(&self, request: CreateVoiceVaultLogRequest) -> Result<VoiceVaultLog, ControllerError>;
pub async fn update_voice_vault_log(&self, id: i64, request: UpdateVoiceVaultLogRequest) -> Result<VoiceVaultLog, ControllerError>;
pub async fn get_voice_vault_log(&self, id: i64) -> Result<VoiceVaultLog, ControllerError>;
pub async fn list_voice_vault_logs(&self, limit: Option<usize>) -> Result<Vec<VoiceVaultLog>, ControllerError>;
```

Expected: methods use existing `ControllerError::VoiceVault`.

- [ ] **Step 3: Add Tauri command wrappers**

In `lib.rs`, import the new types and add commands:

```rust
#[tauri::command]
async fn create_voice_vault_log(
    runtime: State<'_, RuntimeContext>,
    request: CreateVoiceVaultLogRequest,
) -> Result<VoiceVaultLog, String> { ... }
```

Repeat for update/get/list, mapping errors through `AppError::Controller`.

- [ ] **Step 4: Register commands**

Add the four commands to `tauri::generate_handler![...]`.

- [ ] **Step 5: Run backend checks**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml voice_vault --lib --no-default-features
npm run tauri:check
```

Expected: both pass, or any failure is unrelated and documented with exact output.

- [ ] **Step 6: Commit controller and command wiring**

Run:

```powershell
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "Expose voice vault backend commands"
```

## Task 5: Final Review, Push, And Tracker Updates

**Files:**
- No additional code edits unless review finds an issue.

- [ ] **Step 1: Run final status and tests**

Run:

```powershell
git status --short
cargo test --manifest-path src-tauri\Cargo.toml voice_vault --lib --no-default-features
npm run tauri:check
```

Expected: no uncommitted code changes after commits; tests/checks pass or documented as environmental.

- [ ] **Step 2: Push branch**

Run:

```powershell
git push -u origin codex/voice-vault-phase1-backend-core
```

Expected: branch is available on the user's fork.

- [ ] **Step 3: Close Beads work**

Close `TheHawkNest-ptu` if LLVM/libclang validation is fixed. Update or close `TheHawkNest-x48` depending on whether Phase 1 backend core fully passes validation.

Run:

```powershell
bd dolt pull
bd dolt push
```

Expected: Beads sync completes.

## Self-Review

- Spec coverage: The plan covers environment repair, Phase 0 verification, backend DB APIs, controller wiring, Tauri command exposure, tests, commits, push, and Beads updates.
- Placeholder scan: No TBD/TODO placeholders are present.
- Type consistency: The request/response type names are used consistently across DB, controller, and Tauri command tasks.

# VoiceWave Quickstart

Short guide for reopening the app and finding the main files that matter.

## Repo Location

The project lives at:

`H:\Repositories\VoiceWave-VoiceVault`

## Open The App

From the repo root:

```powershell
cd H:\Repositories\VoiceWave-VoiceVault
npm run tauri:dev
```

If the app launches but transcription fails with a Faster Whisper import error, run:

```powershell
npm run faster-whisper:setup:cpu
npm run tauri:dev
```

## Important Paths

- `src-tauri/src/state.rs` - main Tauri backend flow and dictation pipeline
- `src-tauri/src/settings/mod.rs` - saved settings and workflow presets
- `src-tauri/src/transcript_refinement/mod.rs` - local text refinement provider path
- `src-tauri/src/voice_vault/mod.rs` - SQLite voice vault storage
- `src-tauri/src/audio_archive.rs` - local audio file archive and retention cleanup
- `scripts/tauri/run-tauri-dev-windows.ps1` - Windows dev launcher used by `npm run tauri:dev`

## Useful Checks

Run these when you need a clean backend verification:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --no-default-features
npm run tauri:check
```

## What To Expect

- The desktop window title is `VoiceWave Pill`.
- Voice input should stay local on the machine.
- Audio archives are written under `H:\VoiceVault\Audio\`.
- Settings and voice vault history are stored in the app data area by the Rust backend.

## Current Branch

The active work branch is `feature/local-transcript-refinement`.


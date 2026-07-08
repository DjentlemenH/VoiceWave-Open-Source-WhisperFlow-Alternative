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

## Shortcut Rule

VoiceWave shortcuts on this machine:

- Start Menu: `C:\Users\H-Haw\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\VoiceWave.lnk`
- Desktop: `C:\Users\H-Haw\OneDrive\Desktop\VoiceWave.lnk`

These shortcuts target the standalone production release binary at `H:\Repositories\VoiceWave-VoiceVault\src-tauri\target\release\voicewave_core.exe` (with embedded frontend), allowing you to run the app offline without a terminal or a dev server running.

If you ever need to rebuild the standalone release binary, run:
```powershell
# Rebuilds release binary to C:\voicewave-tauri\target-gnu-build and skips installer creation
powershell -ExecutionPolicy Bypass -File ./scripts/tauri/run-tauri-build-windows.ps1 --bundles nsis --no-bundle

# Copy output files back into the repo target directory so shortcuts work
$src = "C:\voicewave-tauri\target-gnu-build\release"
$dst = "H:\Repositories\VoiceWave-VoiceVault\src-tauri\target\release"
Copy-Item "$src\voicewave_core.exe", "$src\*.dll" $dst -Force
Copy-Item "$src\faster-whisper\worker.py" "$dst\faster-whisper\worker.py" -Force
```

Other installed tools with shortcuts:

- `OpenHands Agent Canvas.lnk`
- `AnythingLLM.lnk`
- `Beads.lnk`

All three live in the Start Menu `Programs` folder so they can be found by search.

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

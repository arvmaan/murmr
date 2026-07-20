# Installing murmr (macOS)

murmr is a menu-bar app. This guide covers building, installing, and granting the
macOS permissions it needs to work.

## 1. Prerequisites

- Rust toolchain (`rustup`)
- The Tauri CLI: `cargo install tauri-cli --version "^2"`
- A whisper model. If you don't have one, download it via the CLI:
  ```bash
  cargo run -p murmer-core --bin murmer --features bedrock -- --download-model base.en
  ```
  This writes to `~/Library/Application Support/murmer/models/ggml-base.en.bin`,
  which is murmr's default `model_path` on macOS.

## 2. Configure

Create `~/.config/murmer/config.toml` (murmr prefers this path on macOS):

```toml
[llm]
protocol = "bedrock"
region = "us-west-2"
cleanup_model = "us.anthropic.claude-haiku-4-5-20251001-v1:0"
command_model = "us.anthropic.claude-sonnet-4-20250514-v1:0"

[hotkeys]
dictate = "Super+Shift+K"
command = "Super+Shift+L"
```

If using Bedrock, make sure your AWS credentials are valid (e.g. `aws sts
get-caller-identity` succeeds). No API key goes in the config for Bedrock.

## 3. Build & install

```bash
cargo tauri build --features bedrock
cp -R target/release/bundle/macos/murmer.app /Applications/
open /Applications/murmer.app
```

The build also produces a `.dmg` at
`target/release/bundle/dmg/murmer_0.1.0_aarch64.dmg`.

murmr launches as a **menu-bar agent** (no Dock icon). On first launch it shows its
window; after you close the window it lives in the menu bar. Click the menu-bar icon
(or double-click the app again in Finder) to reopen the window.

## 4. Grant permissions (required)

murmr needs three macOS permissions. Grant them in **System Settings → Privacy &
Security**, then **relaunch murmr** (permissions only take effect after restart):

| Permission | Why | Where |
|------------|-----|-------|
| **Input Monitoring** | detect the global hotkey | Privacy & Security → Input Monitoring |
| **Accessibility** | paste at your cursor (synthetic ⌘V) | Privacy & Security → Accessibility |
| **Microphone** | record your voice | prompted automatically on first record |

Add `/Applications/murmer.app` with the **+** button if it isn't listed, and toggle
it on.

### Note on unsigned local builds

This is an ad-hoc-signed local build. macOS ties permission grants to the app's code
signature, so **rebuilding can drop your grants** and you'll need to re-toggle them.
To reduce this, the build is signed with a stable ad-hoc identity; if a rebuild still
loses permissions, reset and re-grant:

```bash
tccutil reset Accessibility com.arvmaan.murmer
tccutil reset ListenEvent com.arvmaan.murmer
tccutil reset Microphone com.arvmaan.murmer
```

## 5. Use it

- Hold **⌘⇧K**, speak, release → cleaned text pastes at your cursor.
- Start with a mode trigger ("loop this: …", "review this", "spec this") to compile a
  rigorous prompt instead of plain cleanup.
- Hold **⌘⇧L** and speak an instruction for command mode.

## Troubleshooting

Logs are written to `~/Library/Logs/murmr/murmr.log` (GUI apps discard stderr).

- **Hotkey does nothing** → Input Monitoring not granted, or app needs relaunch after
  granting.
- **Records but nothing pastes** → Accessibility not granted.
- **"Bedrock request failed"** → AWS credentials expired; refresh them and relaunch
  murmr (it reads credentials at startup).
- **"no audio recorded"** → the press was too short, or Microphone permission missing.

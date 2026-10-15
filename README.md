<p align="center"><img src="website/icon.png" width="96" alt=""></p>

# Ember

A tiny, fast launcher for Minecraft: Java Edition, built with Tauri.

- Every release and snapshot, plus Beta and Alpha versions, downloaded directly from Mojang
- One-click Fabric, Forge and NeoForge
- **Mod browser**: search Modrinth (and CurseForge with an API key) and install mods plus their dependencies; each loader + game version gets its own mods folder
- **Skin library**: save skins, switch with one click, import any Minecraft or Ely.by player's skin by name
- **FPS Boost**, on by default: Sodium, Lithium, FerriteCore and more in a separate instance, plus tuned JVM flags
- **Shaders** in one click: Iris + Complementary Reimagined on top of FPS Boost
- Correct Java runtime downloaded automatically for each version
- Offline play with **custom skins**, free Ely.by accounts (skins + online servers via authlib-injector), and Microsoft sign-in built in (needs an approved client ID)
- **Rekindle**: signed, one-click self-updates
- **26 languages**, picked automatically (right-to-left included)
- Starts Bedrock Edition on Windows if it's installed

## Download

Get the latest build from [Releases](https://github.com/eymenderdiyok09-dotcom/ember/releases/latest):

| Platform | File |
| --- | --- |
| macOS 11+ (Apple Silicon & Intel) | `Ember_0.6.0_universal.dmg` |
| Windows 10/11 x64 | `Ember_0.6.0_windows-x64.zip` |
| Windows 11 ARM64 | `Ember_0.6.0_windows-arm64.zip` |

The macOS build is signed and notarized. Windows builds aren't signed yet: click **More info → Run anyway**.

## Build

Requires Rust and `cargo install tauri-cli --version "^2"`.

```bash
cd src-tauri
cargo tauri dev                                              # run
cargo tauri build --target universal-apple-darwin --bundles dmg   # macOS
cargo tauri build --target x86_64-pc-windows-gnu --no-bundle     # Windows x64 (needs mingw-w64)
XWIN_CROSS_COMPILER=clang cargo tauri build --runner cargo-xwin \
  --target aarch64-pc-windows-msvc --no-bundle                  # Windows ARM64 (needs cargo-xwin + LLVM)
```

To sign and notarize the macOS build, set `APPLE_SIGNING_IDENTITY="Developer ID Application: …"` for the build, then run `xcrun notarytool submit <dmg> --keychain-profile ember --wait` and `xcrun stapler staple <dmg>`.

To publish a release (builds, signs, notarizes, writes the signed `latest.json` for Rekindle and uploads everything):

```bash
scripts/release.sh 0.5.1 "- First change\n- Second change"
```

To enable CurseForge in the mod browser, build with `EMBER_CURSEFORGE_KEY=<your key from console.curseforge.com>`.

To enable Microsoft sign-in, build with `EMBER_MSA_CLIENT_ID=<your Azure app client ID>`.

## Layout

- `src/`: the UI (vanilla HTML, CSS and JS)
- `src-tauri/src/`: game installer, launcher, Microsoft auth, Bedrock (Windows)
- `website/`: the download page, with a live demo of the UI (`website/sync-demo.sh` refreshes it)

## License

MIT — see [LICENSE](LICENSE). Code signing policy and privacy: https://emberlauncher.app/code-signing.html. Windows signing setup: [docs/SIGNING.md](docs/SIGNING.md).

---

Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.

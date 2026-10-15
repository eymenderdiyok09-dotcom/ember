# Windows code signing (SignPath Foundation)

Ember's Windows builds can be signed for free through SignPath Foundation's
open-source program. Everything on the project side is ready; the application
has to be submitted by the project owner.

## Already in place

- MIT license (`LICENSE`), public repository.
- Windows x64 and ARM64 builds made by GitHub Actions from this repository
  (`.github/workflows/windows.yml`), with a SignPath signing step that switches
  on once SignPath is configured.
- A code signing policy with team roles and a privacy policy:
  https://emberlauncher.app/code-signing.html (linked from the site footer).
- Product name "Ember" and the version in the Windows file metadata
  (`src-tauri/tauri.conf.json`).

## Before applying

1. Turn on **two-factor authentication** for your GitHub account
   (Settings → Password and authentication). SignPath requires it for everyone
   with access.
2. Make sure the latest Windows build workflow run succeeded:
   GitHub → Actions → "Windows builds" → Run workflow.

## Apply

Go to https://signpath.org/apply and use these answers:

| Field | Answer |
| --- | --- |
| Project name | Ember |
| Repository | https://github.com/eymenderdiyok09-dotcom/ember |
| Homepage | https://emberlauncher.app |
| License | MIT |
| Description | Ember is a free, open-source launcher for Minecraft: Java Edition for macOS and Windows. It installs any game version directly from Mojang, adds mod loaders (Fabric, Forge, NeoForge), one-click performance mods and shaders from Modrinth, and supports offline, Ely.by and Microsoft accounts. |
| What gets signed | `Ember.exe` (x64 and ARM64), packaged as zip files, built by GitHub Actions |
| Build system | GitHub Actions (`.github/workflows/windows.yml`) |
| Code signing policy | https://emberlauncher.app/code-signing.html |
| Team | Eymen Derdiyok (@eymenderdiyok09-dotcom) — author, reviewer and approver |

## After approval

1. In SignPath, create:
   - project slug `ember`
   - artifact configuration `windows-zip` (a zip containing `Ember.exe`; sign the exe;
     restrict product name to "Ember")
   - signing policy `release-signing` (manual approval)
   - a CI user and its API token
2. In GitHub → Settings → Secrets and variables → Actions:
   - variable `SIGNPATH_ORGANIZATION_ID` = your SignPath organization id
   - secret `SIGNPATH_API_TOKEN` = the CI user's token
3. Remove the "has been applied for" note from `website/code-signing.html` and
   the "aren't code-signed yet" wording from the FAQ (all languages) and README.
4. Switch the release flow to the signed builds: `scripts/release.sh` should
   download the `Ember-windows-*-signed` artifacts from the tagged workflow run
   (`gh run download`) instead of using the cross-compiled zips, *before* it
   writes and signs `latest.json`. Replacing a release zip after `latest.json`
   is published would make Rekindle reject the update as corrupted.

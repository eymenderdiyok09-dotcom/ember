#!/bin/sh
# Builds, signs and publishes an Ember release, including the signed
# latest.json that Rekindle (the in-app updater) reads.
#
#   scripts/release.sh 0.5.1 "- First change\n- Second change"
#
# Needs: the Developer ID certificate, the "ember" notarytool profile, the
# update key at ~/.ember-signing/update-key.pem, cargo-xwin + LLVM, gh.
set -eu

VERSION="$1"
NOTES="$2"
REPO="eymenderdiyok09-dotcom/ember"
IDENTITY="Developer ID Application: Eymen Derdiyok (J4V5QR4FYJ)"
KEY="$HOME/.ember-signing/update-key.pem"
OPENSSL=/opt/homebrew/opt/openssl@3/bin/openssl
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/opt/homebrew/opt/llvm/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
cd "$ROOT"

test -f "$KEY" || { echo "Missing update key $KEY" >&2; exit 1; }
test -z "$(git status --porcelain)" || { echo "Commit or stash your changes first." >&2; exit 1; }

# 1. Version numbers.
python3 - "$VERSION" <<'PY_VERSION'
import json, re, sys
from pathlib import Path
version = sys.argv[1]
if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
    raise SystemExit("Expected a version such as 0.6.3")
config = Path("src-tauri/tauri.conf.json")
data = json.loads(config.read_text())
data["version"] = version
config.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
cargo = Path("src-tauri/Cargo.toml")
cargo.write_text(re.sub(r'^version = "[^"\n]+"', f'version = "{version}"', cargo.read_text(), count=1, flags=re.M))
for name in ["package.json", "package-lock.json"]:
    path = Path("auth-helper") / name
    data = json.loads(path.read_text())
    data["version"] = version
    if "packages" in data:
        data["packages"][""]["version"] = version
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
# Change download names and the footer, leaving historical version prose alone.
for name in ["website/index.html", "README.md"]:
    path = Path(name)
    text = re.sub(r"Ember_[0-9]+\.[0-9]+\.[0-9]+_", f"Ember_{version}_", path.read_text())
    text = re.sub(r"Ember [0-9]+\.[0-9]+\.[0-9]+ ·", f"Ember {version} ·", text)
    path.write_text(text)
PY_VERSION
sh website/sync-demo.sh

# 2. Builds.
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
# Tauri bundles the pinned prismarine-auth helper using Node/npm.
echo "Microsoft sign-in: prismarine-auth"
cd src-tauri
APPLE_SIGNING_IDENTITY="$IDENTITY" cargo tauri build --target universal-apple-darwin --bundles app,dmg
cargo tauri build --target x86_64-pc-windows-gnu --no-bundle
XWIN_CROSS_COMPILER=clang cargo tauri build --runner cargo-xwin --target aarch64-pc-windows-msvc --no-bundle
cd "$ROOT"

mkdir -p "$DIST/Ember-windows-x64" "$DIST/Ember-windows-arm64"

# 3. Notarize the DMG (its ticket also covers the app inside), staple both.
BUNDLE=src-tauri/target/universal-apple-darwin/release/bundle
DMG="$BUNDLE/dmg/Ember_${VERSION}_universal.dmg"
xcrun notarytool submit "$DMG" --keychain-profile ember --wait | tee /dev/stderr | grep -q "status: Accepted"
xcrun stapler staple "$DMG"
# The DMG's ticket normally covers the app inside; if not, notarize the app on its own.
if [ ! -d "$BUNDLE/macos/Ember.app" ]; then
  MOUNT_DIR=$(mktemp -d /tmp/ember-dmg.XXXXXX)
  hdiutil attach "$DMG" -nobrowse -mountpoint "$MOUNT_DIR"
  cp -R "$MOUNT_DIR/Ember.app" "$BUNDLE/macos/"
  hdiutil detach "$MOUNT_DIR"
  rmdir "$MOUNT_DIR"
fi
if ! xcrun stapler staple "$BUNDLE/macos/Ember.app"; then
  ditto -c -k --keepParent "$BUNDLE/macos/Ember.app" "$DIST/notarize-app.zip"
  xcrun notarytool submit "$DIST/notarize-app.zip" --keychain-profile ember --wait | grep -q "status: Accepted"
  rm -f "$DIST/notarize-app.zip"
  xcrun stapler staple "$BUNDLE/macos/Ember.app"
fi

# 4. Package.
cp "$DMG" "$DIST/"
MAC_ZIP="Ember_${VERSION}_macos.app.zip"
rm -f "$DIST/$MAC_ZIP"
ditto -c -k --keepParent "$BUNDLE/macos/Ember.app" "$DIST/$MAC_ZIP"
cp src-tauri/target/x86_64-pc-windows-gnu/release/mc-launcher.exe "$DIST/Ember-windows-x64/Ember.exe"
cp src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll "$DIST/Ember-windows-x64/"
cp src-tauri/target/aarch64-pc-windows-msvc/release/mc-launcher.exe "$DIST/Ember-windows-arm64/Ember.exe"
X64="Ember_${VERSION}_windows-x64.zip"
ARM="Ember_${VERSION}_windows-arm64.zip"
(cd "$DIST" && rm -f "$X64" "$ARM" && zip -qr "$X64" Ember-windows-x64 && zip -qr "$ARM" Ember-windows-arm64)

# 5. Signed update manifest.
asset() {
  printf '"%s": { "url": "https://github.com/%s/releases/download/v%s/%s", "sha256": "%s", "size": %s }' \
    "$1" "$REPO" "$VERSION" "$2" "$(shasum -a 256 "$DIST/$2" | cut -d' ' -f1)" "$(stat -f%z "$DIST/$2")"
}
NOTES_JSON=$(printf '%b' "$NOTES" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
cat > "$DIST/latest.json" <<EOF
{
  "version": "$VERSION",
  "notes": $NOTES_JSON,
  "platforms": {
    $(asset macos-universal "$MAC_ZIP"),
    $(asset windows-x86_64 "$X64"),
    $(asset windows-aarch64 "$ARM")
  }
}
EOF
python3 -m json.tool "$DIST/latest.json" > /dev/null
$OPENSSL pkeyutl -sign -inkey "$KEY" -rawin -in "$DIST/latest.json" -out "$DIST/latest.json.sig"
(cd src-tauri && REKINDLE_MANIFEST="$DIST/latest.json" REKINDLE_SIG="$DIST/latest.json.sig" \
  cargo test --quiet verifies_release_signature -- --ignored)

# 6. Publish.
git add -A
git commit -q -m "Ember $VERSION

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git push -q
gh release create "v$VERSION" \
  "$DIST/Ember_${VERSION}_universal.dmg" "$DIST/$MAC_ZIP" "$DIST/$X64" "$DIST/$ARM" \
  "$DIST/latest.json" "$DIST/latest.json.sig" \
  --title "Ember $VERSION" \
  --notes "$(printf '%b' "$NOTES")

| Platform | Download |
| --- | --- |
| macOS 11+ (Apple Silicon & Intel) | Ember_${VERSION}_universal.dmg |
| Windows 10/11 x64 | $X64 |
| Windows 11 ARM64 | $ARM |

Already have Ember 0.5.0 or newer? It updates itself (Rekindle)."
echo "Released Ember $VERSION"

# Microsoft sign-in in Ember 0.6.3

Ember uses [prismarine-auth](https://github.com/PrismarineJS/prismarine-auth), pinned to 3.1.1, for the Microsoft → Xbox Live → XSTS → Minecraft Java token chain, including refresh. It uses the library's documented default `live` flow and Minecraft Nintendo Switch title. Microsoft may display that application name during authorization. The account must own Java Edition and have a Minecraft profile; Microsoft's account/family restrictions still apply.

Players select Microsoft and Sign in. Ember prepares the helper, shows a device code, and opens Microsoft's verification page. The code can be copied, the page reopened, and the attempt cancelled. Old attempts cannot overwrite newer ones. Refresh never opens a browser: if interaction is required, Ember asks the player to sign in again.

## Runtime and builds

`auth-helper/package-lock.json` pins npm dependencies. The MSAL dependency uses an override to uuid 11.1.1 for GHSA-w5hq-g745-h8pq; its UUID v4 API remains compatible.  esbuild bundles them into `auth-helper/dist/main.cjs`, embedded in the Rust executable. Tauri's build/dev hooks generate it. Direct Cargo builds/tests require `npm --prefix auth-helper ci --ignore-scripts` and `npm --prefix auth-helper run build` first.

On first Microsoft sign-in, Ember downloads Node **22.23.3** from `https://nodejs.org/dist/v22.23.3/`. SHA-256 hashes for the archives and extracted executable are pinned in `src-tauri/src/microsoft_auth/runtime.rs`. Every use verifies the cached executable. macOS Intel/Apple Silicon and Windows x64/ARM64 are supported, retaining macOS 11 compatibility. Players need no system Node installation. Runtime updates require an Ember update that changes these pinned values.

The runtime is cached under `<minecraft>/ember/microsoft-auth/`. The helper uses private stdin/stdout pipes, no console window on Windows, and a bounded timeout. Cancellation kills and reaps it; closing its parent pipe also stops it. Tokens are never command-line arguments. Node debug logging and `NODE_OPTIONS`/`NODE_PATH` overrides are disabled for this process.

## Credentials

prismarine-auth gets a custom in-memory cache. Rust encrypts the complete cache and Minecraft account using AES-256-GCM, with a random nonce and authenticated format label. Only the random 256-bit key is stored in macOS Keychain or Windows Credential Manager; the encrypted data is saved as `account.v1.enc`. This accommodates caches larger than Windows Credential Manager's per-entry limit. Only the public Minecraft name, UUID and skin URL are returned to the UI. Sign out cancels pending work and deletes cached credentials and the vault key.

Existing Azure-based accounts can keep using an unexpired Minecraft token. Their refresh token cannot be migrated to the library's live flow, so they need one new sign-in when it expires. A successful new sign-in removes the old vault entry. `EMBER_MSA_CLIENT_ID` is no longer used.

## Validation

```sh
npm --prefix auth-helper ci --ignore-scripts
npm --prefix auth-helper run build
npm --prefix auth-helper test
cd src-tauri
cargo test --locked
```

Two opt-in tests contact external services without signing into an account:

```sh
cargo test --locked downloads_and_reuses_node -- --ignored
cargo test --locked prismarine_device_code_smoke -- --ignored
```

The second obtains a device code, validates it, then cancels and terminates the helper. It does not display the code or save an account. End-to-end authorization and launching an online game still require a real Minecraft-owning Microsoft account.

# Ember 0.6.3

- Microsoft sign-in now uses prismarine-auth, with browser device codes and automatic token refresh.
- No separate Azure client ID is needed to build Microsoft sign-in.
- First sign-in downloads and verifies its runtime automatically.
- Credentials stay encrypted, with their key in the operating system's credential vault.
- Cancelling sign-in stops the helper and prevents late responses from completing an old attempt.

Existing Microsoft accounts may need to sign in once again when their old session expires.

Release status: published.

# ADR-007: Credential store unavailable → session-only secrets

Status: Accepted · 2026-10-18

- Passwords are stored with `keyring` 4.x (`v1` feature): macOS Keychain, Windows Credential
  Manager, Linux Secret Service. Key: service `dev.ferrumdata.connection`, account = profile UUID.
- If the platform store fails to initialise, `ferrumdata_credentials::default_store()` falls back
  to an in-memory `SessionStore`. Passwords then last for the process lifetime only. They
  are **never** written to disk, and there is no app-level "encrypted file" with a built-in key.
- The status bar shows which store is active (`Secrets: os-keychain` / `session-memory`).
- Users can always choose "session only" per profile.

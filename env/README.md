# FerrumVariables

A password-locked vault for project environment variables, with encrypted sharing between
developers. Native GPUI desktop app (the UI framework of Zed, a pinned git dependency) on the
shared `ferrum-ui` crate. Part of the Ferrum monorepo, sibling of [FerrumKit](../kit),
[FerrumContainers](../dock) and [FerrumIDE](../editor).

> **Status: alpha.** "Tested" means automated tests: unit tests of `ferrumenv-core` and headless
> GPUI tests of the app (real views, actions and keystrokes, no window shown). The app has
> **not been launched or clicked through by hand** yet.

```bash
cargo env-ui                       # or: cargo run -p ferrumenv-desktop  (binary: ferrumenv)
cargo test -p ferrumenv-core -p ferrumenv-desktop
```

## What it does

- **Locked by default.** The master password is asked on every start; nothing is decrypted
  before. First run: create it twice (minimum 10 characters, strength hint) with a warning that
  it cannot be recovered. **Auto-lock** after 30 min without keyboard or mouse activity
  (5 / 15 / 30 / 60 in Settings), **⌘L** locks at once. The status bar shows "Locks in 29 min".
  Change the master password in Settings (⌘,): the vault is re-encrypted under a new salt.
- **Projects → environments → variables.** A project is linked to a folder (picked in the project
  strip at the top, from the recent Ferrum projects in `~/.ferrum/projects.json`, or a folder
  dialog) or just named. New projects get `development`, `staging`, `production`; add, rename
  and delete environments (tabs). Variables have a key, value, optional comment and a secret
  flag. Sidebar with search.
- **Variable table.** Secret values are masked; reveal per row or all, copy (the clipboard is
  emptied after 30 s if it still holds that value, and at once on lock), add (⌘N), edit
  (double-click or ✎), delete with confirmation.
- **Share key per project.** 256 random bits shown as `fek1_…` (base64url + 4-byte checksum, so
  typos and truncated pastes are rejected) and a fingerprint like `3f9a-07c2` to compare
  out loud. Generate, rotate (with a warning), copy, or set a key received from a teammate.
- **Bundles.** *Share…* encrypts all or chosen environments with the project key into a text
  armor (`-----BEGIN FERRUMENV BUNDLE-----`) to copy, or a `.fenv` file. *Import bundle…* takes
  pasted text or a file, uses the stored key (or one typed in), previews per environment what is
  added / changed / removed (key names only), and imports, optionally removing keys the bundle
  lacks. A bundle of an unknown project creates it and stores the key.
- **.env interop.** Import a project folder's `.env`, `.env.local`, `.env.production`, … (`.env`
  → development, `.env.<name>` → `<name>`; comments, `export `, single / double / backtick
  quotes, escapes and multi-line quoted values). Write an environment to a `.env` file (asks
  before overwriting, mode 0600, offers to add it to `.gitignore`). Copy as `.env` text.
  Compare two environments ("staging lacks STRIPE_KEY", differing values).

## Layout

| Path | What |
| --- | --- |
| `crates/core` (`ferrumenv-core`) | No UI. `model` (vault data), `crypto` (Argon2id, XChaCha20-Poly1305), `store` (vault file, `Session`), `share_key`, `bundle`, `dotenv`, `diff`, `lock` (auto-lock timing), `password` (rules, strength) |
| `apps/desktop` (`ferrumenv-desktop`, binary `ferrumenv`) | GPUI app: `app` (root, lock state, timer, clipboard), `lock_screen`, `unlocked` (strip, sidebar, status bar), `table`, `dialogs/*`, `tests.rs` |

## Security model

**Vault file** `~/.ferrum/env/vault.json`:

```json
{ "format": "ferrumenv-vault", "version": 1,
  "kdf": { "algorithm": "argon2id", "memory_kib": 65536, "iterations": 3, "parallelism": 1,
           "salt": "<16 random bytes, base64>" },
  "cipher": "xchacha20poly1305", "nonce": "<24 random bytes>", "ciphertext": "<base64>" }
```

- Key = Argon2id(master password, salt), 256 bits; defaults m = 64 MiB, t = 3, p = 1 (stored in
  the header, so they can be raised later; tests use cheap parameters). Header parameters are
  bounds-checked so a tampered file can't make unlocking eat memory.
- The serialized vault (projects, keys, values, settings) is sealed with XChaCha20-Poly1305
  under a fresh random nonce on every save. The header (format, version, KDF parameters, salt,
  cipher) is the associated data, so any change to it or to the ciphertext fails to decrypt.
  Nothing but the header is readable on disk, not even project names.
- A wrong password, a tampered file and a newer format are reported as errors, never panics.
- Saves are atomic (temp file, fsync, rename) with mode 0600 in a 0700 folder; the previous
  file is kept as `vault.json.bak` (encrypted the same way).
- Random salts, nonces and keys come from the OS (`getrandom`).

**Bundles** are sealed with XChaCha20-Poly1305 under the project share key, with
`ferrumenv-bundle\0v1\0<project name>` as associated data, so neither the version nor the
project name can be swapped. The armor carries {project, key fingerprint, nonce, ciphertext};
inside are the environments, `created_at` and the sender's note. The key itself is never in a
bundle. A bundle made with another key is reported as such (fingerprints differ); any other
failure as "wrong key or modified". Anyone holding the share key can read *and* create bundles
for that project: it is a shared secret for the team, exchanged once out of band, and rotating
it is the way to cut someone off for future bundles.

**In memory.** Variables, share keys, the session key and decrypted buffers wipe themselves on
drop (`zeroize`), and `Debug` output never contains values. Locking (⌘L, auto-lock) drops the
whole unlocked state: vault, session key, open dialogs. Values are never logged.

**Limits, honestly:**
- Wiping is best effort: copies made by `serde_json`, `Vec` growth, the shared `TextInput`
  (plain `String`s in edit dialogs and the password field) and GPUI's text layout are freed
  but not zeroed. Swap and core dumps are not prevented.
- The clipboard is outside the app's control while a value sits there (clipboard managers may
  keep a copy). Exported `.env` files are plain text by nature.
- No protection against malware running as the same user (keyloggers, memory readers).
- There is no recovery: a forgotten master password means the data is gone.

## Not built yet

Undo, drag-and-drop of `.fenv` / `.env` files, per-variable history, a CLI (`ferrumenv run --
cmd` to inject variables), opening `.fenv` files from Finder, OS keychain / Touch ID unlock,
public-key (per-person) sharing instead of one shared project key.

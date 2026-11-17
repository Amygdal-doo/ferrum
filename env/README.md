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
- **Team keys (no shared password).** Your vault holds an X25519 key pair, created on first
  use. *Team keys* (person icon) shows your public key (`fepk1_…`, not secret: copy it to
  teammates) with a 96-bit fingerprint like `3f9a-07c2-11ab-9e00-4c1d-d2e7`, and lists the
  project's members (name + public key; add, remove). *Share…* then encrypts **to the team
  members' keys** (the default when the project has members or no share key) or with the share
  key. Importing a team bundle uses your own key and shows who sent it ("From Ana", or a warning
  for a key that is not in the project's team). Share-key bundles keep working unchanged.
- **Sync through a shared folder.** *Sync* (↻ icon) links a project to a folder your team
  shares (Dropbox, iCloud Drive, a git checkout…). The app keeps one team bundle there,
  `<project>.fenv`, encrypted to you and the members; every 2 s while unlocked (and no dialog is
  open) it merges files that changed and rewrites the bundle when the vault differs, so edits
  go out within seconds. Merges are three-way against the last synced state: a key changed on one
  side takes that side; a key changed differently on both sides opens **Sync conflicts**, which
  shows both versions (masked until revealed) to choose from. *Later* (or Escape) puts them aside,
  and nothing is written to the folder until they are resolved (*Sync now*). Conflict copies made
  by the sync service (`shop (… conflicted copy).fenv`, `shop 2.fenv`, `shop.sync-conflict-…`)
  are merged too, every difference being a choice. Bundles from a key that is neither yours nor a
  member's are ignored with a notice.
- **.env interop.** Import a project folder's `.env`, `.env.local`, `.env.production`, … (`.env`
  → development, `.env.<name>` → `<name>`; comments, `export `, single / double / backtick
  quotes, escapes and multi-line quoted values). Write an environment to a `.env` file (asks
  before overwriting, mode 0600, offers to add it to `.gitignore`). Copy as `.env` text.
  Compare two environments ("staging lacks STRIPE_KEY", differing values).

## Layout

| Path | What |
| --- | --- |
| `crates/core` (`ferrumenv-core`) | No UI. `model` (vault data), `crypto` (Argon2id, XChaCha20-Poly1305), `store` (vault file, `Session`), `share_key`, `identity` (X25519 key pair, public keys, members), `recipients` (team-bundle sealing), `bundle`, `sync` (shared folder, three-way merge), `dotenv`, `diff`, `lock` (auto-lock timing), `password` (rules, strength) |
| `apps/desktop` (`ferrumenv-desktop`, binary `ferrumenv`) | GPUI app: `app` (root, lock state, timer, clipboard), `lock_screen`, `unlocked` (strip, sidebar, status bar), `table`, `sync` (folder poll), `dialogs/*`, `tests.rs` |

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

**Team bundles** (format v2) are encrypted to public keys instead, in the style of age's X25519
recipients and HPKE's authenticated mode, using X25519 and HKDF-SHA256 from `aws-lc-rs` (already
linked into every Ferrum app through rustls) and the same XChaCha20-Poly1305:
- A random 256-bit file key seals the contents; the associated data binds the format, the
  project name and the sender's public key `S`.
- Per recipient `R`: a fresh ephemeral key `e` (public `E`) and the sender's private key `s` give
  `HKDF-SHA256(salt = E‖S‖R, ikm = X25519(e,R)‖X25519(s,R), info = "ferrumenv-bundle-recipient-v2")`,
  which seals the file key with SHA-256 of the payload (nonce ‖ ciphertext) as associated data.
- The static `X25519(s,R)` term authenticates the sender: only the holder of `s` can make a
  bundle that `R` opens as coming from `S`, so the app can name the sender and warn about
  strangers. Binding the payload hash into each stanza stops one recipient from swapping the
  contents while reusing the sender's stanzas for the others.
- Public keys with a low-order point (all-zero shared secret) are refused; typos are caught by a
  4-byte checksum; the private key lives only inside the encrypted vault. The vault format is
  now version 2, so an older build refuses the vault instead of dropping the key on save.
- Removing a member stops future bundles from reaching them; like with any shared secret, what
  they already received stays readable to them. The armor shows the project name and the
  sender's fingerprint; recipients' public keys are in the envelope (not secret).

**Sync folders** only ever receive team bundles, written atomically (a temp file of ciphertext in
the same folder, then a rename) with mode 0600. Variable names and values never touch the folder;
the file name and armor show the project name and the sender's fingerprint. A file is merged only
if it decrypts for you and its authenticated sender is you or a project member, so someone who can
merely write to the folder can't inject values. The last synced state and the hashes of merged
files are kept in the (encrypted) vault.

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
cmd` to inject variables), opening `.fenv` files from Finder, OS keychain / Touch ID unlock.

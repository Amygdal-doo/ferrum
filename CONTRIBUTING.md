# Contributing to Ferrum

Thanks for helping. Ferrum is a Cargo workspace of native Rust desktop tools
rendered with [GPUI](https://gpui.rs); see [`README.md`](README.md) for the
layout and the products.

## Building

1. Install Rust with [rustup](https://rustup.rs). `rust-toolchain.toml` pins
   the toolchain (with `rustfmt` and `clippy`); rustup installs it on the
   first `cargo` command.
2. Platform prerequisites for GPUI:
   - macOS: Xcode command line tools (Metal shaders are compiled at build
     time).
   - Linux: the system packages listed in
     [`packaging/linux/Dockerfile`](packaging/linux/Dockerfile) (Vulkan,
     Wayland/X11, fontconfig, ALSA, ...).
   - Windows: the MSVC build tools. FerrumTerm and FerrumEngineer have no
     Windows build yet.
3. Run an app from the repository root:

   ```bash
   cargo run -p ferrumgit-desktop      # FerrumGIT
   cargo run -p ferrumterm-gui         # FerrumTerm
   cargo run -p ferrumeditor-desktop   # FerrumIDE
   ```

   Every app is `<app>/apps/*`; the Cargo aliases in `.cargo/config.toml`
   (`cargo git-ui`, `cargo term`, `cargo editor`, ...) are shortcuts for the
   same commands.

GPUI is a git dependency on upstream [Zed](https://github.com/zed-industries/zed),
pinned to one commit in the root `Cargo.toml`. The first build downloads and
compiles it, which takes a few minutes. Dependencies build at `opt-level = 2`
even in dev because GPUI is too slow unoptimized.

## Before you open a pull request

Run these from the repository root; CI and reviewers expect them to pass:

```bash
cargo fmt --all --check
cargo lint                    # clippy on the whole workspace, -D warnings
cargo test --workspace
scripts/check-licenses.sh     # no GPL-family crate in any shipped binary
```

Tests that need external services (a Docker engine, database servers, an
unlocked OS keychain, debuggers) are `#[ignore]`d or skip themselves when the
service is not available; run them with `cargo test -- --ignored` when you
have it.

## Pull requests

- Keep a pull request focused on one change, and explain what it changes and
  why. Link the issue it addresses, if there is one.
- Add or update tests for behaviour you change. Prefer tests against real
  inputs (temporary git repositories, files, local servers) over mocks.
- Follow the existing structure: domain logic in `*/crates/*` (no GPUI),
  UI in `*/apps/*`, shared widgets and theme in `shared/ui` (`ferrum-ui`).
- Update the relevant `README.md` or `docs/` when you change user-visible
  behaviour.
- Dependencies must be permissively licensed (MIT, Apache-2.0, BSD, ISC,
  Zlib, ...). Never add GPL / AGPL / LGPL-only crates, and never copy code
  from Zed's GPL-3.0 or AGPL-3.0 crates (`ui`, `theme`, `editor`, ...); only
  Zed's Apache-2.0 GPUI crates are used. Third-party assets go in
  [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
- `unsafe` code is forbidden workspace-wide.

## Licence of contributions

Ferrum is dual-licensed under the [MIT licence](LICENSE-MIT) and the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in Ferrum by you, as defined in the Apache-2.0
licence, shall be dual-licensed as above, without any additional terms or
conditions. No contributor licence agreement or sign-off is required.

## Conduct and security

Everyone taking part is expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md). Please report security vulnerabilities
privately as described in [`SECURITY.md`](SECURITY.md), not in public issues.

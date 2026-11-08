# Architecture Decision Records

| ID | Title | Status |
|----|-------|--------|
| [ADR-001](ADR-001-dioxus-webview.md) | Dioxus desktop (webview) as the UI runtime | Superseded by ADR-011 |
| [ADR-002](ADR-002-rust-ui-vendoring.md) | Vendor and pin Rust/UI components; pin Dioxus exactly | Superseded by ADR-011 |
| [ADR-003](ADR-003-sql-editor.md) | SQL editor strategy | **Open — spike pending** |
| [ADR-004](ADR-004-result-grid.md) | Result grid virtualization | Accepted (interim), benchmark pending |
| ADR-005 | SSH library choice | Not started (P1) |
| ADR-006 | SQLx version | Accepted: `sqlx = 0.9.0` exact in `Cargo.lock`; MSRV 1.94 drives toolchain 1.99.1 |
| [ADR-007](ADR-007-credential-fallback.md) | Credential store unavailable → session-only | Accepted |
| [ADR-008](ADR-008-pg-cancellation.md) | PostgreSQL cancellation semantics | Accepted |
| ADR-009 | macOS distribution (signing/notarization) | Not started |
| ADR-010 | Licensing strategy | **Open** — no license chosen; avoid GPL deps until decided |
| [ADR-011](ADR-011-gpui.md) | GPUI (from the FerrumIDE fork) as the UI runtime | Accepted |

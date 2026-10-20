# ADR-001: Dioxus desktop (webview) as the UI runtime

Status: Accepted · 2026-10-18

## Context
The spec requires Rust application code with no React, TypeScript, Tauri or Electron.
Dioxus 0.7 desktop renders through the OS webview (WKWebView on macOS, WebView2 on
Windows, WebKitGTK on Linux) and drives it from Rust.

## Decision
Use Dioxus 0.7 desktop. "100% Rust" means **all application and component logic is
Rust**. Accepted non-Rust pieces:

- The OS webview and Dioxus's internal JS glue (shipped by Dioxus, not written by us).
- Tailwind CSS v4, run at build time via `@tailwindcss/cli` (Node). The compiled CSS is
  committed and embedded with `include_str!`, so `cargo run` needs no Node.
- Two one-line `document::eval` snippets: reading the textarea selection and the OS
  dark-mode preference. These are a caret/selection bridge (spec §8 option A), not UI logic.

## Consequences
- UI events cross an IPC boundary between Rust and the webview. High-frequency events
  (scroll, pointer move) must be measured (see ADR-004).
- Database values must render as text nodes only; never inject HTML.
- If OS-native widgets become a hard requirement, re-evaluate Slint/egui/iced.

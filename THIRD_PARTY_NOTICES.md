# Third-party notices

Ferrum is licensed under MIT OR Apache-2.0 (see `LICENSE-MIT` and
`LICENSE-APACHE`). The apps include the following third-party works, each
under its own licence; those licences are unaffected by Ferrum's. Rust crate
dependencies are listed in `Cargo.lock`; `scripts/check-licenses.sh` checks
that none of them is GPL-family.

## Lucide icons

- Files: `shared/ui/assets/icons/*.svg` (embedded in every desktop app)
- Source: <https://lucide.dev>, `lucide-static` 1.72.0 (npm), from
  <https://github.com/lucide-icons/lucide>
- Licence: ISC (some icons derived from Feather, MIT). Full text:
  [`shared/ui/assets/icons/LICENSE`](shared/ui/assets/icons/LICENSE)
- Changes: `play-filled.svg` and `square-filled.svg` are `play.svg` and
  `square.svg` with `fill="currentColor"`.

## IBM Plex Sans

- Files: `shared/ui/assets/fonts/ibm-plex-sans/*.ttf` (the UI font)
- Source: <https://github.com/IBM/plex>, release `@ibm/plex-sans@1.1.0`
- Licence: SIL Open Font License 1.1, Copyright © 2017 IBM Corp. with
  Reserved Font Name "Plex". Full text:
  [`shared/ui/assets/fonts/ibm-plex-sans/OFL.txt`](shared/ui/assets/fonts/ibm-plex-sans/OFL.txt)
- Unmodified.

## Lilex

- Files: `shared/ui/assets/fonts/lilex/*.ttf` (the code font)
- Source: <https://github.com/mishamyrt/Lilex>, release 2.630
- Licence: SIL Open Font License 1.1, Copyright 2019 The Lilex Project
  Authors. Full text:
  [`shared/ui/assets/fonts/lilex/OFL.txt`](shared/ui/assets/fonts/lilex/OFL.txt)
- Unmodified.

## Agent and model logos (trademarks)

- Files: `shared/ui/assets/logos/*.png` (embedded in the desktop apps) and
  their copies in `web/assets/img/ai/`
- Logos of Anthropic (Claude), OpenAI (ChatGPT, Codex), Google (Gemini),
  OpenCode, Meta (Muse), DeepSeek, Moonshot AI (Kimi), Alibaba Cloud (Qwen),
  Z.ai (GLM), Mistral AI, Ollama, Cursor, Amp and goose. Sources, dates and
  each owner's guidelines: [`shared/ui/assets/logos/README.md`](shared/ui/assets/logos/README.md)
- These names and logos are trademarks of their respective owners. They are
  used only to identify compatible services and products the apps work with;
  no endorsement by or affiliation with their owners is implied. They are not
  covered by Ferrum's MIT / Apache-2.0 licence.
- Changes: scaled to 128 px and cropped to the symbol; black marks also get
  a light version for dark themes. No recolouring or redrawing.

## GPUI

- Crates: `gpui`, `gpui_platform` and the Zed crates they pull in
  (`gpui_apple`, `gpui_ios`, `gpui_linux`, `gpui_macos`, `gpui_macros`,
  `gpui_shared_string`, `gpui_util`, `gpui_web`, `gpui_wgpu`, `gpui_windows`,
  `collections`, `derive_refineable`, `refineable`, `http_client`,
  `scheduler`, `sum_tree`, `ztracing`, `ztracing_macro`), used as a library,
  unmodified.
- Source: <https://github.com/zed-industries/zed>, commit
  `03efd49220ba06703d04f9c4732fdf814f9c523c` (Cargo git dependency).
- Licence: Apache-2.0, Copyright 2022 - 2025 Zed Industries, Inc. Notice:
  <https://github.com/zed-industries/zed/blob/03efd49220ba06703d04f9c4732fdf814f9c523c/LICENSE-APACHE>;
  licence text as in this repository's [`LICENSE-APACHE`](LICENSE-APACHE).
  Zed's GPL-3.0 / AGPL-3.0 crates are not used.

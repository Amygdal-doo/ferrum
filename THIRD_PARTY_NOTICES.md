# Third-party notices

The Ferrum apps are proprietary ("All rights reserved"). They include the
following third-party works, each under its own licence. Rust crate
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

## GPUI

- Crates: `gpui`, `gpui_platform` and their internal crates from the `ide/`
  submodule (the Zed codebase), used as a library.
- Licence: Apache-2.0 (`ide/LICENSE-APACHE`).

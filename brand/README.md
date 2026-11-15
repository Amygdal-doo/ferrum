# Ferrum brand

One core mark, with a variant for each app.

## Concept

Every Ferrum logo is a flat **element tile**, like a periodic-table cell for
iron:

- **Tile:** a dark rounded square (`#0d1318`), corner radius about 24% of the side.
- **Border:** a thin line in the app's accent colour (1.5px at 46px).
- **Symbol:** `Fe` in near-white (`#eef2f5`) plus the app's capital in its accent,
  in bold geometric letters, centred on one line: `FeI`, `FeN`, `FeG`, `FeT`,
  `FeA`, `FeC`, `FeD`, `FeV`, `FeP`, `FeK`, `FeS`, `FeL`. The capital is the app
  name's (FerrumIDE, FerrumVariables, FerrumEngineer), so no two apps share a
  letter. The core mark is `Fe` with an accent `e`. Every app's letters are
  the same size. Renamed apps keep their accent and file name (the app id).

The letters are drawn as stroked paths, so nothing depends on installed fonts.
Every tile works on dark and light backgrounds, because it carries its own
dark ground.

### Masters

`generate_svg.py` writes three versions of each logo:

| File | Use |
| --- | --- |
| `svg/<name>.svg` | The flat tile, full bleed: the website, in-app logos, PNGs from 64px up, `.ico`, Linux icons |
| `svg/small/<name>.svg` | The same tile for 16–32px: a heavier border and bigger, bolder letters |
| `svg/macos/<name>.svg` | The macOS app icon: the tile on Apple's icon grid (an 824px rounded square centred in the 1024 canvas, with a soft shadow), for the `.icns` from 64px up |

`cargo brand` picks the small master for every output of 32px or less
(PNGs, `.ico` and `.icns` entries) and the macOS master for the larger
`.icns` entries.

## Palette

| Role | Hex |
| --- | --- |
| Tile | `#0d1318` |
| Letters (`Fe`, wordmark) | `#eef2f5` |
| Light-background wordmark | `#0d1318` |
| Ferrum (core): rust | `#E8743B` |
| API: patina | `#3FB8A0` |
| GIT: brass | `#F0B43C` |
| Data: amethyst | `#A07CF0` |
| Term: phosphor | `#7CC85A` |
| Launcher: ember rose | `#E85A7E` |
| Kit: orchid | `#D46BE0` |
| Containers: glacier | `#38BDF8` |
| IDE: citron | `#C5DC3A` |
| Ports: cobalt | `#5B8DEF` |
| Variables: vermilion | `#F2665A` |
| Engineer: aurora | `#4CD97B` |
| System: aqua | `#2DD4BF` |

The rust and patina colours match the website tokens (`web/assets/css/tokens.css`).

## Marks

| File | Symbol | App |
| --- | --- | --- |
| `ferrum.svg` | Fe | the core mark |
| `editor.svg` | FeI | FerrumIDE (formerly Ferrum Editor; the file keeps the app id `editor`) |
| `vibe.svg` | FeN | FerrumEngineer (formerly FerrumVibe; file `vibe`) |
| `git.svg` | FeG | FerrumGIT |
| `term.svg` | FeT | FerrumTerm |
| `api.svg` | FeA | FerrumAPI |
| `dock.svg` | FeC | FerrumContainers (file `dock`) |
| `data.svg` | FeD | FerrumData |
| `env.svg` | FeV | FerrumVariables (formerly FerrumEnv; file `env`) |
| `ports.svg` | FeP | FerrumPorts |
| `kit.svg` | FeK | FerrumKit |
| `system.svg` | FeS | FerrumSystem |
| `launcher.svg` | FeL | FerrumLauncher |
| `ferrum-wordmark.svg` | Fe + "Ferrum" | light lettering, for dark backgrounds |
| `ferrum-wordmark-light.svg` | Fe + "Ferrum" | dark lettering, for light backgrounds |

## Outputs

- `png/<name>-<size>.png` at 16, 32, 64, 128, 256, 512 and 1024 px tall
  (the wordmarks keep their aspect ratio).
- `icons/<name>.icns` for macOS app bundles, built with `iconutil`.
- `icons/<name>.ico` for Windows (PNG entries, 16–256 px).

The outputs are committed, because CI packaging and the website use them directly.

## Regenerating

```bash
python3 brand/generate_svg.py brand/svg   # only if you changed the generator
cargo brand                                # SVG → PNG / ICNS / ICO
```

To add an app, add an entry to `APPS` in `generate_svg.py` (its capital and
accent colour; a capital not yet in `GLYPHS` needs its centre lines there),
then run both commands and copy `svg/*.svg` to `web/assets/img/brand/`. You can also edit the SVGs by hand; `cargo brand` renders whatever
is in `svg/`. `.icns` files are only built on macOS.

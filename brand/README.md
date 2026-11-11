# Ferrum brand

One core mark, with a variant for each app.

## Concept

Every Ferrum logo is an **element tile**, in the style of a periodic-table cell for
iron. The tile has these parts:

- **Tile:** a dark, warm iron-black rounded square (`#2b241c` → `#14110d`) with a
  soft accent glow in the top-left corner.
- **Accent ring:** an inset border in the product's accent colour.
- **Iron lattice:** two concentric hexagons with nodes at the outer vertices.
  This is iron's crystal structure, drawn faintly behind the letters.
- **Symbol:** monoline, geometric letters. The core mark is `Fe` (iron) with
  a light "ingot" `F` (`#f1e9df`) and an accent `e`. Each app is `Fe` in ingot
  ink plus a capital in its accent colour: `FeI`, `FeA`, `FeG`, `FeD`, `FeT`,
  `FeX`, `FeK`, `FeC`, `FeE`.
- **Job badge** (app variants only): a circle in the accent colour that
  overlaps the top-right of the ring and shows the app's job in dark ink. The
  core mark has no badge.

The letters are drawn as stroked paths, so nothing depends on installed fonts.
Every tile works on dark and light backgrounds, because the tile carries its
own dark ground.

## Palette

| Role | Hex |
| --- | --- |
| Tile top / bottom | `#2b241c` / `#14110d` |
| Ingot ink (the `F`, wordmark) | `#f1e9df` |
| Dark ink (badge glyphs, light wordmark) | `#16130f` / `#1f1913` |
| Ferrum (core): rust | `#E8743B` |
| API: patina | `#3FB8A0` |
| GIT: brass | `#F0B43C` |
| Data: amethyst | `#A07CF0` |
| Term: phosphor | `#7CC85A` |
| Launcher: ember rose | `#E85A7E` |
| Kit: orchid | `#D46BE0` |
| Dock: glacier | `#38BDF8` |
| Editor: citron | `#C5DC3A` |

The rust and patina colours match the website tokens (`web/assets/css/tokens.css`).

## Marks

| File | Symbol | Badge glyph |
| --- | --- | --- |
| `ferrum.svg` | Fe | none (core mark) |
| `api.svg` | FeA | request/response arrows |
| `git.svg` | FeG | branch |
| `data.svg` | FeD | database cylinder |
| `term.svg` | FeT | prompt `>_` |
| `launcher.svg` | FeX | 2×2 app grid |
| `kit.svg` | FeK | two sliders |
| `dock.svg` | FeC | stacked containers |
| `editor.svg` | FeE | lines of text and an I-beam cursor |
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

To add an app, add an entry to `APPS` in `generate_svg.py` (second letter,
accent colour and badge glyph), add its glyph to `glyph()`, then run both
commands. You can also edit the SVGs by hand; `cargo brand` renders whatever
is in `svg/`. `.icns` files are only built on macOS.

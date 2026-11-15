# Ferrum web design system: "tempered steel"

Ferrum is iron, element 26. The site looks like worked steel. Dark blued steel is the
default theme. The accents are the temper colours steel turns as it heats: straw,
bronze, violet, blue. Every app keeps its own brand colour. Titles are set in a
condensed display face that reads like stamped markings.

The system has two halves:

- **Tokens and CSS**: `assets/css/tokens.css` is the only file with raw colour values.
- **maud components**: `src/components/`. Pages put components together and add no
  one-off chrome of their own.

## Tokens (`assets/css/tokens.css`)

| Group | Tokens | Rule |
| --- | --- | --- |
| Surfaces | `--bg` < `--bg-elev` < `--surface` < `--surface-2`; `--border`, `--border-strong` | Each step is a little lighter (dark theme). Use `--bg-elev` for bands, the footer and code. |
| Text | `--text` (16:1 on `--bg`), `--text-muted` (8.2:1), `--text-faint` (6.1:1; 5.0:1 in light) | All three pass WCAG AA for body text in both themes. Use faint only for meta text. |
| Action | `--accent`, `--accent-hover`, `--accent-text`, `--link`, `--focus` | Straw in dark, ink with straw text in light. |
| Status | `--ok`, `--info`, `--warn`, `--security` | Callouts and chips only. |
| Temper | `--straw`, `--bronze`, `--violet`, `--blue`, `--temper` (gradient) | `--temper` is the signature rule. Use it sparingly. |
| Apps | `--app-api`, `--app-git`, … and `--app-accent` | `Product::accent_style()` sets `--app-accent` inline. Use it for logos, rules, dots and glows, never for body text. |
| Type | `--font-display` (Big Shoulders Display), `--font-sans` (Hanken Grotesk), `--font-mono` (JetBrains Mono, ligatures off) | Mono is only for code, commands and keys. Never use it for labels. |
| Type scale | `--fs-xs` 12 · `--fs-sm` 14 · `--fs-base` 17 · `--fs-lg` 19 · `--fs-xl` 24 · `--fs-2xl` (h2) · `--fs-3xl` (h1) · `--fs-hero` | Display sizes are fluid. `--measure` is 68ch for reading. |
| Space | `--sp-1` … `--sp-32` on a 4px grid | Sections use `--sp-24`; `--tight` uses `--sp-12`. |
| Shape | `--radius-xs` 2 (keys) < `--radius-sm` 3 < `--radius` 6 (buttons, cards) < `--radius-lg` 12 (app windows); `--rule`, `--rule-strong` | Radius grows with the size of the thing. |
| Motion | `--ease`, `--dur-fast`, `--dur`, `--dur-slow` | Motion responds to an action. The home hero's "cooling" headline is the one entrance animation, and the home wall of app windows the one ambient motion (with a pause button). `prefers-reduced-motion` turns all motion off (`base.css`). |
| Layout | `--container` 1240, `--container-narrow`, `--gutter`, `--header-h`, `--z-header`, `--z-skip` | |

## Components (`src/components/`)

| Component | Use | CSS |
| --- | --- | --- |
| `page_header::PageHeader` | The top of every inner page: breadcrumb, the page's **only** `<h1>`, lead, actions, optional media (an app logo) and app accent. The temper rule along its bottom edge is the signature. | `.page-header` |
| `breadcrumb::{Crumb, render}` | Trail after Home. The same crumbs go to `PageMeta::trail`, which emits `BreadcrumbList` JSON-LD. | `.breadcrumb` |
| `section::Section` | A labelled `<section>` with an `<h2>` and an optional lead. `.tight()`, `.band()` (raised background), `.hidden_title()`. | `.section` |
| `feature_grid::render` | (title, text) pairs as `h3` + text, with rules above. | `.feature-list` |
| `card::product_card` | The app card. The whole card is clickable and the app's colour shows on hover. | `.card--product` |
| `card::app_strip(apps, with_kind)` | Compact logo + name links to apps. | `.app-strip` |
| `card::grouped_app_strips(with_kind)` | The apps in their two groups (`products::Group`: Apps, then Tools), each a labelled strip. Logos are full-bleed SVG tiles with their own border and radius: never add a mask, plate, padding or glow. | `.app-groups` |
| `card::steps` | A real sequence, so numbered. | `.steps` |
| `callout::render(Kind, title, body)` | `Note`, `Warn`, `Security`. Not a heading, so the outline stays intact. | `.callout--*` |
| `code::block`, `code::command`, `code::keys("⌥⌘O")` | Code blocks, a one-line command with a copy button, and key combos drawn as key caps with spoken text ("Option Command O"). Chords are written with spaces: `"⌘K →"`. | `.code-block`, `.install-cmd`, `.keys` |
| `cta::band`, `cta::get_launcher()` | Closing call to action. The footer hides its own download button when a page ends with one (home's closing section uses `.cta` too). | `.cta` |
| `button::link`, `badge::status` | Buttons (`Primary`, `Secondary`, `Ghost`) and the status badge. | `.btn`, `.badge` |

Page-level CSS: `pages.css` (home, `/apps`, product pages, about, 404, download), `docs.css`
(docs, changelog, FAQ, security, compare, licences), `features.css` (animated demo windows),
`updates.css` (posts).

## Rules

- **One `<h1>` per page**, always from `PageHeader` (or the home hero). Headings go in
  order: sections use `h2`, items inside them use `h3`.
- **No eyebrows.** Don't put a small label above a heading.
- **Numbers only for real sequences** (`steps`, `prose-steps`).
- **Left-aligned.** Reading columns stay within `--measure`. Tables and grids may use
  the full width.
- **Accessible by default.** Focus is always visible (`:focus-visible`, 2px `--focus`).
  Keep a skip link. Decorative images get `alt=""`. Keys have spoken text. Tables have
  captions and `scope`. Check pages at 390px and 1440px in both themes.
- **Copy is factual.** Every claim about an app comes from its README or code. Say
  what isn't built yet.

## Home and `/apps`

- Home (`pages/home.rs`, `pages/home_readme.rs`), top to bottom:
  1. Hero: one headline beside a short intro, Download for macOS and View source.
  2. The wall: every app's animated demo window (`feature_demos`, `project_demos`,
     `tool_demos`) in two rows drifting in opposite directions. It's the page's one
     bold element. Each row holds its tiles twice for a seamless loop; the wall is one
     `role="img"`; hover or the "Pause motion" button pauses it; reduced motion stops it.
     Under it, the platform note and three stats.
  3. An agent at work: FerrumEngineer's demo beside the four real steps it shows.
  4. Shell commands (`fei .`, `feg .`, `fet`, `fen .`, `fterm …`) with one line each.
  5. A README with a plain anchor index (not numbered: the topics aren't a
     sequence): native, the launcher, agents, the project bar, local, open source,
     questions.
  6. The closing download: the headline again, Download, the install command.
- The header has the GitHub mark (with its name from 1100px) next to Download. Under
  480px the mark moves into the menu as "Source on GitHub".
- The footer: brand, tagline and licence line, then Product, Learn and Community
  (GitHub links only).
- `/apps` (`pages/apps.rs`) is the one list of every app, in two groups (Apps, Tools)
  and the launcher: logo, one line, key features, link. The nav's "Apps" item points there. Facts come from each app's
  README and `catalog.json`; don't list FerrumAPI protocols that haven't shipped.

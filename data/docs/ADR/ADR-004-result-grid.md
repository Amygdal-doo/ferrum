# ADR-004: Result grid virtualization

Status: Accepted (interim) · benchmark numbers pending · 2026-10-18

## Decision
- Custom grid in `apps/desktop/src/components/result_grid/` driven by Dioxus `onscroll`
  and `onresize` events (no `web_sys`). Rust/UI's grid is web-only (ADR-002).
- Pure viewport math in `ferrum-result-grid-core::visible_range`; a unit test shows the
  number of rendered rows is bounded by `viewport/row_height + 2·overscan + 1` for 100k rows.
- Rows live in a `ResultBuffer` capped by the client fetch limit (default 10,000; settings
  `query_limits.max_rows`). The driver stops fetching at the limit, marks the result
  truncated, and the UI says so. User SQL is never rewritten.
- Components receive the tab id, never the buffer, so Dioxus prop clone/compare never
  touches row data.
- Fixed row height (26 px), fixed column width (180 px), sticky header. Columns are not
  virtualized yet (fine up to roughly 100 columns).

## Spike harness
The empty results panel has "Load 100k synthetic rows (grid benchmark)", which fills a
buffer without a database.

## Still to measure
Scroll smoothness (each scroll event is an IPC round trip), memory with 100k × 5 cells,
and time to first paint after the first chunk. Record numbers here. Spill-to-disk and
page-on-demand fetching are follow-ups if limits above 100k are needed.

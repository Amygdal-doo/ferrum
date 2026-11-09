//! UI-agnostic result grid model.
//!
//! The grid renders only rows inside the viewport (plus overscan). Rows live
//! in a [`ResultBuffer`] capped by the client fetch limit; nothing here grows
//! without bound.

mod buffer;
pub mod export;
mod selection;
pub mod sort;
mod viewport;
mod widths;

pub use buffer::{PushOutcome, ResultBuffer};
pub use export::{ExportFormat, cell_text, export_string, write_export};
pub use selection::{CellPos, GridSelection};
pub use sort::{SortDirection, sorted_order};
pub use viewport::{Viewport, visible_range};
pub use widths::{
    MAX_COL_WIDTH, MAX_RESIZED_WIDTH, MIN_COL_WIDTH, MIN_RESIZED_WIDTH, WIDTH_SAMPLE_ROWS,
    estimate_widths,
};

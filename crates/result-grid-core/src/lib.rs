//! UI-agnostic result grid model.
//!
//! The grid renders only rows inside the viewport (plus overscan). Rows live
//! in a [`ResultBuffer`] capped by the client fetch limit; nothing here grows
//! without bound.

mod buffer;
mod viewport;
mod widths;

pub use buffer::{PushOutcome, ResultBuffer};
pub use viewport::{Viewport, visible_range};
pub use widths::{MAX_COL_WIDTH, MIN_COL_WIDTH, WIDTH_SAMPLE_ROWS, estimate_widths};

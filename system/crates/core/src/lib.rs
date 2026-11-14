//! Ferrum System (FeS) core: the architecture model saved in a project's
//! `.architecture/` folder, the catalogue of component kinds, technologies
//! and clouds, the demo systems, and live Kubernetes / Docker status.

pub mod catalog;
pub mod demos;
pub mod model;
pub mod runtime;
pub mod store;

pub use catalog::{Category, Kind, Provider, Tech};
pub use model::*;

//! web-ferrum: server-rendered marketing site for the Ferrum product family.

pub mod components;
pub mod config;
pub mod layout;
pub mod pages;
pub mod products;
pub mod releases;
pub mod routes;
pub mod seo;
pub mod site;
pub mod updates;

pub use config::Config;
pub use routes::app;

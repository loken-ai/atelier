//! New dashboard UI system
//!
//! A top bar, a sidebar and a content area.

pub mod components;
pub mod layout;
pub mod models;
pub mod settings;
// Consumed by the chrome and the views as they move onto the surfaces.
#[allow(dead_code)]
pub mod surface;

#[path = "app/core.rs"]
pub mod core;
#[path = "app/input.rs"]
pub mod input;
#[path = "app/state.rs"]
pub mod state;
#[path = "app/ui.rs"]
pub mod ui;

pub use state::ImageViewerApp;

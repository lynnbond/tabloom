pub mod editor_region;
pub mod find_replace;
pub mod shortcuts;
pub mod text;
pub mod theme;

#[cfg(target_os = "macos")]
pub(crate) mod native_menu;

pub mod markdown;

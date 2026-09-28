#[cfg(not(windows))]
mod portable;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use portable::{init, speak, speak_queued};
#[cfg(windows)]
pub use windows::{fedra_in_foreground, init, speak, speak_queued};

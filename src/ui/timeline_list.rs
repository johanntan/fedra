#[cfg(not(windows))]
mod portable;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use portable::TimelineList;
#[cfg(windows)]
pub use windows::TimelineList;

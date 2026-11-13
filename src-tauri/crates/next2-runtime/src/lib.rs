//! Generic package verification and native process transport; no business services.
pub mod package;
#[cfg(windows)]
mod process;
pub mod resource;
#[cfg(windows)]
pub mod tool;
pub mod transport;
#[cfg(windows)]
pub mod worker;

//! Compile-time platform classification for application services.
//!
//! Keep this deliberately small: the desktop implementation remains the current application,
//! while future mobile work can select native services without treating every non-Linux target as
//! a desktop target.

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub(crate) mod desktop;

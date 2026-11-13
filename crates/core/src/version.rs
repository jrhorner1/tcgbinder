//! The app's version.

/// The release version. Release builds stamp it from the tag (TCGBINDER_VERSION);
/// otherwise it is the crate version.
pub const VERSION: &str = match option_env!("TCGBINDER_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// The one-line version shown by --version, e.g. "tcgbinder 0.1.0 macos/aarch64".
pub fn describe(name: &str) -> String {
    format!("{name} {VERSION} {}/{}", std::env::consts::OS, std::env::consts::ARCH)
}

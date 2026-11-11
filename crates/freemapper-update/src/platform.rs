//! Platform names as used in release asset names and manifest keys.

/// The five platforms the suite releases for.
pub const PLATFORMS: [&str; 5] = [
    "linux-x86_64",
    "linux-arm64",
    "macos-arm64",
    "macos-x86_64",
    "windows-x86_64",
];

/// The platform name of this build, or `None` on a system without a release asset.
pub fn current_platform() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux-arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos-arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("macos-x86_64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_name_is_one_of_five() {
        if let Some(name) = current_platform() {
            assert!(PLATFORMS.contains(&name), "{name}");
        }
        // The hosts we develop and test on all have an asset.
        if cfg!(any(
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(
                target_os = "macos",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(target_os = "windows", target_arch = "x86_64"),
        )) {
            assert!(current_platform().is_some());
        }
    }
}

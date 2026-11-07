//! `nros version` — Phase 111.A.12.

use eyre::Result;
use nros_launcher::pin::{self, VersionOrigin};

pub fn run() -> Result<()> {
    println!("nros {}", release_version());
    // phase-429 W2 — the release version above answers "which build is this";
    // this answers "what does it emit, and what runtime can compile that". They
    // are different questions, and conflating them is what the old ABI guard
    // did. `nros --codegen-version` prints the number on its own, for scripts.
    println!("codegen version {}", crate::abi_guard::EMITTED_VERSION);
    // phase-478 I7 — and "which grammar judged this contract": the vendored
    // play_launch commit and the ros-launch-manifest tag. Here, not on
    // `--version`, because `install.sh`, `check-release-manifest.py` and the
    // launcher's dispatch read `--version` as one line.
    println!("{}", crate::BUILD_INFO);
    Ok(())
}

/// The version a user compares against an installer line or a pin: the STORE
/// version (`0.6.0-nros8`) when this binary is a release, else the crate
/// version with the honest reason it has no release version.
///
/// Issue 2334 — this printed the crate version (`0.6.0`) on a release, so an
/// instructor checking seats could not tell `-nros6` from `-nros7`. The store
/// version comes from [`pin::running_version`], the one reader `nros pin`,
/// `nros doctor` and the launcher's dispatch already share; nothing here reads
/// a second source. `--version` keeps the bare crate version, because
/// `install.sh` and the release workflow read it as one line.
#[must_use]
pub fn release_version() -> String {
    let running = std::env::current_exe()
        .ok()
        .and_then(|exe| pin::running_version(&exe));
    describe(
        env!("CARGO_PKG_VERSION"),
        running,
        env!("NROS_CLI_SOURCE_STAMP"),
    )
}

/// Pure half of [`release_version`], so both arms are unit-testable without an
/// installed store.
fn describe(crate_version: &str, running: Option<(String, VersionOrigin)>, stamp: &str) -> String {
    match running {
        Some((v, _)) => v,
        // A checkout build corresponds to no release, so it names none — the
        // source stamp is what identifies it (`just check cli-fresh` compares
        // the same value), never an invented `-nrosN` suffix.
        None => {
            let short: String = stamp.chars().take(12).collect();
            if short.is_empty() || short == "unknown" {
                format!("{crate_version} (checkout build, no release version)")
            } else {
                format!("{crate_version} (checkout build, no release version; sources {short})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_prints_its_store_version_with_the_suffix() {
        let got = describe(
            "0.6.0",
            Some(("0.6.0-nros8".to_string(), VersionOrigin::StorePrefix)),
            "abc",
        );
        assert_eq!(got, "0.6.0-nros8");
        let got = describe(
            "0.6.0",
            Some(("0.6.0-nros7".to_string(), VersionOrigin::VersionFile)),
            "abc",
        );
        assert_eq!(got, "0.6.0-nros7");
    }

    #[test]
    fn a_checkout_build_says_it_has_no_release_version() {
        let got = describe("0.6.0", None, "0123456789abcdef0123");
        assert_eq!(
            got,
            "0.6.0 (checkout build, no release version; sources 0123456789ab)"
        );
        assert!(!got.contains("-nros"), "never invent a suffix: {got}");
        let got = describe("0.6.0", None, "unknown");
        assert_eq!(got, "0.6.0 (checkout build, no release version)");
    }

    #[test]
    fn the_store_prefix_names_the_release_end_to_end() {
        let dir = std::env::temp_dir().join(format!("nros-version-{}", std::process::id()));
        let exe = dir.join("sdk/nros/0.6.0-nros8/bin/nros");
        let running = pin::running_version(&exe);
        assert_eq!(describe("0.6.0", running, "x"), "0.6.0-nros8");
    }
}

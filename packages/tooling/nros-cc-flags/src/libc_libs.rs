//! Which C-library pieces a hosted build script names with `-l`, per TARGET
//! (issue 2844).
//!
//! On glibc, POSIX threads (`libpthread`) and the realtime clock/timer calls
//! (`librt`) were historically separate archives, so a build script that links
//! C code using them says `cargo:rustc-link-lib=pthread` (and `rt`). glibc 2.34
//! folded both into `libc.so` but still ships empty stubs, so naming them stays
//! harmless there.
//!
//! musl never split them: everything is in `libc.a`, and a musl sysroot ships
//! no `libpthread.a` or `librt.a` at all. Rust's self-contained musl sysroot
//! (`-C link-self-contained=yes`, the default for `*-linux-musl*`) is one of
//! those, so a single `-lpthread` fails the final link of every musl image:
//!
//! ```text
//! rust-lld: error: unable to find library -lpthread
//! ```
//!
//! The decision is a fact about the TARGET's C library, so it is read from
//! cargo's `CARGO_CFG_TARGET_ENV`, which describes the target a build script
//! builds for, never from `cfg!(target_env)`, which describes the host the
//! build script itself runs on.

/// Whether a target with this `target_env` keeps the POSIX threads and
/// realtime interfaces inside libc itself, so `-lpthread` / `-lrt` must not be
/// named. True for every musl triple (`*-linux-musl`, `*-musleabi`,
/// `*-musleabihf`, `*-muslabi64`: cargo reports `target_env = "musl"` for all
/// of them).
pub fn libc_bundles_thread_libs(target_env: &str) -> bool {
    target_env == "musl"
}

/// The `cargo:` lines that link `libs`, for a target with `target_env`:
/// nothing on a target whose libc already carries them, one
/// `cargo:rustc-link-lib=<lib>` per entry otherwise.
pub fn thread_lib_lines(target_env: &str, libs: &[&str]) -> Vec<String> {
    if libc_bundles_thread_libs(target_env) {
        return Vec::new();
    }
    libs.iter()
        .map(|lib| format!("cargo:rustc-link-lib={lib}"))
        .collect()
}

/// Link `libs` (`"pthread"`, `"rt"`) for the TARGET this build script builds
/// for, unless its libc already carries them. Call only from a build script:
/// it reads `CARGO_CFG_TARGET_ENV` and prints `cargo:` lines.
///
/// ```no_run
/// nros_cc_flags::libc_libs::link_thread_libs(&["pthread"]);
/// ```
pub fn link_thread_libs(libs: &[&str]) {
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    for line in thread_lib_lines(&target_env, libs) {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musl_targets_name_no_thread_libs() {
        assert!(thread_lib_lines("musl", &["pthread", "rt"]).is_empty());
    }

    #[test]
    fn glibc_and_unset_targets_keep_their_thread_libs() {
        // `gnu` is glibc Linux; an empty env is what cargo reports for most
        // bare and BSD targets. Both link exactly what they did before.
        for env in ["gnu", ""] {
            assert_eq!(
                thread_lib_lines(env, &["pthread", "rt"]),
                ["cargo:rustc-link-lib=pthread", "cargo:rustc-link-lib=rt"],
                "target_env = {env:?}"
            );
        }
    }

    #[test]
    fn only_musl_is_treated_as_bundling() {
        for env in ["gnu", "", "uclibc", "newlib", "msvc", "sgx"] {
            assert!(!libc_bundles_thread_libs(env), "target_env = {env:?}");
        }
        assert!(libc_bundles_thread_libs("musl"));
    }
}

//! Issue 1640 — whether a heap refusal halts through the port's fatal hook.
//!
//! `NROS_HEAP_EXHAUSTION_IS_FATAL` (`0` / `n` / empty = off), defaulting to
//! `NROS_BOOT_REPORT`: an image that asked for the boot record expects to be
//! read after it stops, and one that did not is a development image where the
//! NULL-and-log it has always had is right. The same rule `nros-node`'s
//! `build.rs` applies to `NROS_ARENA_EXHAUSTION_IS_FATAL` and Zephyr's Kconfig
//! to `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL`.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(nros_heap_exhaustion_fatal)");
    println!("cargo:rerun-if-env-changed=NROS_HEAP_EXHAUSTION_IS_FATAL");
    println!("cargo:rerun-if-env-changed=NROS_BOOT_REPORT");
    let truthy = |v: &str| !v.is_empty() && v != "0" && v != "n";
    let fatal = match std::env::var("NROS_HEAP_EXHAUSTION_IS_FATAL") {
        Ok(v) => truthy(&v),
        Err(_) => std::env::var("NROS_BOOT_REPORT").is_ok_and(|v| truthy(&v)),
    };
    if fatal {
        println!("cargo:rustc-cfg=nros_heap_exhaustion_fatal");
    }
}

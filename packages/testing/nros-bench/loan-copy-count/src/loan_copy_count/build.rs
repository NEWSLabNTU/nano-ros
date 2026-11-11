fn main() {
    nros_build_paths::link_script!("memory.x");
    // phase-510 W5 — route every byte copy through the counter in `src/lib.rs`.
    // zenoh-pico's C calls `memcpy`; Rust's copies lower to the ARM EABI
    // `__aeabi_memcpy*` helpers. `--wrap` redirects each UNDEFINED reference
    // to `__wrap_<sym>`, whose `__real_<sym>` is the original.
    for sym in [
        "memcpy",
        "__aeabi_memcpy",
        "__aeabi_memcpy4",
        "__aeabi_memcpy8",
    ] {
        println!("cargo:rustc-link-arg-bins=--wrap={sym}");
    }
}

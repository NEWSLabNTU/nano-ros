//! Issue 2918 -- the static heap's size is `memory.heap_bytes`, read through
//! the knob registry like every other knob.
//!
//! `src/memory.rs` sizes the heap at compile time with
//! `option_env!("NROS_MEMORY_HEAP_BYTES")`. That alone would read only the
//! canonical name, so a retired spelling (`NROS_HEAP_SIZE`) would be dropped
//! without a word. This script reads the knob through `knob_env`, which stops
//! the build on a retired spelling and names its replacement, and hands the
//! value it resolved to the compile.

fn main() {
    const KNOB: &str = "NROS_MEMORY_HEAP_BYTES";
    nros_platform_config::platform_config::watch_knob_env(KNOB);
    if let Some(v) =
        nros_platform_config::platform_config::knob_env(&|k| std::env::var(k).ok(), KNOB)
    {
        let v = v.trim();
        if !v.is_empty() {
            println!("cargo:rustc-env={KNOB}={v}");
        }
    }
}

//! Build script for `nros` — phase-391 W5.
//!
//! Reads `NROS_RUNTIME_*` environment variables (or their `CONFIG_*` Kconfig
//! spellings) and generates `nros_runtime_config.rs`, included by
//! `src/config.rs`.
//!
//! # Why a knob and not a const generic
//!
//! `node_runtime` carries nine `extern "C"` sites and backs
//! `__nros_component_<pkg>_install`, the uniform cross-language
//! component-install seam. A const generic would put a type parameter on a type
//! that C and C++ consume; a baked `pub const` is invisible at the ABI. Its
//! twin `node_metadata` has ZERO `extern "C"` sites and uses const generics
//! freely — the tree already draws this line, W5 just states it.
//!
//! # Why `knob_usize` and not `env::var`
//!
//! Issue 0460: a Zephyr RUST image inherits none of the cmake `set(ENV{...})`
//! knob exports, so an `env::var` read compiles the crate default whatever
//! Kconfig said. `knob_usize` falls back to `$DOTCONFIG`.
use std::{env, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Component pool slots. `register_node::<C>()` is a runtime call so the
    // COUNT is not known at compile time, but the BOUND is — which is all a
    // static pool needs. Mirrors `NROS_EXECUTOR_MAX_NODES` (default 4) one
    // layer down; 4 leaves room for the multi-component entries codegen emits
    // without charging a single-node image for slots it never fills.
    let max_components = env_usize("NROS_RUNTIME_MAX_COMPONENTS", 4);

    // Per-slot byte budget for the type-erased `TypedSlot<C>`. A BYTE budget
    // rather than a type, because the pool is heterogeneous (`TypedSlot<C>` is
    // generic over `C`) and the FFI seam cannot name a generic. A component
    // whose slot does not fit is a registration error, not a compile error.
    let component_slot_bytes = env_usize("NROS_RUNTIME_COMPONENT_SLOT_BYTES", 512);

    // phase-391 W5.3b — instances of ONE component class the macro-emitted
    // per-class store can hold. Multi-instance is real: the launch path bakes
    // one identity per plan node and can name the same class twice. 2 covers
    // the pair case without charging every class for more; install past the
    // cap is a Full-style registration error, not silent reuse.
    let max_class_instances = env_usize("NROS_RUNTIME_MAX_CLASS_INSTANCES", 2);

    let contents = format!(
        "/// Component pool slots (set via `NROS_RUNTIME_MAX_COMPONENTS`, default 4).\n\
         ///\n\
         /// phase-391 W5. The bound the static component pool is sized to; a\n\
         /// `register_node` past it fails rather than growing a `Vec`.\n\
         pub const MAX_COMPONENTS: usize = {max_components};\n\
         \n\
         /// Per-slot storage for a type-erased `TypedSlot<C>`, in bytes\n\
         /// (set via `NROS_RUNTIME_COMPONENT_SLOT_BYTES`, default 512).\n\
         pub const COMPONENT_SLOT_BYTES: usize = {component_slot_bytes};\n\
         \n\
         /// Instances of one component class the per-class store holds\n\
         /// (set via `NROS_RUNTIME_MAX_CLASS_INSTANCES`, default 2).\n\
         pub const MAX_CLASS_INSTANCES: usize = {max_class_instances};\n"
    );
    std::fs::write(Path::new(&out_dir).join("nros_runtime_config.rs"), contents).unwrap();
}

fn env_usize(name: &str, default: usize) -> usize {
    nros_zephyr_build::knob_usize(name, &format!("CONFIG_{name}"), default)
}

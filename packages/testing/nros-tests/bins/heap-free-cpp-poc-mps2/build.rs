//! Compile `src/poc.cpp` into this image -- issue 2131.
//!
//! The bare-metal C++ build (`examples/mps2-an385-baremetal/cpp/talker`'s
//! `build.rs` explains each flag) minus what this image does not have: no
//! messages, so no `nros generate cpp`, and no RMW, so no backend crate
//! announces itself for `nros_cc_flags::backends::generate` to register. The
//! registration TU is written from an EMPTY list instead, through the same
//! `backends::source`, so the symbol the C++ init calls has the one spelling.

use std::path::PathBuf;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "heap-free-cpp-poc-mps2: {name} is not set -- nros-c / nros-cpp must carry \
             their `links` keys (issue 1512)"
        )
    })
}

fn main() {
    nros_build_paths::link_script!("memory.x");
    println!("cargo:rerun-if-changed=src/poc.cpp");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env("OUT_DIR"));
    let cpp_include = env("DEP_NROS_CPP_INCLUDE");
    let cpp_config_include = env("DEP_NROS_CPP_CONFIG_INCLUDE");
    let c_include = env("DEP_NROS_C_INCLUDE");
    let c_config_include = env("DEP_NROS_C_CONFIG_INCLUDE");
    let platform_include = env("DEP_NROS_C_PLATFORM_INCLUDE");
    println!("cargo:rerun-if-changed={c_config_include}/rclc/detail/nros_config_generated.h");
    println!(
        "cargo:rerun-if-changed={cpp_config_include}/rclcpp/detail/nros_cpp_config_generated.h"
    );

    // No backend: `nros_app_register_backends()` registers nothing, so
    // `rclcpp::init_in()` is refused at run time. The LINK is what is under
    // test (see `src/main.rs`).
    let backends = out_dir.join("nros_app_register_backends.cpp");
    std::fs::write(&backends, nros_cc_flags::backends::source(&[]))
        .expect("write nros_app_register_backends.cpp");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .cpp_link_stdlib(None)
        .include(&cpp_config_include)
        .include(&c_config_include)
        .include(&cpp_include)
        .include(&c_include)
        .include(&platform_include)
        .std("c++17")
        .flag("-ffreestanding")
        .flag("-fno-exceptions")
        .flag("-fno-rtti")
        .flag("-fno-threadsafe-statics")
        .flag("-fno-use-cxa-atexit")
        .file("src/poc.cpp")
        .file(&backends);
    nros_cc_flags::gcc_safe_frame_pointer(&mut build);
    nros_cc_flags::app_main::generate(&mut build, true);
    nros_cc_flags::header_deps::track_header_deps(&mut build);
    build.compile("heap_free_cpp_poc_app");
    nros_cc_flags::header_deps::emit_header_deps(&out_dir);
}

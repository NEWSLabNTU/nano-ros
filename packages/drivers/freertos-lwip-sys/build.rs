//! Build script for freertos-lwip-sys.
//!
//! Runs bindgen to generate Rust FFI bindings from lwIP + FreeRTOS headers.
//! Requires environment variables:
//!   FREERTOS_DIR       — FreeRTOS kernel source (e.g., third-party/freertos/kernel)
//!   FREERTOS_PORT      — Portable layer (e.g., GCC/ARM_CM3)
//!   FREERTOS_CONFIG_DIR — Directory with FreeRTOSConfig.h and lwipopts.h
//!   LWIP_DIR           — lwIP source (e.g., third-party/freertos/lwip)

use std::env;
use std::path::PathBuf;

fn main() {
    let freertos_dir = env::var("FREERTOS_DIR").expect(
        "FREERTOS_DIR not set. Run: just freertos setup && source .envrc",
    );
    let lwip_dir =
        env::var("LWIP_DIR").expect("LWIP_DIR not set");
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR").expect(
        "FREERTOS_CONFIG_DIR not set",
    );

    let freertos = PathBuf::from(&freertos_dir);
    let lwip = PathBuf::from(&lwip_dir);
    let config = PathBuf::from(&freertos_config_dir);

    // Build bindgen with cross-compilation flags for ARM Cortex-M
    let target = env::var("TARGET").unwrap_or_default();
    let is_arm = target.contains("thumbv7") || target.contains("arm");

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .use_core()
        .ctypes_prefix("core::ffi")
        // Include paths
        .clang_arg(format!("-I{}", freertos.join("include").display()))
        .clang_arg(format!(
            "-I{}",
            freertos.join("portable").join(&freertos_port).display()
        ))
        .clang_arg(format!("-I{}", config.display()))
        .clang_arg(format!("-I{}", lwip.join("src/include").display()))
        // Types we want
        .allowlist_type("timeval")
        .allowlist_type("addrinfo")
        .allowlist_type("sockaddr")
        .allowlist_type("sockaddr_in")
        .allowlist_type("sockaddr_in6")
        .allowlist_type("sockaddr_storage")
        .allowlist_type("linger")
        .allowlist_type("fd_set")
        .allowlist_type("_z_sys_net_socket_t")
        .allowlist_type("_z_sys_net_endpoint_t")
        // Socket functions (lwIP exports lwip_* names)
        .allowlist_function("lwip_socket")
        .allowlist_function("lwip_connect")
        .allowlist_function("lwip_bind")
        .allowlist_function("lwip_listen")
        .allowlist_function("lwip_accept")
        .allowlist_function("lwip_recv")
        .allowlist_function("lwip_recvfrom")
        .allowlist_function("lwip_send")
        .allowlist_function("lwip_sendto")
        .allowlist_function("lwip_setsockopt")
        .allowlist_function("lwip_getsockopt")
        .allowlist_function("lwip_close")
        .allowlist_function("lwip_shutdown")
        .allowlist_function("lwip_fcntl")
        .allowlist_function("lwip_select")
        .allowlist_function("lwip_getaddrinfo")
        .allowlist_function("lwip_freeaddrinfo")
        .allowlist_function("lwip_socket_thread_init")
        .allowlist_function("lwip_socket_thread_cleanup")
        // Socket constants
        .allowlist_var("AF_.*")
        .allowlist_var("PF_.*")
        .allowlist_var("SOCK_.*")
        .allowlist_var("IPPROTO_.*")
        .allowlist_var("SOL_.*")
        .allowlist_var("SO_.*")
        .allowlist_var("TCP_.*")
        .allowlist_var("F_GETFL")
        .allowlist_var("F_SETFL")
        .allowlist_var("O_NONBLOCK")
        .allowlist_var("SHUT_.*")
        .allowlist_var("MSG_.*")
        // Don't generate layout tests (can't run on embedded)
        .layout_tests(false)
        // Derive common traits
        .derive_debug(false)
        .derive_default(true)
        .derive_copy(true);

    if is_arm {
        let gcc_include = find_gcc_include().unwrap_or_default();
        // GCC's include-fixed is a sibling of include: .../10.3.1/include-fixed
        let gcc_base = PathBuf::from(&gcc_include).parent().unwrap_or(std::path::Path::new("")).to_path_buf();
        let gcc_include_fixed = gcc_base.join("include-fixed").to_string_lossy().to_string();
        let newlib_include = find_newlib_include().unwrap_or_default();

        builder = builder
            .clang_arg("--target=arm-none-eabi")
            .clang_arg("-mthumb")
            .clang_arg("-march=armv7-m")
            // Use the ARM GCC's built-in + newlib headers (not host system headers)
            .clang_arg("-nostdinc")
            .clang_arg(format!("-isystem{gcc_include}"))
            .clang_arg(format!("-isystem{gcc_include_fixed}"))
            .clang_arg(format!("-isystem{newlib_include}"));
    }

    let bindings = builder.generate().expect("Failed to generate bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Failed to write bindings");

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=FREERTOS_DIR");
    println!("cargo:rerun-if-env-changed=LWIP_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=FREERTOS_CONFIG_DIR");
}

/// Find the ARM GCC built-in include directory (for stdint.h, stddef.h, etc.)
fn find_gcc_include() -> Option<String> {
    let output = std::process::Command::new("arm-none-eabi-gcc")
        .args(["-print-file-name=include"])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || path == "include" {
        None
    } else {
        Some(path)
    }
}

/// Find newlib's include directory (for time.h, sys/types.h, etc.)
fn find_newlib_include() -> Option<String> {
    // newlib headers are typically at /usr/arm-none-eabi/include
    let candidates = [
        "/usr/arm-none-eabi/include",
        "/usr/lib/arm-none-eabi/include",
    ];
    for c in &candidates {
        if std::path::Path::new(c).join("time.h").exists() {
            return Some(c.to_string());
        }
    }
    None
}

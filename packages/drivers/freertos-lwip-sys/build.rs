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
    let freertos_dir = match env::var("FREERTOS_DIR") {
        Ok(d) => d,
        Err(_) => {
            // Env vars not set — emit placeholder types so the crate compiles
            // in workspace checks without FreeRTOS installed.
            emit_placeholder_bindings();
            return;
        }
    };
    let lwip_dir = match env::var("LWIP_DIR") {
        Ok(d) => d,
        Err(_) => {
            emit_placeholder_bindings();
            return;
        }
    };
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let freertos_config_dir = match env::var("FREERTOS_CONFIG_DIR") {
        Ok(d) => d,
        Err(_) => {
            emit_placeholder_bindings();
            return;
        }
    };

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

/// Emit minimal placeholder bindings when FreeRTOS env vars aren't set.
/// Allows the crate to compile in workspace checks without FreeRTOS installed.
fn emit_placeholder_bindings() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(
        out_dir.join("bindings.rs"),
        r#"
// Placeholder — FreeRTOS/lwIP headers not available.
// Set FREERTOS_DIR, LWIP_DIR, FREERTOS_CONFIG_DIR for real bindings.
pub type time_t = i64;
pub type suseconds_t = i64;
pub type socklen_t = u32;

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct timeval { pub tv_sec: time_t, pub tv_usec: suseconds_t }

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct addrinfo {
    pub ai_flags: core::ffi::c_int,
    pub ai_family: core::ffi::c_int,
    pub ai_socktype: core::ffi::c_int,
    pub ai_protocol: core::ffi::c_int,
    pub ai_addrlen: socklen_t,
    pub ai_addr: *mut sockaddr,
    pub ai_canonname: *mut core::ffi::c_char,
    pub ai_next: *mut addrinfo,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct sockaddr { pub sa_len: u8, pub sa_family: u8, pub sa_data: [u8; 14] }

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct linger { pub l_onoff: core::ffi::c_int, pub l_linger: core::ffi::c_int }

pub const PF_UNSPEC: u32 = 0;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;
pub const SOL_SOCKET: u32 = 0xFFF;
pub const SO_KEEPALIVE: u32 = 0x0008;
pub const SO_LINGER: u32 = 0x0080;
pub const SO_RCVTIMEO: u32 = 0x1006;
pub const SO_SNDTIMEO: u32 = 0x1005;
pub const SO_REUSEADDR: u32 = 0x0004;
pub const TCP_NODELAY: u32 = 0x01;
pub const F_GETFL: u32 = 3;
pub const F_SETFL: u32 = 4;
pub const O_NONBLOCK: u32 = 1;
pub const SHUT_RDWR: u32 = 2;

unsafe extern "C" {
    pub fn lwip_socket_thread_init();
    pub fn lwip_socket_thread_cleanup();
    pub fn lwip_socket(domain: core::ffi::c_int, ty: core::ffi::c_int, proto: core::ffi::c_int) -> core::ffi::c_int;
    pub fn lwip_connect(s: core::ffi::c_int, name: *const sockaddr, namelen: socklen_t) -> core::ffi::c_int;
    pub fn lwip_bind(s: core::ffi::c_int, name: *const sockaddr, namelen: socklen_t) -> core::ffi::c_int;
    pub fn lwip_listen(s: core::ffi::c_int, backlog: core::ffi::c_int) -> core::ffi::c_int;
    pub fn lwip_accept(s: core::ffi::c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> core::ffi::c_int;
    pub fn lwip_recv(s: core::ffi::c_int, mem: *mut core::ffi::c_void, len: usize, flags: core::ffi::c_int) -> isize;
    pub fn lwip_recvfrom(s: core::ffi::c_int, mem: *mut core::ffi::c_void, len: usize, flags: core::ffi::c_int, from: *mut sockaddr, fromlen: *mut socklen_t) -> isize;
    pub fn lwip_send(s: core::ffi::c_int, data: *const core::ffi::c_void, size: usize, flags: core::ffi::c_int) -> isize;
    pub fn lwip_sendto(s: core::ffi::c_int, data: *const core::ffi::c_void, size: usize, flags: core::ffi::c_int, to: *const sockaddr, tolen: socklen_t) -> isize;
    pub fn lwip_setsockopt(s: core::ffi::c_int, level: core::ffi::c_int, optname: core::ffi::c_int, optval: *const core::ffi::c_void, optlen: socklen_t) -> core::ffi::c_int;
    pub fn lwip_close(s: core::ffi::c_int) -> core::ffi::c_int;
    pub fn lwip_shutdown(s: core::ffi::c_int, how: core::ffi::c_int) -> core::ffi::c_int;
    pub fn lwip_fcntl(s: core::ffi::c_int, cmd: core::ffi::c_int, val: core::ffi::c_int) -> core::ffi::c_int;
    pub fn lwip_select(maxfdp1: core::ffi::c_int, readset: *mut core::ffi::c_void, writeset: *mut core::ffi::c_void, exceptset: *mut core::ffi::c_void, timeout: *mut timeval) -> core::ffi::c_int;
    pub fn lwip_getaddrinfo(nodename: *const core::ffi::c_char, servname: *const core::ffi::c_char, hints: *const addrinfo, res: *mut *mut addrinfo) -> core::ffi::c_int;
    pub fn lwip_freeaddrinfo(ai: *mut addrinfo);
}
"#,
    )
    .unwrap();

    println!("cargo:rerun-if-env-changed=FREERTOS_DIR");
    println!("cargo:rerun-if-env-changed=LWIP_DIR");
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

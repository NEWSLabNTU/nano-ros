//! Build script for threadx-netx-sys.
//!
//! Runs bindgen to generate Rust FFI bindings from NetX Duo BSD headers.
//! Requires environment variables:
//!   THREADX_DIR        — ThreadX kernel source
//!   THREADX_CONFIG_DIR — Directory with tx_user.h, nx_user.h
//!   NETX_DIR           — NetX Duo source
//!   NETX_CONFIG_DIR    — (optional, defaults to THREADX_CONFIG_DIR)

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=THREADX_CONFIG_DIR");
    println!("cargo:rerun-if-env-changed=NETX_DIR");
    println!("cargo:rerun-if-env-changed=NETX_CONFIG_DIR");

    let threadx_dir = match env::var("THREADX_DIR") {
        Ok(d) => d,
        Err(_) => {
            emit_placeholder_bindings();
            return;
        }
    };
    let netx_dir = match env::var("NETX_DIR") {
        Ok(d) => d,
        Err(_) => {
            emit_placeholder_bindings();
            return;
        }
    };
    let threadx_config_dir = match env::var("THREADX_CONFIG_DIR") {
        Ok(d) => d,
        Err(_) => {
            emit_placeholder_bindings();
            return;
        }
    };
    let netx_config_dir =
        env::var("NETX_CONFIG_DIR").unwrap_or_else(|_| threadx_config_dir.clone());

    let threadx = PathBuf::from(&threadx_dir);
    let netx = PathBuf::from(&netx_dir);

    // Detect port directory based on target
    let target = env::var("TARGET").unwrap_or_default();
    let tx_port = if target.contains("linux") || target.contains("x86_64") {
        "linux/gnu"
    } else if target.contains("riscv") {
        "risc-v64/gnu"
    } else {
        "linux/gnu" // fallback
    };

    let nx_port = if target.contains("linux") || target.contains("x86_64") {
        "linux/gnu"
    } else if target.contains("riscv") {
        "risc-v64/gnu"
    } else {
        "linux/gnu"
    };

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .use_core()
        .ctypes_prefix("core::ffi")
        // Include paths (order matches threadx-support.cmake)
        .clang_arg(format!("-I{}", threadx_config_dir))
        .clang_arg(format!("-I{}", netx_config_dir))
        .clang_arg(format!("-I{}", threadx.join("common/inc").display()))
        .clang_arg(format!(
            "-I{}",
            threadx.join("ports").join(tx_port).join("inc").display()
        ))
        .clang_arg(format!("-I{}", netx.join("common/inc").display()))
        .clang_arg(format!(
            "-I{}",
            netx.join("ports").join(nx_port).join("inc").display()
        ))
        .clang_arg(format!("-I{}", netx.join("addons/BSD").display()))
        // Required defines
        .clang_arg("-DTX_INCLUDE_USER_DEFINE_FILE")
        .clang_arg("-DNX_INCLUDE_USER_DEFINE_FILE")
        // Types
        .allowlist_type("nx_bsd_sockaddr")
        .allowlist_type("nx_bsd_sockaddr_in")
        .allowlist_type("nx_bsd_fd_set")
        .allowlist_type("nx_bsd_timeval")
        // Socket functions (nx_bsd_* prefix)
        .allowlist_function("nx_bsd_socket")
        .allowlist_function("nx_bsd_connect")
        .allowlist_function("nx_bsd_bind")
        .allowlist_function("nx_bsd_listen")
        .allowlist_function("nx_bsd_accept")
        .allowlist_function("nx_bsd_recv")
        .allowlist_function("nx_bsd_recvfrom")
        .allowlist_function("nx_bsd_send")
        .allowlist_function("nx_bsd_sendto")
        .allowlist_function("nx_bsd_setsockopt")
        .allowlist_function("nx_bsd_getsockopt")
        .allowlist_function("nx_bsd_soc_close")
        .allowlist_function("nx_bsd_select")
        // htonl/htons/ntohl/ntohs are C macros in NetX — provide as Rust functions
        // Constants
        .allowlist_var("AF_INET")
        .allowlist_var("AF_INET6")
        .allowlist_var("PF_INET")
        .allowlist_var("SOCK_STREAM")
        .allowlist_var("SOCK_DGRAM")
        .allowlist_var("IPPROTO_TCP")
        .allowlist_var("IPPROTO_UDP")
        .allowlist_var("SOL_SOCKET")
        .allowlist_var("SO_RCVTIMEO")
        .allowlist_var("SO_SNDTIMEO")
        .allowlist_var("SO_KEEPALIVE")
        .allowlist_var("SO_REUSEADDR")
        .allowlist_var("SO_LINGER")
        .allowlist_var("TCP_NODELAY")
        .allowlist_var("NX_BSD_MAX_SOCKETS")
        // No layout tests (can't run on embedded)
        .layout_tests(false)
        .derive_debug(false)
        .derive_default(true)
        .derive_copy(true);

    // For RISC-V cross-compilation
    if target.contains("riscv") {
        builder = builder
            .clang_arg("--target=riscv64-unknown-elf")
            .clang_arg("-march=rv64imc");
    }

    let bindings = builder.generate().expect("Failed to generate bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Failed to write bindings");
}

/// Emit placeholder bindings when env vars aren't set (workspace check).
fn emit_placeholder_bindings() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(
        out_dir.join("bindings.rs"),
        r#"
// Placeholder — ThreadX/NetX headers not available.
// Set THREADX_DIR, NETX_DIR, THREADX_CONFIG_DIR for real bindings.

pub type INT = core::ffi::c_int;
pub type UINT = core::ffi::c_uint;
pub type ULONG = core::ffi::c_ulong;
pub type CHAR = core::ffi::c_char;

pub const AF_INET: u32 = 2;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;
pub const SOL_SOCKET: u32 = 0xFFFF;
pub const SO_RCVTIMEO: u32 = 0x1006;
pub const SO_REUSEADDR: u32 = 0x0004;

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct nx_bsd_sockaddr {
    pub sa_family: u16,
    pub sa_data: [core::ffi::c_char; 14],
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct nx_bsd_in_addr {
    pub s_addr: u32,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct nx_bsd_sockaddr_in {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: nx_bsd_in_addr,
    pub sin_zero: [core::ffi::c_char; 8],
}

unsafe extern "C" {
    pub fn nx_bsd_socket(protofamily: INT, type_: INT, protocol: INT) -> INT;
    pub fn nx_bsd_connect(sockID: INT, remoteAddress: *mut nx_bsd_sockaddr, addressLength: INT) -> INT;
    pub fn nx_bsd_bind(sockID: INT, localAddress: *mut nx_bsd_sockaddr, addressLength: INT) -> INT;
    pub fn nx_bsd_listen(sockID: INT, backlog: INT) -> INT;
    pub fn nx_bsd_accept(sockID: INT, ClientAddress: *mut nx_bsd_sockaddr, addressLength: *mut INT) -> INT;
    pub fn nx_bsd_recv(sockID: INT, rcvBuffer: *mut CHAR, bufferLength: INT, flags: INT) -> INT;
    pub fn nx_bsd_recvfrom(sockID: INT, rcvBuffer: *mut CHAR, bufferLength: INT, flags: INT, fromAddr: *mut nx_bsd_sockaddr, fromAddrLen: *mut INT) -> INT;
    pub fn nx_bsd_send(sockID: INT, msg: *const CHAR, msgLength: INT, flags: INT) -> INT;
    pub fn nx_bsd_sendto(sockID: INT, msg: *const CHAR, msgLength: INT, flags: INT, destAddr: *mut nx_bsd_sockaddr, destAddrLen: INT) -> INT;
    pub fn nx_bsd_setsockopt(sockID: INT, option_level: INT, option_name: INT, option_value: *const core::ffi::c_void, option_length: INT) -> INT;
    pub fn nx_bsd_soc_close(sockID: INT) -> INT;
    // htonl/htons/ntohl/ntohs are Rust functions in lib.rs (C macros can't be bindgen'd)
}
"#,
    )
    .unwrap();
}

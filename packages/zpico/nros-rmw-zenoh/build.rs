fn main() {
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SERVICE_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=NROS_SERVICE_TIMEOUT_MS");
    println!("cargo:rerun-if-env-changed=NROS_KEYEXPR_STRING_SIZE");

    let sub_size: usize = env_usize("ZPICO_SUBSCRIBER_BUFFER_SIZE", 1024);
    let svc_size: usize = env_usize("ZPICO_SERVICE_BUFFER_SIZE", 1024);
    let service_timeout_ms: usize = env_usize("NROS_SERVICE_TIMEOUT_MS", 10_000);
    let keyexpr_string_size: usize = env_usize("NROS_KEYEXPR_STRING_SIZE", 256);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("buffer_config.rs");
    std::fs::write(
        &path,
        format!(
            "/// Subscriber buffer size (set via ZPICO_SUBSCRIBER_BUFFER_SIZE, default 1024).\n\
             pub const SUBSCRIBER_BUFFER_SIZE: usize = {sub_size};\n\
             /// Service request buffer size (set via ZPICO_SERVICE_BUFFER_SIZE, default 1024).\n\
             pub const SERVICE_BUFFER_SIZE: usize = {svc_size};\n\
             /// Default service client RPC timeout in milliseconds\n\
             /// (set via NROS_SERVICE_TIMEOUT_MS, default 10000).\n\
             pub const SERVICE_DEFAULT_TIMEOUT_MS: u32 = {service_timeout_ms};\n\
             /// Maximum key expression string size for topic/service names\n\
             /// (set via NROS_KEYEXPR_STRING_SIZE, default 256).\n\
             pub const KEYEXPR_STRING_SIZE: usize = {keyexpr_string_size};\n\
             /// Key expression buffer size (KEYEXPR_STRING_SIZE + 1 for null terminator).\n\
             pub const KEYEXPR_BUFFER_SIZE: usize = {keyexpr_buf_size};\n",
            keyexpr_buf_size = keyexpr_string_size + 1,
        ),
    )
    .unwrap();
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

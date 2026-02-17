fn main() {
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SERVICE_BUFFER_SIZE");

    let sub_size: usize = std::env::var("ZPICO_SUBSCRIBER_BUFFER_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);

    let svc_size: usize = std::env::var("ZPICO_SERVICE_BUFFER_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("buffer_config.rs");
    std::fs::write(
        &path,
        format!(
            "/// Subscriber buffer size (set via ZPICO_SUBSCRIBER_BUFFER_SIZE env var, default 1024).\n\
             pub const SUBSCRIBER_BUFFER_SIZE: usize = {sub_size};\n\
             /// Service request buffer size (set via ZPICO_SERVICE_BUFFER_SIZE env var, default 1024).\n\
             pub const SERVICE_BUFFER_SIZE: usize = {svc_size};\n"
        ),
    )
    .unwrap();
}

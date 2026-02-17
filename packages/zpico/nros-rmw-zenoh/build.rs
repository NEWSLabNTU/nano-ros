fn main() {
    println!("cargo:rerun-if-env-changed=NROS_SUBSCRIBER_BUFFER_SIZE");

    let size: usize = std::env::var("NROS_SUBSCRIBER_BUFFER_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("buffer_config.rs");
    std::fs::write(
        &path,
        format!(
            "/// Subscriber buffer size (set via NROS_SUBSCRIBER_BUFFER_SIZE env var, default 1024).\n\
             pub const SUBSCRIBER_BUFFER_SIZE: usize = {size};\n"
        ),
    )
    .unwrap();
}

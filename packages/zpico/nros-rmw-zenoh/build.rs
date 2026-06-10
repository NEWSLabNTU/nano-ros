fn main() {
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SERVICE_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=NROS_SERVICE_TIMEOUT_MS");
    println!("cargo:rerun-if-env-changed=NROS_KEYEXPR_STRING_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_RING_DEPTH");
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_LARGE_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_SIZE_THRESHOLD");
    println!("cargo:rerun-if-env-changed=ZPICO_MAX_LARGE_SUBSCRIBERS");

    // Phase 214.C.3 — default coordinated with
    // `packages/core/nros-node/build.rs::NROS_SUBSCRIPTION_BUFFER_SIZE`
    // (also 1024). If you change one, change the other — they share the
    // wire-format expectation. Both can be overridden independently via
    // their respective env vars.
    let sub_size: usize = env_usize("ZPICO_SUBSCRIBER_BUFFER_SIZE", 1024);
    let svc_size: usize = env_usize("ZPICO_SERVICE_BUFFER_SIZE", 1024);
    // Phase 160.C.2 — bumped 10_000 → 30_000. The original 10 s default
    // was too short for slow zenoh-pico flushes on Zephyr/NSOS where
    // each publish/query can take ~2.5 s under Z_FEATURE_INTEREST=1. An
    // action `get_result` query sent while the server is still running a
    // feedback loop (11 publishes × ~2.5 s each = ~28 s before
    // `complete_goal` fires) expires the internal query timer well
    // before the server reaches its `try_handle_get_result` handler.
    // Bumping to 30 s covers the common slow-Zephyr action window; fast
    // services on POSIX still return in milliseconds so the wider cap
    // only matters when something is genuinely slow.
    let service_timeout_ms: usize = env_usize("NROS_SERVICE_TIMEOUT_MS", 30_000);
    let keyexpr_string_size: usize = env_usize("NROS_KEYEXPR_STRING_SIZE", 256);
    // Phase 124.D.3.c — SPSC ring depth per subscriber. Default 4
    // keeps the static-RAM bump small (4 × SUBSCRIBER_BUFFER_SIZE
    // per subscriber); raise for burst-heavy topics. Must be ≥ 1.
    let ring_depth: usize = env_usize("ZPICO_SUBSCRIBER_RING_DEPTH", 4).max(1);
    // Phase 231 (RFC-0038) — size-class receive buffers. `SUBSCRIBER_BUFFER_SIZE`
    // above is the `small` class slot size; the `large` class is for big
    // messages (images, point clouds). A subscription routes to `large` when its
    // `rx_buffer_hint` exceeds the threshold. `large` is capped at a small count
    // so the big slots don't multiply across every subscriber.
    let large_size: usize = env_usize("ZPICO_SUBSCRIBER_LARGE_SIZE", 16384);
    let size_threshold: usize = env_usize("ZPICO_SUBSCRIBER_SIZE_THRESHOLD", 2048);
    let max_large: usize = env_usize("ZPICO_MAX_LARGE_SUBSCRIBERS", 2).max(1);

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
             /// (set via NROS_SERVICE_TIMEOUT_MS, default 30000).\n\
             pub const SERVICE_DEFAULT_TIMEOUT_MS: u32 = {service_timeout_ms};\n\
             /// Maximum key expression string size for topic/service names\n\
             /// (set via NROS_KEYEXPR_STRING_SIZE, default 256).\n\
             pub const KEYEXPR_STRING_SIZE: usize = {keyexpr_string_size};\n\
             /// Key expression buffer size (KEYEXPR_STRING_SIZE + 1 for null terminator).\n\
             pub const KEYEXPR_BUFFER_SIZE: usize = {keyexpr_buf_size};\n\
             /// Phase 124.D.3.c — per-subscriber SPSC ring depth\n\
             /// (set via ZPICO_SUBSCRIBER_RING_DEPTH, default 4).\n\
             pub const SUBSCRIBER_RING_DEPTH: usize = {ring_depth};\n\
             /// Phase 231 (RFC-0038) — `large` size-class slot size\n\
             /// (set via ZPICO_SUBSCRIBER_LARGE_SIZE, default 16384).\n\
             pub const SUBSCRIBER_LARGE_SIZE: usize = {large_size};\n\
             /// Phase 231 — rx_buffer_hint above this routes to the `large` class\n\
             /// (set via ZPICO_SUBSCRIBER_SIZE_THRESHOLD, default 2048).\n\
             pub const SUBSCRIBER_SIZE_THRESHOLD: usize = {size_threshold};\n\
             /// Phase 231 — max concurrent `large`-class subscribers\n\
             /// (set via ZPICO_MAX_LARGE_SUBSCRIBERS, default 2).\n\
             pub const MAX_LARGE_SUBSCRIBERS: usize = {max_large};\n",
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

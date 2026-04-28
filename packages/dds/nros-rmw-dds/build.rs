// Phase 92.5 — pass the local interface IPv4 (in dotted-quad form)
// from the build environment into transport_nros.rs as a static
// constant. RTPS SPDP advertises this address to peers as the
// participant's unicast locator; without it, peers send SEDP /
// user-data to localhost and never reach us across guest VMs.
//
// Resolution order:
//   1. `NROS_LOCAL_IPV4` env var (explicit override).
//   2. `CONFIG_NET_CONFIG_MY_IPV4_ADDR` parsed from Zephyr's `.config`
//      (path supplied via the `DOTCONFIG` env var that
//      `rust_cargo_application()` exports). This is the path used
//      by Zephyr-Rust embedded targets — the upstream cmake helper
//      doesn't propagate arbitrary env vars to cargo, but
//      `DOTCONFIG` is in its allow-list so we read .config directly.
//   3. `127.0.0.1` (keeps native_sim NSOS host-loopback path
//      working unchanged).

fn main() {
    println!("cargo:rerun-if-env-changed=NROS_LOCAL_IPV4");
    println!("cargo:rerun-if-env-changed=DOTCONFIG");

    let ip = std::env::var("NROS_LOCAL_IPV4")
        .ok()
        .or_else(read_kconfig_ipv4)
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let octets: Vec<u8> = ip
        .split('.')
        .map(|s| {
            s.parse::<u8>()
                .expect("NROS_LOCAL_IPV4 must be IPv4 dotted-quad")
        })
        .collect();
    assert_eq!(
        octets.len(),
        4,
        "NROS_LOCAL_IPV4='{ip}' is not a 4-octet IPv4 address"
    );
    println!(
        "cargo:rustc-env=NROS_LOCAL_IPV4_BYTES={},{},{},{}",
        octets[0], octets[1], octets[2], octets[3]
    );
}

fn read_kconfig_ipv4() -> Option<String> {
    let path = std::env::var("DOTCONFIG").ok()?;
    let body = std::fs::read_to_string(&path).ok()?;
    println!("cargo:rerun-if-changed={path}");
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("CONFIG_NET_CONFIG_MY_IPV4_ADDR=") {
            // Value is a quoted Kconfig string ("192.0.2.1")
            let trimmed = rest.trim().trim_matches('"');
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

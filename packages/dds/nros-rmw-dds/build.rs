// Phase 92.5 — pass the local interface IPv4 (in dotted-quad form)
// from the build environment into transport_nros.rs as a static
// constant. RTPS SPDP advertises this address to peers as the
// participant's unicast locator; without it, peers send SEDP /
// user-data to localhost and never reach us across guest VMs.
//
// Resolution order:
//   1. `CONFIG_NET_CONFIG_MY_IPV4_ADDR` parsed from Zephyr's `.config`
//      when `DOTCONFIG` is present and contains a non-empty value.
//      This is authoritative for embedded Zephyr boards
//      (qemu_cortex_a9 + Xilinx GEM, ESP32 + WiFi, …) that hard-pin
//      a real interface IP via `boards/<board>.conf`.
//   2. `NROS_LOCAL_IPV4` env var (explicit override). Used by
//      `native_sim` examples that bypass the Zephyr IP stack via
//      NSOS — `.config` has no `CONFIG_NET_CONFIG_MY_IPV4_ADDR` set
//      there, but `.cargo/config.toml` provides distinct loopback
//      IPs (e.g. 127.0.0.10 / 127.0.0.20) so two sibling
//      `native_sim` processes derive different RTPS GUID prefixes.
//   3. `127.0.0.1` (final fallback for non-Zephyr POSIX builds).
//
// Order is `.config` first so an a9 / ESP32 build automatically
// inherits its real interface IP without per-example
// `.cargo/config.toml` patching — earlier the order was reversed
// and the `NROS_LOCAL_IPV4 = "127.0.0.20"` line that native_sim
// needs was poisoning the a9 build, producing SPDP locators of
// `127.0.0.20:7410` instead of `192.0.2.2:7410`. Cross-VM SEDP
// then never reached the listener.

fn main() {
    println!("cargo:rerun-if-env-changed=NROS_LOCAL_IPV4");
    println!("cargo:rerun-if-env-changed=DOTCONFIG");

    let ip = read_kconfig_ipv4()
        .or_else(|| std::env::var("NROS_LOCAL_IPV4").ok())
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

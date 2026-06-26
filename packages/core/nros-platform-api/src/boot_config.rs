// ============================================================================
// BakedBootConfig — RFC-0045 "Single embedded bake site"
//
// Lives in nros-platform-api (no deps, no_std) so that nros-platform's
// DeployOverlay can hold a `&'static BakedBootConfig` without creating a
// dependency cycle back through nros-node.
// ============================================================================

/// 0x4E524243 = ASCII "NRBC". A post-link tool scans for this magic to locate
/// the struct in a firmware image.
pub const NROS_BOOT_CONFIG_MAGIC: u32 = 0x4E52_4243;

/// Layout version — lets the resolver reject a mismatched baked struct.
pub const NROS_BOOT_CONFIG_VERSION: u16 = 1;

// `set_flags` bit assignments in `BakedBootConfig`.
/// Bit 0 — `node_name` field is set.
pub const BOOT_SET_NODE_NAME: u16 = 1 << 0;
/// Bit 1 — `locator` field is set.
pub const BOOT_SET_LOCATOR: u16 = 1 << 1;
/// Bit 2 — `domain_id` field is set.
pub const BOOT_SET_DOMAIN: u16 = 1 << 2;
/// Bit 3 — `namespace` field is set.
pub const BOOT_SET_NAMESPACE: u16 = 1 << 3;

/// Build-time-baked boot config, emitted (in W4b) into the `.nros_boot_config`
/// linker section by the entry macro / cmake. Fixed-size + pointer-free so a
/// future post-link tool can patch it in place (RFC-0045). The resolver reads
/// it on embedded via `BootConfig::from_baked` (in `nros-node`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BakedBootConfig {
    /// 0x4E524243 = b"NRBC". A post-link tool scans for this to locate the struct.
    pub magic: u32,
    /// Layout version (start at 1) — lets the tool/reader reject mismatched layouts.
    pub version: u16,
    /// One bit per field that is baked-set (else the reader yields `None` → resolver
    /// default). bit0 = node_name, bit1 = locator, bit2 = domain_id, bit3 = namespace.
    pub set_flags: u16,
    /// ROS 2 domain ID (valid only when `BOOT_SET_DOMAIN` bit is set).
    pub domain_id: u32,
    /// NUL-padded UTF-8; the trailing NUL bytes are not part of the value.
    pub node_name: [u8; 64],
    /// NUL-padded UTF-8 middleware locator; the trailing NUL bytes are not part of
    /// the value.
    pub locator: [u8; 96],
    /// NUL-padded UTF-8 node namespace; the trailing NUL bytes are not part of the
    /// value.
    pub namespace: [u8; 64],
}

/// Copy `s` bytes into a zero-padded `[u8; N]` array at compile time.
///
/// A string longer than `N` bytes is a **compile-time** error (const panic).
/// Never silently truncated.
const fn pack<const N: usize>(s: &str) -> [u8; N] {
    let bytes = s.as_bytes();
    if bytes.len() > N {
        panic!("BakedBootConfig: string field exceeds its fixed-size buffer");
    }
    let mut buf = [0u8; N];
    let mut i = 0;
    while i < bytes.len() {
        buf[i] = bytes[i];
        i += 1;
    }
    buf
}

impl BakedBootConfig {
    /// Pack baked fields at compile time.  `None` → field unset (bit clear,
    /// bytes zeroed).  A string longer than its fixed buffer is a
    /// **compile-time** error (const panic) — never silently truncated.
    ///
    /// `must be const fn` so W4b's entry macro can use it in a `static` initializer.
    pub const fn new(
        node_name: Option<&str>,
        locator: Option<&str>,
        domain_id: Option<u32>,
        namespace: Option<&str>,
    ) -> BakedBootConfig {
        let mut flags: u16 = 0;

        let node_name_bytes: [u8; 64] = match node_name {
            Some(s) => {
                flags |= BOOT_SET_NODE_NAME;
                pack::<64>(s)
            }
            None => [0u8; 64],
        };

        let locator_bytes: [u8; 96] = match locator {
            Some(s) => {
                flags |= BOOT_SET_LOCATOR;
                pack::<96>(s)
            }
            None => [0u8; 96],
        };

        let domain_id_val: u32 = match domain_id {
            Some(d) => {
                flags |= BOOT_SET_DOMAIN;
                d
            }
            None => 0,
        };

        let namespace_bytes: [u8; 64] = match namespace {
            Some(s) => {
                flags |= BOOT_SET_NAMESPACE;
                pack::<64>(s)
            }
            None => [0u8; 64],
        };

        BakedBootConfig {
            magic: NROS_BOOT_CONFIG_MAGIC,
            version: NROS_BOOT_CONFIG_VERSION,
            set_flags: flags,
            domain_id: domain_id_val,
            node_name: node_name_bytes,
            locator: locator_bytes,
            namespace: namespace_bytes,
        }
    }
}

// ============================================================================
// BakedBootConfig unit tests (no_std-compatible, run under std test runner)
//
// These tests verify BakedBootConfig::new / pack directly (struct-field
// inspection only — no BootConfig::from_baked, which lives in nros-node).
// Round-trip tests (new → from_baked) live in nros-node/src/executor/types.rs.
// ============================================================================

#[cfg(test)]
mod baked_boot_config_tests {
    use super::*;

    // ── T-BB8: set_flags bit-pattern is exact ─────────────────────────────────

    /// Verify the set_flags bitmask matches the expected bit positions.
    #[test]
    fn set_flags_bits_correct() {
        let baked = BakedBootConfig::new(
            Some("n"), // bit 0
            None,
            Some(0),  // bit 2
            Some(""), // bit 3
        );
        assert_eq!(
            baked.set_flags,
            BOOT_SET_NODE_NAME | BOOT_SET_DOMAIN | BOOT_SET_NAMESPACE
        );
    }

    // ── T-BB-MAGIC: magic and version are populated ───────────────────────────

    /// `BakedBootConfig::new` must embed the correct magic word and version.
    #[test]
    fn magic_and_version_populated() {
        let baked = BakedBootConfig::new(None, None, None, None);
        assert_eq!(baked.magic, NROS_BOOT_CONFIG_MAGIC);
        assert_eq!(baked.version, NROS_BOOT_CONFIG_VERSION);
    }

    // ── T-BB-PACK: short string is NUL-padded ────────────────────────────────

    /// A node_name shorter than 64 bytes must be stored in the leading bytes,
    /// with the remaining bytes zeroed (NUL-padded).
    #[test]
    fn short_name_is_nul_padded() {
        let name = "robot";
        let baked = BakedBootConfig::new(Some(name), None, None, None);
        assert_eq!(&baked.node_name[..name.len()], name.as_bytes());
        assert!(baked.node_name[name.len()..].iter().all(|&b| b == 0));
        assert_eq!(baked.set_flags & BOOT_SET_NODE_NAME, BOOT_SET_NODE_NAME);
    }

    // ── T-BB-NONE: all-None zeroes bytes and clears flags ────────────────────

    /// When every argument is None the set_flags must be zero, domain_id zero,
    /// and all byte arrays zeroed.
    #[test]
    fn all_none_zeroes_fields() {
        let baked = BakedBootConfig::new(None, None, None, None);
        assert_eq!(baked.set_flags, 0);
        assert_eq!(baked.domain_id, 0);
        assert!(baked.node_name.iter().all(|&b| b == 0));
        assert!(baked.locator.iter().all(|&b| b == 0));
        assert!(baked.namespace.iter().all(|&b| b == 0));
    }

    // ── Compile-failure comment ───────────────────────────────────────────────
    // Uncommenting the line below must FAIL to compile because the string
    // exceeds the 64-byte node_name buffer.  Do NOT uncomment in CI.
    //
    // const _: BakedBootConfig = BakedBootConfig::new(
    //     Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"), // 65 A's (> 64-byte buffer)
    //     None, None, None,
    // );

    // ── T-LAYOUT: layout-drift guard (mirrors C header static_asserts) ────────
    //
    // If `BakedBootConfig` ever changes size or field offsets, this test fails
    // and forces the C header `include/nros/boot_config.h` to be updated in
    // lockstep.  The C header carries matching _Static_assert / static_assert
    // guards on the C/C++ side.

    /// Size and field offsets of `BakedBootConfig` must match the C header's
    /// documented layout (total 236 bytes, no padding).
    #[test]
    fn baked_boot_config_layout() {
        use core::mem::{offset_of, size_of};
        assert_eq!(size_of::<BakedBootConfig>(), 236, "total size must be 236");
        assert_eq!(offset_of!(BakedBootConfig, magic), 0, "magic @ 0");
        assert_eq!(offset_of!(BakedBootConfig, version), 4, "version @ 4");
        assert_eq!(offset_of!(BakedBootConfig, set_flags), 6, "set_flags @ 6");
        assert_eq!(offset_of!(BakedBootConfig, domain_id), 8, "domain_id @ 8");
        assert_eq!(offset_of!(BakedBootConfig, node_name), 12, "node_name @ 12");
        assert_eq!(offset_of!(BakedBootConfig, locator), 76, "locator @ 76");
        assert_eq!(
            offset_of!(BakedBootConfig, namespace),
            172,
            "namespace @ 172"
        );
    }
}

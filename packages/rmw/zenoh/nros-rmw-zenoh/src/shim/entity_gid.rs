//! The zenoh entity GID — issue 1495.
//!
//! # What the 16 bytes are, and why
//!
//! `rmw_zenoh_cpp` does not GENERATE a gid. It DERIVES one: every entity's gid
//! is XXH3-128 of that entity's complete liveliness keyexpression
//! (`Entity::Entity` in `rmw_zenoh_cpp/src/detail/liveliness_utils.cpp`,
//! 0.1.9, the version Humble ships), `low64` little-endian then `high64`
//! little-endian. The same 16 bytes go into every attachment the entity sends
//! and come back out of `rmw_get_gid_for_publisher`, and a peer that parses
//! the entity's token rebuilds the keyexpr and hashes it to the same value —
//! which is how `ros2 topic info --verbose` on a zenoh graph prints a GID for
//! an endpoint it has never received a sample from.
//!
//! So deriving ours the same way is not a choice of hash. It is the one value
//! that means the same thing to both sides: the attachment a stock subscriber
//! reads, our `get_gid_for_publisher`, and the GID a stock graph query reports
//! for our token all become one number.
//!
//! What it replaced was a counter times a constant, XORed with the address of
//! a stack local — stable across nothing, derived from nothing a peer knows us
//! by, and able to collide between two processes.
//!
//! # Stability
//!
//! The keyexpr carries the session's zenoh id, so the gid is exactly as stable
//! as that id: a session that configures `session_zid` gets the same gid for
//! the same entity on every restart; one that lets zenoh-pico pick a random id
//! (the default) gets a fresh gid per run. That is upstream's behaviour too —
//! `rmw_zenoh_cpp`'s zid is per-process — and it is the honest one: a gid that
//! survived a restart while the zid did not would claim a continuity the graph
//! does not have.
//!
//! # The hash
//!
//! XXH3-128 with the default secret and seed 0, ported branch for branch from
//! upstream's `simplified_xxhash3.cpp` (itself a reduction of xxHash's
//! reference implementation, BSD-2-Clause, Copyright (C) 2012-2023 Yann
//! Collet). No dependency: one is not worth a lockfile move in every leaf for
//! ~150 lines, and the test vectors below were produced by compiling
//! upstream's own file, so this is checked against the code the peer runs
//! rather than against a reading of it.

/// One XXH3 128-bit result, in xxHash's own field order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Xxh128 {
    pub low64: u64,
    pub high64: u64,
}

const STRIPE_LEN: usize = 64;
const SECRET_CONSUME_RATE: usize = 8;
const ACC_NB: usize = STRIPE_LEN / 8;
const SECRET_SIZE_MIN: usize = 136;
const MIDSIZE_MAX: usize = 240;
const MIDSIZE_STARTOFFSET: usize = 3;
const MIDSIZE_LASTOFFSET: usize = 17;
const SECRET_LASTACC_START: usize = 7;
const SECRET_MERGEACCS_START: usize = 11;

const PRIME32_1: u64 = 0x9E37_79B1;
const PRIME32_2: u64 = 0x85EB_CA77;
const PRIME32_3: u64 = 0xC2B2_AE3D;
const PRIME64_1: u64 = 0x9E37_79B1_85EB_CA87;
const PRIME64_2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const PRIME64_3: u64 = 0x1656_67B1_9E37_79F9;
const PRIME64_4: u64 = 0x85EB_CA77_C2B2_AE63;
const PRIME64_5: u64 = 0x27D4_EB2F_1656_67C5;
const PRIME_MX1: u64 = 0x1656_6791_9E37_79F9;
const PRIME_MX2: u64 = 0x9FB2_1C65_1E98_DF25;

const SECRET: [u8; 192] = [
    0xb8, 0xfe, 0x6c, 0x39, 0x23, 0xa4, 0x4b, 0xbe, 0x7c, 0x01, 0x81, 0x2c, 0xf7, 0x21, 0xad, 0x1c,
    0xde, 0xd4, 0x6d, 0xe9, 0x83, 0x90, 0x97, 0xdb, 0x72, 0x40, 0xa4, 0xa4, 0xb7, 0xb3, 0x67, 0x1f,
    0xcb, 0x79, 0xe6, 0x4e, 0xcc, 0xc0, 0xe5, 0x78, 0x82, 0x5a, 0xd0, 0x7d, 0xcc, 0xff, 0x72, 0x21,
    0xb8, 0x08, 0x46, 0x74, 0xf7, 0x43, 0x24, 0x8e, 0xe0, 0x35, 0x90, 0xe6, 0x81, 0x3a, 0x26, 0x4c,
    0x3c, 0x28, 0x52, 0xbb, 0x91, 0xc3, 0x00, 0xcb, 0x88, 0xd0, 0x65, 0x8b, 0x1b, 0x53, 0x2e, 0xa3,
    0x71, 0x64, 0x48, 0x97, 0xa2, 0x0d, 0xf9, 0x4e, 0x38, 0x19, 0xef, 0x46, 0xa9, 0xde, 0xac, 0xd8,
    0xa8, 0xfa, 0x76, 0x3f, 0xe3, 0x9c, 0x34, 0x3f, 0xf9, 0xdc, 0xbb, 0xc7, 0xc7, 0x0b, 0x4f, 0x1d,
    0x8a, 0x51, 0xe0, 0x4b, 0xcd, 0xb4, 0x59, 0x31, 0xc8, 0x9f, 0x7e, 0xc9, 0xd9, 0x78, 0x73, 0x64,
    0xea, 0xc5, 0xac, 0x83, 0x34, 0xd3, 0xeb, 0xc3, 0xc5, 0x81, 0xa0, 0xff, 0xfa, 0x13, 0x63, 0xeb,
    0x17, 0x0d, 0xdd, 0x51, 0xb7, 0xf0, 0xda, 0x49, 0xd3, 0x16, 0x55, 0x26, 0x29, 0xd4, 0x68, 0x9e,
    0x2b, 0x16, 0xbe, 0x58, 0x7d, 0x47, 0xa1, 0xfc, 0x8f, 0xf8, 0xb8, 0xd1, 0x7a, 0xd0, 0x31, 0xce,
    0x45, 0xcb, 0x3a, 0x8f, 0x95, 0x16, 0x04, 0x28, 0xaf, 0xd7, 0xfb, 0xca, 0xbb, 0x4b, 0x40, 0x7e,
];

#[inline]
fn r64(b: &[u8], at: usize) -> u64 {
    let mut w = [0u8; 8];
    w.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(w)
}

#[inline]
fn r32(b: &[u8], at: usize) -> u32 {
    let mut w = [0u8; 4];
    w.copy_from_slice(&b[at..at + 4]);
    u32::from_le_bytes(w)
}

#[inline]
fn mult32to64(x: u64, y: u64) -> u64 {
    (x & 0xFFFF_FFFF).wrapping_mul(y & 0xFFFF_FFFF)
}

#[inline]
fn mult64to128(lhs: u64, rhs: u64) -> Xxh128 {
    let p = (lhs as u128).wrapping_mul(rhs as u128);
    Xxh128 {
        low64: p as u64,
        high64: (p >> 64) as u64,
    }
}

#[inline]
fn mul128_fold64(lhs: u64, rhs: u64) -> u64 {
    let p = mult64to128(lhs, rhs);
    p.low64 ^ p.high64
}

#[inline]
fn xorshift64(v: u64, shift: u32) -> u64 {
    v ^ (v >> shift)
}

fn avalanche(mut h: u64) -> u64 {
    h = xorshift64(h, 37);
    h = h.wrapping_mul(PRIME_MX1);
    xorshift64(h, 32)
}

fn xxh64_avalanche(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(PRIME64_2);
    h ^= h >> 29;
    h = h.wrapping_mul(PRIME64_3);
    h ^= h >> 32;
    h
}

fn len_1to3(input: &[u8]) -> Xxh128 {
    let len = input.len();
    let c1 = input[0] as u32;
    let c2 = input[len >> 1] as u32;
    let c3 = input[len - 1] as u32;
    let combinedl = (c1 << 16) | (c2 << 24) | c3 | ((len as u32) << 8);
    let combinedh = combinedl.swap_bytes().rotate_left(13);
    let bitflipl = (r32(&SECRET, 0) ^ r32(&SECRET, 4)) as u64;
    let bitfliph = (r32(&SECRET, 8) ^ r32(&SECRET, 12)) as u64;
    Xxh128 {
        low64: xxh64_avalanche(combinedl as u64 ^ bitflipl),
        high64: xxh64_avalanche(combinedh as u64 ^ bitfliph),
    }
}

fn len_4to8(input: &[u8]) -> Xxh128 {
    let len = input.len();
    let input_lo = r32(input, 0) as u64;
    let input_hi = r32(input, len - 4) as u64;
    let input_64 = input_lo.wrapping_add(input_hi << 32);
    let bitflip = r64(&SECRET, 16) ^ r64(&SECRET, 24);
    let keyed = input_64 ^ bitflip;
    let mut m = mult64to128(keyed, PRIME64_1.wrapping_add((len as u64) << 2));
    m.high64 = m.high64.wrapping_add(m.low64 << 1);
    m.low64 ^= m.high64 >> 3;
    m.low64 = xorshift64(m.low64, 35);
    m.low64 = m.low64.wrapping_mul(PRIME_MX2);
    m.low64 = xorshift64(m.low64, 28);
    m.high64 = avalanche(m.high64);
    m
}

fn len_9to16(input: &[u8]) -> Xxh128 {
    let len = input.len();
    let bitflipl = r64(&SECRET, 32) ^ r64(&SECRET, 40);
    let bitfliph = r64(&SECRET, 48) ^ r64(&SECRET, 56);
    let input_lo = r64(input, 0);
    let mut input_hi = r64(input, len - 8);
    let mut m = mult64to128(input_lo ^ input_hi ^ bitflipl, PRIME64_1);
    m.low64 = m.low64.wrapping_add(((len - 1) as u64) << 54);
    input_hi ^= bitfliph;
    // The 64-bit spelling; upstream's 32-bit branch computes the same value.
    m.high64 = m
        .high64
        .wrapping_add(input_hi)
        .wrapping_add(mult32to64(input_hi & 0xFFFF_FFFF, PRIME32_2 - 1));
    m.low64 ^= m.high64.swap_bytes();
    let mut h = mult64to128(m.low64, PRIME64_2);
    h.high64 = h.high64.wrapping_add(m.high64.wrapping_mul(PRIME64_2));
    Xxh128 {
        low64: avalanche(h.low64),
        high64: avalanche(h.high64),
    }
}

fn len_0to16(input: &[u8]) -> Xxh128 {
    match input.len() {
        9.. => len_9to16(input),
        4.. => len_4to8(input),
        1.. => len_1to3(input),
        _ => Xxh128 {
            low64: xxh64_avalanche(r64(&SECRET, 64) ^ r64(&SECRET, 72)),
            high64: xxh64_avalanche(r64(&SECRET, 80) ^ r64(&SECRET, 88)),
        },
    }
}

fn mix16b(input: &[u8], at: usize, secret_at: usize) -> u64 {
    mul128_fold64(
        r64(input, at) ^ r64(&SECRET, secret_at),
        r64(input, at + 8) ^ r64(&SECRET, secret_at + 8),
    )
}

fn mix32b(mut acc: Xxh128, input: &[u8], i1: usize, i2: usize, secret_at: usize) -> Xxh128 {
    acc.low64 = acc.low64.wrapping_add(mix16b(input, i1, secret_at));
    acc.low64 ^= r64(input, i2).wrapping_add(r64(input, i2 + 8));
    acc.high64 = acc.high64.wrapping_add(mix16b(input, i2, secret_at + 16));
    acc.high64 ^= r64(input, i1).wrapping_add(r64(input, i1 + 8));
    acc
}

fn finish_mid(acc: Xxh128, len: usize) -> Xxh128 {
    let len = len as u64;
    let low = acc.low64.wrapping_add(acc.high64);
    let high = acc
        .low64
        .wrapping_mul(PRIME64_1)
        .wrapping_add(acc.high64.wrapping_mul(PRIME64_4))
        .wrapping_add(len.wrapping_mul(PRIME64_2));
    Xxh128 {
        low64: avalanche(low),
        high64: 0u64.wrapping_sub(avalanche(high)),
    }
}

fn len_17to128(input: &[u8]) -> Xxh128 {
    let len = input.len();
    let mut acc = Xxh128 {
        low64: (len as u64).wrapping_mul(PRIME64_1),
        high64: 0,
    };
    if len > 32 {
        if len > 64 {
            if len > 96 {
                acc = mix32b(acc, input, 48, len - 64, 96);
            }
            acc = mix32b(acc, input, 32, len - 48, 64);
        }
        acc = mix32b(acc, input, 16, len - 32, 32);
    }
    acc = mix32b(acc, input, 0, len - 16, 0);
    finish_mid(acc, len)
}

fn len_129to240(input: &[u8]) -> Xxh128 {
    let len = input.len();
    let mut acc = Xxh128 {
        low64: (len as u64).wrapping_mul(PRIME64_1),
        high64: 0,
    };
    let mut i = 32;
    while i < 160 {
        acc = mix32b(acc, input, i - 32, i - 16, i - 32);
        i += 32;
    }
    acc.low64 = avalanche(acc.low64);
    acc.high64 = avalanche(acc.high64);
    // `i <= len` re-mixes the last 32 bytes when len % 32 == 0 — upstream's
    // stability quirk, kept because the value is the contract.
    let mut i = 160;
    while i <= len {
        acc = mix32b(acc, input, i - 32, i - 16, MIDSIZE_STARTOFFSET + i - 160);
        i += 32;
    }
    acc = mix32b(
        acc,
        input,
        len - 16,
        len - 32,
        SECRET_SIZE_MIN - MIDSIZE_LASTOFFSET - 16,
    );
    finish_mid(acc, len)
}

fn accumulate_512(acc: &mut [u64; ACC_NB], input: &[u8], at: usize, secret_at: usize) {
    for lane in 0..ACC_NB {
        let data_val = r64(input, at + lane * 8);
        let data_key = data_val ^ r64(&SECRET, secret_at + lane * 8);
        acc[lane ^ 1] = acc[lane ^ 1].wrapping_add(data_val);
        acc[lane] = mult32to64(data_key, data_key >> 32).wrapping_add(acc[lane]);
    }
}

fn scramble(acc: &mut [u64; ACC_NB], secret_at: usize) {
    for (lane, a) in acc.iter_mut().enumerate() {
        let key64 = r64(&SECRET, secret_at + lane * 8);
        let mut v = xorshift64(*a, 47);
        v ^= key64;
        *a = v.wrapping_mul(PRIME32_1);
    }
}

fn merge_accs(acc: &[u64; ACC_NB], secret_at: usize, start: u64) -> u64 {
    let mut r = start;
    for i in 0..4 {
        r = r.wrapping_add(mul128_fold64(
            acc[2 * i] ^ r64(&SECRET, secret_at + 16 * i),
            acc[2 * i + 1] ^ r64(&SECRET, secret_at + 16 * i + 8),
        ));
    }
    avalanche(r)
}

fn hash_long(input: &[u8]) -> Xxh128 {
    let len = input.len();
    let mut acc: [u64; ACC_NB] = [
        PRIME32_3, PRIME64_1, PRIME64_2, PRIME64_3, PRIME64_4, PRIME32_2, PRIME64_5, PRIME32_1,
    ];
    let stripes_per_block = (SECRET.len() - STRIPE_LEN) / SECRET_CONSUME_RATE;
    let block_len = STRIPE_LEN * stripes_per_block;
    let nb_blocks = (len - 1) / block_len;
    for n in 0..nb_blocks {
        for s in 0..stripes_per_block {
            accumulate_512(
                &mut acc,
                input,
                n * block_len + s * STRIPE_LEN,
                s * SECRET_CONSUME_RATE,
            );
        }
        scramble(&mut acc, SECRET.len() - STRIPE_LEN);
    }
    let nb_stripes = ((len - 1) - block_len * nb_blocks) / STRIPE_LEN;
    for s in 0..nb_stripes {
        accumulate_512(
            &mut acc,
            input,
            nb_blocks * block_len + s * STRIPE_LEN,
            s * SECRET_CONSUME_RATE,
        );
    }
    accumulate_512(
        &mut acc,
        input,
        len - STRIPE_LEN,
        SECRET.len() - STRIPE_LEN - SECRET_LASTACC_START,
    );
    Xxh128 {
        low64: merge_accs(
            &acc,
            SECRET_MERGEACCS_START,
            (len as u64).wrapping_mul(PRIME64_1),
        ),
        high64: merge_accs(
            &acc,
            SECRET.len() - ACC_NB * 8 - SECRET_MERGEACCS_START,
            !(len as u64).wrapping_mul(PRIME64_2),
        ),
    }
}

/// XXH3-128, default secret, seed 0 — `simplified_XXH3_128bits`.
pub(crate) fn xxh3_128(input: &[u8]) -> Xxh128 {
    match input.len() {
        0..=16 => len_0to16(input),
        17..=128 => len_17to128(input),
        129..=MIDSIZE_MAX => len_129to240(input),
        _ => hash_long(input),
    }
}

/// The 16-byte gid `rmw_zenoh_cpp` derives for the entity whose liveliness
/// keyexpr is `keyexpr` — `low64` then `high64`, each little-endian, which is
/// the `memcpy` order upstream uses on every target it builds for.
pub fn entity_gid(keyexpr: &str) -> [u8; 16] {
    let h = xxh3_128(keyexpr.as_bytes());
    let mut gid = [0u8; 16];
    gid[..8].copy_from_slice(&h.low64.to_le_bytes());
    gid[8..].copy_from_slice(&h.high64.to_le_bytes());
    gid
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Produced by compiling `rmw_zenoh_cpp` 0.1.9's own
    /// `simplified_xxhash3.cpp` and hashing `byte[i] = (i * 31 + 7) & 0xff`
    /// for each length. The lengths straddle every branch boundary (0, 1–3,
    /// 4–8, 9–16, 17–128 in four mix depths, 129–240 including the
    /// `len % 32 == 0` re-mix, and the striped long path across one block
    /// boundary at 1000), because a port that is wrong in one branch is right
    /// in all the others.
    const VECTORS: &[(usize, u64, u64)] = &[
        (0, 0x6001c324468d497f, 0x99aa06d3014798d8),
        (1, 0x4c5cca45d0f4811f, 0x495b62073ef70ca4),
        (2, 0xa7e250c97710ff27, 0x12b2847aa0de5aaa),
        (3, 0x15f7093b173d005c, 0x46f66cb935381565),
        (4, 0xb987ca5d9241572a, 0x7fefeeffb4d0eab3),
        (7, 0x90d8d40e8b5ca9c4, 0x9194efbddb0d752c),
        (8, 0x56bb836ceb6d4baa, 0x803c675a846cc6c2),
        (9, 0x4376673580310154, 0xd46556872d230f22),
        (16, 0xf853dd94614dfa07, 0x650fe308c566747d),
        (17, 0x78c349fe81b2f26c, 0x18217300b5132d5a),
        (31, 0x45e862e1ac921624, 0xa7591e70669b73f8),
        (32, 0x5726e079716c6a62, 0x3220ff5fe507b3c0),
        (33, 0x3b25275300c8b44e, 0x91a4c56ad1b91d88),
        (64, 0x36c5f7e547426bc4, 0xf9bfa77da0891a96),
        (65, 0xd0d1d7884590a330, 0x5642c5d38e6e787d),
        (96, 0x63451be079edd707, 0x59861d1adb3e51a2),
        (97, 0xfa4138b7dc44e45b, 0x0912f66857975b13),
        (128, 0x1e04fad9f0cacb4d, 0xb4f87b99d2db8a51),
        (129, 0xc51bc887976aef63, 0x6881633650cd8924),
        (159, 0x4710980f58432f1d, 0x23d73d2cfa83c4b0),
        (160, 0xf661814e66697391, 0xc000b788df6dbbc4),
        (191, 0x07e4807e2b806ac0, 0xa738c169d152e2bc),
        (192, 0xd8d6fbd475016236, 0xd4479caa4bea0dce),
        (224, 0x202dfe467f90949c, 0x20d5c625750648fc),
        (240, 0x93e173833f75ab66, 0xde57aab31e77a2ff),
        (241, 0x0b3b630948ce4a00, 0x92b991a7192f3f08),
        (255, 0x89932170686cdd9a, 0x3e68b7e415ce7e5c),
        (256, 0xec85b75bafe6ca74, 0x24ee30633ca52c6a),
        (300, 0xbaad8e6a5f186ce6, 0xa0b29158ee3e8112),
        (1000, 0x989765d0ea7a5ecd, 0xf534f51e82a81d29),
    ];

    #[test]
    fn xxh3_128_matches_upstream_on_every_branch() {
        let mut buf = [0u8; 1000];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = ((i * 31 + 7) & 0xff) as u8;
        }
        for &(len, low, high) in VECTORS {
            assert_eq!(
                xxh3_128(&buf[..len]),
                Xxh128 {
                    low64: low,
                    high64: high
                },
                "XXH3-128 disagrees with rmw_zenoh_cpp's at length {len}"
            );
        }
    }

    /// The whole derivation, not just the hash: a real publisher token, hashed
    /// by upstream's code (same driver as `VECTORS`), laid out in upstream's
    /// byte order. A byte-order slip here passes every vector above.
    #[test]
    fn entity_gid_is_upstreams_for_a_real_token() {
        let key = "@ros2_lv/0/abcdef0123456789abcdef0123456789/0/10/MP/%/%/talker/%chatter/\
                   std_msgs::msg::dds_::String_/\
                   RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18/\
                   ::,7:,:,:,,";
        assert_eq!(
            entity_gid(key),
            [
                0x5d, 0x93, 0xeb, 0x0a, 0xa1, 0xfe, 0xd6, 0x74, 0x1f, 0xd0, 0x75, 0x19, 0x22, 0xff,
                0xa0, 0x30
            ]
        );
    }
}

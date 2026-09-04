//! phase-206 W3 — zenoh-pico's run-time options are its ONLY configuration
//! surface, so the key map is where a typo lives or dies.
//!
//! `zp_config_insert(config, Z_CONFIG_<X>_KEY, value)` is what upstream calls
//! "the primary configuration method"; the pico client has no config-file
//! format at all (JSON5/YAML belongs to `zenohd`, the router). Two things were
//! wrong with how nano-ros exposed it:
//!
//!  * the map in `zpico.c` was a hand-written closed list covering 10 of the 23
//!    keys `config.h` defines — `user`, `password`, `scouting_what` and ten of
//!    the thirteen TLS keys could not be set at all;
//!  * an unrecognised name hit an `else { continue; }` labelled "Unknown key —
//!    silently ignore", so a misspelling produced a session that opened,
//!    reported success, and ran a configuration nobody asked for.
//!
//! The map is derived from `config.h` now (`scripts/gen-zpico-config-keys.py`,
//! gated by `just check zpico-config-keys`), and every rejection below is a
//! hard `InvalidArgument` off `zpico_init_with_config` — which runs BEFORE
//! `zpico_open`, so the refusal tests here need no router and no network.

#![cfg(feature = "platform-posix")]

use nros_rmw::{Session, SessionMode, Transport, TransportConfig, TransportError};
use nros_rmw_zenoh::ZenohTransport;

fn client_config<'a>(
    properties: &'a [(&'a str, &'a str)],
    locator: &'a str,
) -> TransportConfig<'a> {
    TransportConfig {
        locator: Some(locator),
        mode: SessionMode::Client,
        properties,
        node_name: "cfg_keys",
        namespace: "",
        domain_id: 0,
    }
}

#[test]
fn an_unknown_property_key_is_refused_not_ignored() {
    // One transposed letter. This is the entire point: the key set is a bare
    // numbered enum with no schema validation upstream, so nano-ros's map is
    // the only place a typo can ever be caught.
    let cfg = client_config(&[("multicast_scoutingg", "false")], "tcp/127.0.0.1:1");
    let result = ZenohTransport::open(&cfg);
    assert!(
        matches!(result, Err(TransportError::InvalidArgument)),
        "a misspelled key must fail the session; silently ignoring it is \
         indistinguishable from applying it — got {:?}",
        result.map(|_| "Ok(session)")
    );
}

#[test]
fn a_key_upstream_does_not_define_is_refused() {
    // Documented by zenoh-pico's manual, absent from the 1.x we pin: there is
    // no `Z_CONFIG_CONNECT_TIMEOUT_KEY` in this `config.h`. A caller who read
    // the manual and not the header must be told, not humoured.
    let cfg = client_config(&[("connect_timeout", "500")], "tcp/127.0.0.1:1");
    assert!(
        matches!(
            ZenohTransport::open(&cfg),
            Err(TransportError::InvalidArgument)
        ),
        "a key this pin does not define must be refused"
    );
}

#[test]
fn more_properties_than_the_build_carries_is_refused_not_truncated() {
    // Comfortably past both caps (16 hosted, 8 embedded). The bound used to be
    // `config.properties.len().min(MAX_SESSION_PROPERTIES)`, so everything
    // past the eighth simply never happened and the session opened anyway.
    let props: Vec<(&str, &str)> = (0..32).map(|_| ("listen", "tcp/0.0.0.0:0")).collect();
    let cfg = client_config(&props, "tcp/127.0.0.1:1");
    assert!(
        matches!(
            ZenohTransport::open(&cfg),
            Err(TransportError::InvalidArgument)
        ),
        "more properties than the build can carry must be refused, not cut"
    );
}

#[test]
fn an_over_long_property_value_is_refused_not_clipped() {
    let long = "x".repeat(4096);
    let props = [("listen", long.as_str())];
    let cfg = client_config(&props, "tcp/127.0.0.1:1");
    assert!(
        matches!(
            ZenohTransport::open(&cfg),
            Err(TransportError::InvalidArgument)
        ),
        "a value that does not fit the build's buffer must be refused"
    );
}

/// The positive half: keys the hand-written map did NOT have, plus both
/// spellings of a key that was renamed by the derivation, all accepted on a
/// session that actually opens.
///
/// Needs a router, because "accepted" is only observable as a session that
/// reaches `zpico_open`. Skips rather than fails when zenohd is absent — the
/// refusal tests above carry the regression this file exists for and need
/// nothing.
#[test]
fn keys_the_hand_written_map_lacked_are_accepted() {
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found — cannot open a session to observe acceptance");
    }
    let router = nros_tests::fixtures::ZenohRouter::start_unique().expect("start zenohd");
    let locator = router.locator();
    let cfg = client_config(
        &[
            // `scouting_what` was one of the 13 unreachable keys.
            ("scouting_what", "3"),
            // Derived spelling of Z_CONFIG_SCOUTING_TIMEOUT_KEY…
            ("scouting_timeout", "1000"),
            // …and the legacy alias the map used to carry, still accepted.
            ("scouting_timeout_ms", "1000"),
            ("add_timestamp", "false"),
            ("multicast_scouting", "false"),
        ],
        &locator,
    );
    let mut session = ZenohTransport::open(&cfg).expect(
        "every key here is defined by the zenoh-pico we pin, so the session must open; \
         an InvalidArgument means the derived table lost a key",
    );
    assert!(session.is_open());
    session.close().expect("close");
}

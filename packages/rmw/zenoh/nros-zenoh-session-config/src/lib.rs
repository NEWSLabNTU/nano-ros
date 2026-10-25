//! RFC-0109 D8 (phase-514 W3) — a zenoh session config, in upstream zenoh's
//! own json5 language, lowered to the zenoh-pico session properties nano-ros
//! hands `zp_config_insert`.
//!
//! The file a user writes for a nano-ros image is the file they would write
//! for `rmw_zenoh` (`ZENOH_SESSION_CONFIG_URI`): the same keys, the same
//! nesting. zenoh-pico has no document format of its own — its config is a
//! numbered map of strings (`Z_CONFIG_*_KEY`), and nano-ros's shim takes it as
//! named properties (`packages/rmw/zenoh/zpico-sys/c/zpico/zpico_config_keys.h`).
//! So the document is parsed HERE, on the host, and never on an embedded
//! target.
//!
//! **A key zenoh-pico cannot take is refused, never dropped.** A dropped line
//! is indistinguishable from one that took effect, so whoever runs the image
//! debugs the transport rather than the file (phase-206 W3's lesson). That
//! makes the accepted set deliberately small: the subset of zenoh's session
//! config that zenoh-pico implements. It is not a filter over
//! `DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5` — write the keys you mean.

#![no_std]

extern crate alloc;
#[cfg(any(feature = "std", test))]
extern crate std;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use serde_json::Value;

pub mod json5;

/// One lowered property: a `zpico_config_keys.h` name and its value.
pub type Property = (String, String);

/// Lower a json5 zenoh session config to zenoh-pico properties, in document
/// order (`connect` endpoints keep theirs: each becomes one `connect`
/// property, which zenoh-pico appends).
///
/// # Errors
///
/// The text is not json5, the top level is not an object, a key is not one
/// zenoh-pico implements (named by its dotted path), or a value has the wrong
/// type.
pub fn lower(src: &str) -> Result<Vec<Property>, String> {
    let doc: Value = json5::parse(src).map_err(|e| format!("not json5: {e}"))?;
    let Value::Object(top) = doc else {
        return Err("a zenoh session config is a json5 object".to_string());
    };
    let mut out = Vec::new();
    walk(&Value::Object(top), "", &mut out)?;
    Ok(out)
}

/// Lower the file at `path` ([`lower`]), naming it in an error.
///
/// # Errors
///
/// The file cannot be read, or [`lower`] refuses it.
#[cfg(feature = "std")]
pub fn lower_file(path: &std::path::Path) -> Result<Vec<Property>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    lower(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// How one leaf path lowers.
enum Leaf {
    /// A string, under this property name.
    Str(&'static str),
    /// A boolean, as `"true"` / `"false"`.
    Bool(&'static str),
    /// A non-negative integer.
    Int(&'static str),
    /// A list of strings, one property each.
    StrList(&'static str),
    /// A list of strings holding at most one: zenoh-pico listens on a single
    /// endpoint.
    One(&'static str),
    /// `scouting.multicast.address`: zenoh's `"224.0.0.224:7446"` is
    /// zenoh-pico's `"udp/224.0.0.224:7446"`.
    MulticastAddress,
}

/// Every json5 path zenoh-pico implements, and the property it lowers to.
/// The names are `zpico_config_keys.h`'s (a test holds them to it).
const LEAVES: &[(&str, Leaf)] = &[
    ("mode", Leaf::Str("mode")),
    ("id", Leaf::Str("session_zid")),
    ("connect.endpoints", Leaf::StrList("connect")),
    ("listen.endpoints", Leaf::One("listen")),
    ("scouting.timeout", Leaf::Int("scouting_timeout")),
    (
        "scouting.multicast.enabled",
        Leaf::Bool("multicast_scouting"),
    ),
    ("scouting.multicast.address", Leaf::MulticastAddress),
    ("transport.auth.usrpwd.user", Leaf::Str("user")),
    ("transport.auth.usrpwd.password", Leaf::Str("password")),
    (
        "transport.link.tls.root_ca_certificate",
        Leaf::Str("tls_root_ca_certificate"),
    ),
    (
        "transport.link.tls.root_ca_certificate_base64",
        Leaf::Str("tls_root_ca_certificate_base64"),
    ),
    (
        "transport.link.tls.listen_private_key",
        Leaf::Str("tls_listen_private_key"),
    ),
    (
        "transport.link.tls.listen_private_key_base64",
        Leaf::Str("tls_listen_private_key_base64"),
    ),
    (
        "transport.link.tls.listen_certificate",
        Leaf::Str("tls_listen_certificate"),
    ),
    (
        "transport.link.tls.listen_certificate_base64",
        Leaf::Str("tls_listen_certificate_base64"),
    ),
    (
        "transport.link.tls.enable_mtls",
        Leaf::Bool("tls_enable_mtls"),
    ),
    (
        "transport.link.tls.connect_private_key",
        Leaf::Str("tls_connect_private_key"),
    ),
    (
        "transport.link.tls.connect_private_key_base64",
        Leaf::Str("tls_connect_private_key_base64"),
    ),
    (
        "transport.link.tls.connect_certificate",
        Leaf::Str("tls_connect_certificate"),
    ),
    (
        "transport.link.tls.connect_certificate_base64",
        Leaf::Str("tls_connect_certificate_base64"),
    ),
    (
        "transport.link.tls.verify_name_on_connect",
        Leaf::Bool("tls_verify_name_on_connect"),
    ),
];

fn walk(v: &Value, path: &str, out: &mut Vec<Property>) -> Result<(), String> {
    if let Some((_, leaf)) = LEAVES.iter().find(|(p, _)| *p == path) {
        return lower_leaf(path, leaf, v, out);
    }
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk(child, &p, out)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "`{path}` is not a key zenoh-pico implements, so this image cannot honour it \
             (RFC-0109 D8: refused, never dropped). zenoh-pico takes: {}",
            LEAVES
                .iter()
                .map(|(p, _)| *p)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn lower_leaf(path: &str, leaf: &Leaf, v: &Value, out: &mut Vec<Property>) -> Result<(), String> {
    let want = |what: &str| format!("`{path}` must be {what}, not {v}");
    let strings = || -> Result<Vec<String>, String> {
        let Value::Array(a) = v else {
            return Err(want("a list of strings"));
        };
        a.iter()
            .map(|e| {
                e.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| want("a list of strings"))
            })
            .collect()
    };
    match leaf {
        Leaf::Str(name) => {
            let s = v.as_str().ok_or_else(|| want("a string"))?;
            out.push(((*name).to_string(), s.to_string()));
        }
        Leaf::Bool(name) => {
            let b = v.as_bool().ok_or_else(|| want("true or false"))?;
            out.push(((*name).to_string(), b.to_string()));
        }
        Leaf::Int(name) => {
            let n = v.as_u64().ok_or_else(|| want("a non-negative integer"))?;
            out.push(((*name).to_string(), n.to_string()));
        }
        Leaf::StrList(name) => {
            for s in strings()? {
                out.push(((*name).to_string(), s));
            }
        }
        Leaf::One(name) => {
            let list = strings()?;
            if list.len() > 1 {
                return Err(format!(
                    "`{path}` names {} endpoints; zenoh-pico listens on one",
                    list.len()
                ));
            }
            for s in list {
                out.push(((*name).to_string(), s));
            }
        }
        Leaf::MulticastAddress => {
            let s = v.as_str().ok_or_else(|| want("a string"))?;
            let loc = if s.contains('/') {
                s.to_string()
            } else {
                format!("udp/{s}")
            };
            out.push(("multicast_locator".to_string(), loc));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(src: &str) -> Vec<(String, String)> {
        lower(src).unwrap()
    }

    #[test]
    fn the_supported_keys_lower_to_zenoh_pico_properties() {
        let got = props(
            r#"{
              // a peer that listens, and dials two routers
              mode: "peer",
              listen: { endpoints: ["tcp/0.0.0.0:7448"] },
              connect: { endpoints: ["tcp/10.0.2.2:7447", "tcp/10.0.2.3:7447"] },
              scouting: { timeout: 3000, multicast: { enabled: false, address: "224.0.0.224:7446" } },
            }"#,
        );
        let want: Vec<(String, String)> = [
            ("mode", "peer"),
            ("listen", "tcp/0.0.0.0:7448"),
            ("connect", "tcp/10.0.2.2:7447"),
            ("connect", "tcp/10.0.2.3:7447"),
            ("scouting_timeout", "3000"),
            ("multicast_scouting", "false"),
            ("multicast_locator", "udp/224.0.0.224:7446"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        // Object key order is the parser's; compare as sets, but `connect`'s
        // two endpoints must keep theirs.
        let mut a = got.clone();
        a.sort();
        let mut b = want;
        b.sort();
        assert_eq!(a, b);
        let connects: Vec<_> = got.iter().filter(|(k, _)| k == "connect").collect();
        assert_eq!(connects[0].1, "tcp/10.0.2.2:7447");
        assert!(props("{}").is_empty());
    }

    #[test]
    fn a_key_zenoh_pico_cannot_take_is_refused_by_path() {
        for (src, path) in [
            ("{ adminspace: { enabled: true } }", "`adminspace.enabled`"),
            (
                "{ routing: { router: { peers_failover_brokering: true } } }",
                "`routing.router",
            ),
            (
                "{ transport: { unicast: { lowlatency: true } } }",
                "`transport.unicast.lowlatency`",
            ),
        ] {
            let e = lower(src).unwrap_err();
            assert!(e.contains(path) && e.contains("refused"), "{src}\n-> {e}");
        }
        for (src, want) in [
            (
                "{ listen: { endpoints: [\"tcp/a:1\", \"tcp/b:2\"] } }",
                "listens on one",
            ),
            ("{ mode: 3 }", "must be a string"),
            ("{ scouting: { timeout: -1 } }", "non-negative"),
            ("[1]", "json5 object"),
            ("{ mode: ", "not json5"),
        ] {
            let e = lower(src).unwrap_err();
            assert!(e.contains(want), "{src}\n-> {e}");
        }
    }

    /// Every property this lowers to is a name the zpico shim accepts, so a
    /// lowered file can never fail at session open on an unknown key.
    #[test]
    fn every_lowered_name_is_a_zpico_config_key() {
        let header = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../zpico-sys/c/zpico/zpico_config_keys.h"),
        )
        .unwrap();
        let mut names: Vec<&str> = LEAVES
            .iter()
            .filter_map(|(_, l)| match l {
                Leaf::Str(n) | Leaf::Bool(n) | Leaf::Int(n) | Leaf::StrList(n) | Leaf::One(n) => {
                    Some(*n)
                }
                Leaf::MulticastAddress => None,
            })
            .collect();
        names.push("multicast_locator");
        for n in names {
            assert!(
                header.contains(&format!("{{\"{n}\",")),
                "`{n}` is not in zpico_config_keys.h"
            );
        }
    }
}

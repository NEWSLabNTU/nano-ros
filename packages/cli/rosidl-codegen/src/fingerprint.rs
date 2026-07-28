//! Codegen fingerprint — RFC-0061 / phase-318 W1.
//!
//! The workspace-fixture signature used to hash `sha256(target/release/nros)`.
//! Rust binaries are not reproducible across rebuilds, so that hash moved on
//! every `just setup-cli` and invalidated every workspace fixture — measured
//! 2026-07-28: a codegen change that only ADDED a rejection path no fixture uses
//! invalidated 40 fixtures across 35 build dirs, a multi-hour ~100 GB rebuild
//! whose correct answer was zero.
//!
//! What the signature actually wants to know is *"would this tool emit different
//! bytes?"*. So answer that question directly: run every emitter over a corpus
//! compiled into the binary and hash the output.
//!
//! **Why the binary answers for itself.** Computing this from the source tree
//! would track the sources rather than the tool in use, and issue #182's original
//! bug was precisely a STALE BINARY emitting museum output while its sources
//! looked current. The fingerprint has to come from the artifact that will do the
//! generating.
//!
//! **Why an embedded corpus rather than a package on disk.** `nros generate*`
//! emits bindings for a package's DEPENDENCIES, resolved through the ament index,
//! so a self-contained probe package cannot drive it without dragging real
//! interface packages in — whose contents would then move the fingerprint for
//! reasons that have nothing to do with the emitters. `include_str!` keeps the
//! corpus reviewable in a diff while making it immune to what is installed.
//!
//! Adding a shape to the corpus is cheap and safe. REMOVING one silently narrows
//! what the fingerprint can notice — treat deletions like deleting a test.

use crate::{
    CapacityResolver, generate_c_action_package, generate_c_message_package,
    generate_c_service_package, generate_cpp_message_package, generate_nros_action_package,
    generate_nros_message_package, generate_nros_service_package, rihs::ActionTypeHashes,
};
use rosidl_parser::{parse_action, parse_message, parse_service};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const CORPUS: &str = "fingerprint-corpus";

const SHAPES_MSG: &str = include_str!("../tests/fixtures/fingerprint-corpus/msg/Shapes.msg");
const NESTED_MSG: &str = include_str!("../tests/fixtures/fingerprint-corpus/msg/Nested.msg");
const PROBE_SRV: &str = include_str!("../tests/fixtures/fingerprint-corpus/srv/Probe.srv");
const PROBE_ACTION: &str = include_str!("../tests/fixtures/fingerprint-corpus/action/Probe.action");
const CODEGEN_TOML: &str = include_str!("../tests/fixtures/fingerprint-corpus/nros-codegen.toml");

/// Storage-mode variants the corpus is generated under. `owned` is the default
/// path; the configured file exercises `heap` + `borrowed` (issues 0343–0346),
/// which have their own emitter arms and must move the fingerprint when they
/// change.
fn resolvers() -> Vec<(&'static str, CapacityResolver)> {
    let mut v = vec![("owned", CapacityResolver::empty())];
    if let Ok(r) = CapacityResolver::from_toml_str(CODEGEN_TOML) {
        v.push(("configured", r));
    }
    v
}

fn hashes() -> ActionTypeHashes {
    let h = || "fingerprint".to_string();
    ActionTypeHashes {
        goal: h(),
        result: h(),
        feedback: h(),
        send_goal_request: h(),
        send_goal_response: h(),
        get_result_request: h(),
        get_result_response: h(),
        feedback_message: h(),
        action: h(),
        send_goal_service: h(),
        get_result_service: h(),
    }
}

/// Hash of every byte this build of the emitters produces for the corpus.
///
/// Stable across rebuilds that do not change emitted output; moves as soon as any
/// emitted byte does. Errors are folded into the hash rather than propagated: a
/// build whose emitters REJECT a corpus shape is a different tool than one that
/// accepts it, and the signature must notice that too.
pub fn codegen_fingerprint() -> String {
    let mut h = Sha256::new();
    h.update(b"nros-codegen-fingerprint-v1\0");

    let deps: HashSet<String> = HashSet::new();
    let msgs = [("Shapes", SHAPES_MSG), ("Nested", NESTED_MSG)];

    for (mode, r) in resolvers() {
        h.update(mode.as_bytes());
        h.update(b"\0");

        for (name, src) in &msgs {
            let Ok(m) = parse_message(src) else {
                h.update(b"parse-error\0");
                continue;
            };
            feed(&mut h, "msg-rust", |_| {
                generate_nros_message_package(CORPUS, name, &m, &deps, "0.0.0", "h", &r)
                    .map(|g| g.message_rs)
            });
            feed(&mut h, "msg-c-h", |_| {
                generate_c_message_package(CORPUS, name, &m, "h", &r).map(|g| g.header)
            });
            feed(&mut h, "msg-c-c", |_| {
                generate_c_message_package(CORPUS, name, &m, "h", &r).map(|g| g.source)
            });
            feed(&mut h, "msg-cpp", |_| {
                generate_cpp_message_package(CORPUS, name, &m, "h", &r).map(|g| g.header)
            });
        }

        if let Ok(s) = parse_service(PROBE_SRV) {
            feed(&mut h, "srv-rust", |_| {
                generate_nros_service_package(
                    CORPUS, "Probe", &s, &deps, "0.0.0", "h", "h", "h", &r,
                )
                .map(|g| g.service_rs)
            });
            feed(&mut h, "srv-c-h", |_| {
                generate_c_service_package(CORPUS, "Probe", &s, "h", &r).map(|g| g.header)
            });
            feed(&mut h, "srv-c-c", |_| {
                generate_c_service_package(CORPUS, "Probe", &s, "h", &r).map(|g| g.source)
            });
        }

        if let Ok(a) = parse_action(PROBE_ACTION) {
            feed(&mut h, "act-rust", |_| {
                generate_nros_action_package(CORPUS, "Probe", &a, &deps, "0.0.0", &hashes(), &r)
                    .map(|g| g.action_rs)
            });
            feed(&mut h, "act-c-h", |_| {
                generate_c_action_package(CORPUS, "Probe", &a, "h", &r).map(|g| g.header)
            });
            feed(&mut h, "act-c-c", |_| {
                generate_c_action_package(CORPUS, "Probe", &a, "h", &r).map(|g| g.source)
            });
        }
    }

    format!("{:x}", h.finalize())
}

/// Fold one emitter's result into the hash. An `Err` contributes its message, so
/// a change in WHICH shapes are rejected moves the fingerprint exactly as a change
/// in emitted code does.
fn feed<F, E>(h: &mut Sha256, tag: &str, f: F)
where
    F: FnOnce(()) -> Result<String, E>,
    E: std::fmt::Display,
{
    h.update(tag.as_bytes());
    h.update(b"\0");
    match f(()) {
        Ok(s) => {
            h.update(b"ok\0");
            h.update(s.as_bytes());
        }
        Err(e) => {
            h.update(b"err\0");
            h.update(e.to_string().as_bytes());
        }
    }
    h.update(b"\0");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_deterministic() {
        assert_eq!(codegen_fingerprint(), codegen_fingerprint());
    }

    #[test]
    fn fingerprint_is_a_sha256_hex_digest() {
        let f = codegen_fingerprint();
        assert_eq!(f.len(), 64, "expected a sha256 hex digest, got {f:?}");
        assert!(f.chars().all(|c| c.is_ascii_hexdigit()), "{f}");
    }

    /// The corpus must actually reach the emitters — a fingerprint computed over
    /// an empty corpus would be stable, useless, and indistinguishable from a
    /// working one by the tests above.
    #[test]
    fn corpus_exercises_the_emitters() {
        let m = parse_message(SHAPES_MSG).expect("corpus msg must parse");
        let r = CapacityResolver::empty();
        let out = generate_c_message_package(CORPUS, "Shapes", &m, "h", &r)
            .expect("corpus msg must generate");
        for shape in [
            "bool", "int64_t", "double", "char", // primitives + string
        ] {
            assert!(
                out.header.contains(shape),
                "corpus lost the {shape} shape:\n{}",
                out.header
            );
        }
        assert!(
            parse_service(PROBE_SRV).is_ok() && parse_action(PROBE_ACTION).is_ok(),
            "corpus srv/action must parse"
        );
    }
}

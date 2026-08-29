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
use std::collections::{BTreeMap, HashSet};

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
    let mut v = vec![("inline", CapacityResolver::empty())];
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

/// Every artifact this build of the emitters produces for the corpus, keyed by a
/// stable relative path.
///
/// One map, two consumers: [`codegen_fingerprint`] hashes it, and the golden test
/// (phase-318 W2) diffs it against committed files. Sharing the map is the point
/// — a golden test that covered different bytes than the fingerprint could pass
/// while the fingerprint moved, or the reverse, and neither would be believable.
///
/// Errors are recorded as content rather than propagated: a build whose emitters
/// REJECT a corpus shape is a different tool than one that accepts it, and both
/// consumers must notice that.
pub fn emit_corpus() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let deps: HashSet<String> = HashSet::new();
    let msgs = [("Shapes", SHAPES_MSG), ("Nested", NESTED_MSG)];

    for (mode, r) in resolvers() {
        let mut put = |name: &str, res: Result<String, String>| {
            let (tag, body) = match res {
                Ok(s) => ("ok", s),
                Err(e) => ("err", e),
            };
            out.insert(format!("{mode}/{name}"), format!("// emit:{tag}\n{body}"));
        };

        for (name, src) in &msgs {
            let Ok(m) = parse_message(src) else {
                put(&format!("{name}.parse"), Err("parse-error".into()));
                continue;
            };
            put(
                &format!("{name}.nros.rs"),
                generate_nros_message_package(CORPUS, name, &m, &deps, "0.0.0", "h", &r)
                    .map(|g| g.message_rs)
                    .map_err(|e| e.to_string()),
            );
            put(
                &format!("{name}.h"),
                generate_c_message_package(CORPUS, name, &m, "h", &r)
                    .map(|g| g.header)
                    .map_err(|e| e.to_string()),
            );
            put(
                &format!("{name}.c"),
                generate_c_message_package(CORPUS, name, &m, "h", &r)
                    .map(|g| g.source)
                    .map_err(|e| e.to_string()),
            );
            put(
                &format!("{name}.hpp"),
                generate_cpp_message_package(CORPUS, name, &m, "h", &r)
                    .map(|g| g.header)
                    .map_err(|e| e.to_string()),
            );
        }

        match parse_service(PROBE_SRV) {
            Ok(sv) => {
                put(
                    "Probe.srv.nros.rs",
                    generate_nros_service_package(
                        CORPUS, "Probe", &sv, &deps, "0.0.0", "h", "h", "h", &r,
                    )
                    .map(|g| g.service_rs)
                    .map_err(|e| e.to_string()),
                );
                put(
                    "Probe.srv.h",
                    generate_c_service_package(CORPUS, "Probe", &sv, "h", &r)
                        .map(|g| g.header)
                        .map_err(|e| e.to_string()),
                );
                put(
                    "Probe.srv.c",
                    generate_c_service_package(CORPUS, "Probe", &sv, "h", &r)
                        .map(|g| g.source)
                        .map_err(|e| e.to_string()),
                );
            }
            Err(_) => put("Probe.srv.parse", Err("parse-error".into())),
        }

        match parse_action(PROBE_ACTION) {
            Ok(ac) => {
                put(
                    "Probe.action.nros.rs",
                    generate_nros_action_package(
                        CORPUS,
                        "Probe",
                        &ac,
                        &deps,
                        "0.0.0",
                        &hashes(),
                        &r,
                    )
                    .map(|g| g.action_rs)
                    .map_err(|e| e.to_string()),
                );
                put(
                    "Probe.action.h",
                    generate_c_action_package(CORPUS, "Probe", &ac, "h", &r)
                        .map(|g| g.header)
                        .map_err(|e| e.to_string()),
                );
                put(
                    "Probe.action.c",
                    generate_c_action_package(CORPUS, "Probe", &ac, "h", &r)
                        .map(|g| g.source)
                        .map_err(|e| e.to_string()),
                );
            }
            Err(_) => put("Probe.action.parse", Err("parse-error".into())),
        }
    }
    out
}

/// Hash of every byte this build of the emitters produces for the corpus.
///
/// Stable across rebuilds that do not change emitted output; moves as soon as any
/// emitted byte does.
pub fn codegen_fingerprint() -> String {
    let mut h = Sha256::new();
    h.update(b"nros-codegen-fingerprint-v2\0");
    for (path, body) in emit_corpus() {
        h.update(path.as_bytes());
        h.update(b"\0");
        h.update(body.as_bytes());
        h.update(b"\0");
    }
    // phase-335 W4.a — hash every bundled pack file too, so a template edit the
    // emit corpus doesn't exercise (rmw / idiomatic / scaffolding / cpp srv+action)
    // still moves the fingerprint and marks fixtures stale.
    h.update(b"packs\0");
    for (name, content) in crate::render::bundled_packs() {
        h.update(name.as_bytes());
        h.update(b"\0");
        h.update(content.as_bytes());
        h.update(b"\0");
    }
    format!("{:x}", h.finalize())
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

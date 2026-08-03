//! The neutral, target-independent IR (RFC-0068 Stage 1 output).
//!
//! `Resolved*` wraps the parsed body with the facts a generator needs but must
//! not recompute: fully-qualified name, RIHS type hash, and the canonical
//! type-description closure. It is deliberately LANGUAGE- and TARGET-neutral —
//! per-field storage / `repr(C)` layout / plainness are computed by the Lower
//! stage (phase-335 W1.b), never here.

use rosidl_parser::ast::{Action, Message, Service};

use crate::rihs::{self, ActionTypeHashes, TypeDescription};

/// A message resolved into the neutral IR.
#[derive(Debug, Clone)]
pub struct ResolvedMessage {
    /// The parsed message body (unchanged from Stage 0).
    pub parsed: Message,
    /// Fully-qualified ROS type name, `pkg/msg/Name`.
    pub type_name: String,
    /// Canonical REP-2011 type-description closure (the hash input; also the
    /// payload a type-description service would serve).
    pub type_description: TypeDescription,
    /// `RIHS01_<64 hex>`.
    pub type_hash: String,
}

impl ResolvedMessage {
    /// Resolve `msg`, named `pkg/msg/Name`, using `resolve` to reach nested
    /// dependency types (the ambient ament/offline resolver).
    pub fn resolve(
        type_name: impl Into<String>,
        msg: &Message,
        resolve: impl Fn(&str) -> Option<Message>,
    ) -> Result<Self, String> {
        let type_name = type_name.into();
        let type_description = rihs::build_type_description(&type_name, msg, resolve)?;
        let type_hash = rihs::rihs01(&type_description);
        Ok(Self {
            parsed: msg.clone(),
            type_name,
            type_description,
            type_hash,
        })
    }
}

/// A service resolved into the neutral IR: the service-level hash plus its two
/// member messages resolved as REP-2011 service members.
#[derive(Debug, Clone)]
pub struct ResolvedService {
    pub parsed: Service,
    /// Fully-qualified name, `pkg/srv/Name`.
    pub type_name: String,
    pub package: String,
    pub name: String,
    /// Service-level type description (request/response/event framing).
    pub type_description: TypeDescription,
    /// Service-level `RIHS01_…`.
    pub type_hash: String,
    /// `_Request` member hash.
    pub request_hash: String,
    /// `_Response` member hash.
    pub response_hash: String,
}

impl ResolvedService {
    pub fn resolve(
        package: &str,
        name: &str,
        srv: &Service,
        resolve: impl Fn(&str) -> Option<Message> + Copy,
    ) -> Result<Self, String> {
        let type_description = rihs::build_service_type_description(
            package,
            name,
            &srv.request,
            &srv.response,
            resolve,
        )?;
        let type_hash = rihs::rihs01(&type_description);
        let request_hash = rihs::rihs01(&rihs::service_member_type_description(
            package,
            name,
            "_Request",
            &srv.request,
            resolve,
        )?);
        let response_hash = rihs::rihs01(&rihs::service_member_type_description(
            package,
            name,
            "_Response",
            &srv.response,
            resolve,
        )?);
        Ok(Self {
            parsed: srv.clone(),
            type_name: format!("{package}/srv/{name}"),
            package: package.to_string(),
            name: name.to_string(),
            type_description,
            type_hash,
            request_hash,
            response_hash,
        })
    }
}

/// An action resolved into the neutral IR: the full REP-2011 §3b hash bundle
/// (goal/result/feedback + the two nested services) plus the top-level
/// action type description.
#[derive(Debug, Clone)]
pub struct ResolvedAction {
    pub parsed: Action,
    /// Fully-qualified name, `pkg/action/Name`.
    pub type_name: String,
    pub package: String,
    pub name: String,
    pub type_description: TypeDescription,
    /// All nine action protocol hashes.
    pub hashes: ActionTypeHashes,
}

impl ResolvedAction {
    pub fn resolve(
        package: &str,
        name: &str,
        action: &Action,
        resolve: impl Fn(&str) -> Option<Message> + Copy,
    ) -> Result<Self, String> {
        let goal = &action.spec.goal;
        let result = &action.spec.result;
        let feedback = &action.spec.feedback;
        let hashes = rihs::action_type_hashes(package, name, goal, result, feedback, resolve)?;
        let type_description =
            rihs::build_action_type_description(package, name, goal, result, feedback, resolve)?;
        Ok(Self {
            parsed: action.clone(),
            type_name: format!("{package}/action/{name}"),
            package: package.to_string(),
            name: name.to_string(),
            type_description,
            hashes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosidl_parser::parse_message;
    use std::cell::Cell;

    fn no_deps(_: &str) -> Option<Message> {
        None
    }

    #[test]
    fn resolved_message_carries_hash_and_name() {
        let msg = parse_message("int32 x\nfloat64 y\n").unwrap();
        let r = ResolvedMessage::resolve("test_msgs/msg/Point", &msg, no_deps).unwrap();
        assert_eq!(r.type_name, "test_msgs/msg/Point");
        assert!(r.type_hash.starts_with("RIHS01_"));
        assert_eq!(r.type_hash.len(), 71);
        // The wrapper's hash must equal the direct engine call it stands in for.
        let td = rihs::build_type_description("test_msgs/msg/Point", &msg, no_deps).unwrap();
        assert_eq!(r.type_hash, rihs::rihs01(&td));
    }

    // phase-335 W5 — the resolve-only contract of the Resolve seam.
    //
    // A cross-package dependency reaches the hash ONLY through the `resolve`
    // closure: the seam pulls the nested type's description into the DAG so the
    // hash is correct, but it produces no artifact for that dependency — emitting
    // a crate for it is the caller's later, separate decision (see phase-333 /
    // RFC-0067, which settled that a structurally-embedded dep gets its own
    // `0.0.0` path crate). These tests pin the two halves: the closure IS the dep
    // channel, and an ABSENT closure is a hard error, never a plausible-but-wrong
    // hash on the wire.

    #[test]
    fn cross_package_dep_reaches_the_hash_only_through_the_resolver() {
        // std_msgs/Header references builtin_interfaces/Time — a cross-package
        // nested type the seam cannot see except through the closure.
        let header = parse_message("builtin_interfaces/Time stamp\nstring frame_id\n").unwrap();
        let consulted = Cell::new(false);
        let resolve = |fqn: &str| -> Option<Message> {
            if fqn == "builtin_interfaces/msg/Time" {
                consulted.set(true);
                parse_message("int32 sec\nuint32 nanosec\n").ok()
            } else {
                None
            }
        };
        let r = ResolvedMessage::resolve("std_msgs/msg/Header", &header, resolve).unwrap();
        assert!(
            consulted.get(),
            "the resolver is the ONLY channel a cross-package dep enters the DAG — it must be consulted"
        );
        assert!(r.type_hash.starts_with("RIHS01_"));
        assert_eq!(r.type_hash.len(), 71);
    }

    #[test]
    fn unresolvable_cross_package_dep_is_a_hard_error_not_a_wrong_hash() {
        // Same Header, but the closure cannot supply Time. A missing nested type
        // must FAIL loudly — a wrong hash silently breaks discovery on the wire.
        let header = parse_message("builtin_interfaces/Time stamp\nstring frame_id\n").unwrap();
        let err = ResolvedMessage::resolve("std_msgs/msg/Header", &header, no_deps)
            .expect_err("an unresolvable nested type must be an error, never a hash");
        assert!(
            err.contains("cannot resolve nested type"),
            "expected the loud unresolved-nested error, got: {err}"
        );
    }
}

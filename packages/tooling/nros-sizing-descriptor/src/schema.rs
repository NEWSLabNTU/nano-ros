//! The descriptor's typed shape — RFC-0100 D4.
//!
//! # How per-field status is spelled on the wire
//!
//! A section holds its values as ordinary TOML keys and its refusals in one
//! sub-table beside them:
//!
//! ```toml
//! [[endpoint]]
//! kind = "subscription"
//! type = "sensor_msgs/msg/Image"
//! topic = "/image"
//! history = "keep_all"
//! wire_bound_bytes = 1170
//!
//! [endpoint.refused]
//! depth = "history = keep_all on subscription /image: a KEEP_ALL queue has no static bound"
//! storage_bytes = "depends on `depth`, which is refused above"
//! ```
//!
//! So the common case — a fully derived image — reads exactly like the D4 sketch
//! and costs nothing, while a refusal carries its prose to the consumer that
//! needs it instead of to a build log nobody kept.
//!
//! Three rules make the encoding safe, and all three are enforced at PARSE:
//!
//! * a key in both the value slot and `refused` is a CONTRADICTION and an
//!   error. It is exactly the shape that reads as derived and is not;
//! * a `refused` key naming a field the section does not have is an error. A
//!   refusal nobody can read is worse than no refusal — the consumer defaults
//!   silently and the producer believes it warned;
//! * an unknown key anywhere is an error (`deny_unknown_fields`), so a field
//!   added by a newer writer is a loud refusal to read rather than a number
//!   silently missing.
//!
//! # What is a fact and what is identity
//!
//! `kind`, `type` and `topic` are the join key every consumer of the entity and
//! bound inventories already uses (`DeclaredDepth`, `DeclaredQosPolicies`, both
//! sorted by `(kind, type_name, topic)`). They are REQUIRED: a row without them
//! is not an endpoint that refused something, it is a row nothing can join, and
//! the inventories that produce these rows carry all three unconditionally.
//! Everything else is a [`Fact`].

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{
    fact::Fact,
    vocabulary::{Basis, Durability, EndpointKind, History, RegistrationPath, Reliability, Status},
};

/// Resolve one field to its [`Fact`]: refused beats absent, and a value that is
/// present is stated.
///
/// The contradiction (present AND refused) cannot reach here — [`validate`]
/// rejects it at parse — so this is a total function over the states that exist.
fn fact<T: Clone>(value: &Option<T>, key: &str, refused: &BTreeMap<String, String>) -> Fact<T> {
    match (value, refused.get(key)) {
        (Some(v), None) => Fact::Stated(v.clone()),
        (None, Some(r)) => Fact::Refused(r.clone()),
        (None, None) => Fact::Absent,
        // Unreachable after `validate`; spelled as a refusal rather than a
        // panic so a hand-edited file cannot take a build script down.
        (Some(_), Some(r)) => Fact::Refused(r.clone()),
    }
}

/// Reject a `refused` table naming something this section does not have.
fn known_refusals(
    section: &str,
    refused: &BTreeMap<String, String>,
    fields: &[&str],
    present: &[(&str, bool)],
) -> Result<(), String> {
    for key in refused.keys() {
        if !fields.contains(&key.as_str()) {
            return Err(format!(
                "[{section}] refuses `{key}`, which is not a field of this section \
                 -- expected one of: {}",
                fields.join(", ")
            ));
        }
    }
    for (key, is_present) in present {
        if *is_present && refused.contains_key(*key) {
            return Err(format!(
                "[{section}] both states and refuses `{key}` -- a field that carries a \
                 value and a refusal reads as derived to every consumer that checks \
                 only one of them (RFC-0100 D6)"
            ));
        }
    }
    Ok(())
}

/// `[meta]` — what this descriptor is and how far it got.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    /// The entry this descriptor describes — the file's own stem, restated
    /// inside it so a copied file still says what it is about.
    pub entry: String,
    /// A SUMMARY of the per-field statuses. Never a substitute for reading the
    /// field a consumer needs.
    pub status: Status,
    /// What the numbers describe. D6 forbids widening it silently.
    pub basis: Basis,
    undeclared_endpoints: Option<usize>,
    #[serde(default)]
    refused: BTreeMap<String, String>,
}

impl Meta {
    pub(crate) const FIELDS: &'static [&'static str] = &["undeclared_endpoints"];

    /// Endpoints that could carry a per-endpoint fact and stated none.
    ///
    /// The load-bearing number, for the reason `DeclaredDepths::undeclared` is:
    /// `endpoints = []` and `endpoints = [...]` with `undeclared_endpoints = 4`
    /// are different facts, and a consumer that attributes per endpoint must
    /// refuse on both while one that counts must refuse on neither. **Absence is
    /// not zero**, which is why this is a [`Fact`] and not a `usize` that
    /// defaults: an inventory that could not compose knows nothing about how
    /// many endpoints stayed silent, and reporting 0 there would read as "every
    /// endpoint declared".
    pub fn undeclared_endpoints(&self) -> Fact<usize> {
        fact(
            &self.undeclared_endpoints,
            "undeclared_endpoints",
            &self.refused,
        )
    }

    pub fn set_undeclared_endpoints(&mut self, v: Option<usize>) -> &mut Self {
        self.undeclared_endpoints = v;
        self
    }

    pub fn refuse(&mut self, field: &str, reason: impl Into<String>) -> &mut Self {
        debug_assert!(
            Self::FIELDS.contains(&field),
            "unknown [meta] field {field}"
        );
        self.refused.insert(field.to_string(), reason.into());
        self
    }

    pub(crate) fn refusals(&self) -> &BTreeMap<String, String> {
        &self.refused
    }

    pub(crate) fn raw_value(&self, field: &str) -> Option<String> {
        match field {
            "undeclared_endpoints" => self.undeclared_endpoints.map(|v| v.to_string()),
            _ => None,
        }
    }

    fn validate(&self) -> Result<(), String> {
        known_refusals(
            "meta",
            &self.refused,
            Self::FIELDS,
            &[("undeclared_endpoints", self.undeclared_endpoints.is_some())],
        )
    }
}

/// `[target]` — the board's facts, never the host's.
///
/// RFC-0100 D1 and the reason this section exists at all:
///
/// > Target facts are separate because **build scripts run for the host**
/// > (phase-118-E), so `DEP_NROS_NODE_*` carry host sizes on a cross build.
/// > Storage capacity is the target-ABI-dependent size; it cannot be inferred
/// > where it is computed today.
///
/// The live instance of that: `nros-node/build.rs` prices an `SpscRing`'s
/// per-slot length array at a hard `RING_LEN_BYTES = 8`, whose own comment says
/// *"taken at its 64-bit width. A 32-bit target spends 4, so this over-states
/// there"*. The over-statement is deliberate and safe; it is also a number the
/// board knows exactly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pointer_bytes: Option<usize>,
    max_align: Option<usize>,
    heap_budget_bytes: Option<usize>,
    #[serde(default)]
    refused: BTreeMap<String, String>,
}

impl Target {
    pub(crate) const FIELDS: &'static [&'static str] =
        &["pointer_bytes", "max_align", "heap_budget_bytes"];

    /// `size_of::<usize>()` on the TARGET.
    pub fn pointer_bytes(&self) -> Fact<usize> {
        fact(&self.pointer_bytes, "pointer_bytes", &self.refused)
    }

    /// The largest fundamental alignment a pool on this target must satisfy.
    pub fn max_align(&self) -> Fact<usize> {
        fact(&self.max_align, "max_align", &self.refused)
    }

    /// The heap this image is configured with — D11's Cyclone budget, and the
    /// number an image asserts at boot.
    pub fn heap_budget_bytes(&self) -> Fact<usize> {
        fact(&self.heap_budget_bytes, "heap_budget_bytes", &self.refused)
    }

    /// Build one. `None` on a field the caller could not answer leaves it
    /// ABSENT; use [`Self::refuse`] to say why instead.
    pub fn new(
        pointer_bytes: Option<usize>,
        max_align: Option<usize>,
        heap_budget_bytes: Option<usize>,
    ) -> Self {
        Self {
            pointer_bytes,
            max_align,
            heap_budget_bytes,
            refused: BTreeMap::new(),
        }
    }

    /// Record that `field` could not be derived, and why.
    pub fn refuse(&mut self, field: &str, reason: impl Into<String>) -> &mut Self {
        debug_assert!(
            Self::FIELDS.contains(&field),
            "unknown [target] field {field}"
        );
        self.refused.insert(field.to_string(), reason.into());
        self
    }

    pub(crate) fn refusals(&self) -> &BTreeMap<String, String> {
        &self.refused
    }

    pub(crate) fn raw_value(&self, field: &str) -> Option<String> {
        match field {
            "pointer_bytes" => self.pointer_bytes.map(|v| v.to_string()),
            "max_align" => self.max_align.map(|v| v.to_string()),
            "heap_budget_bytes" => self.heap_budget_bytes.map(|v| v.to_string()),
            _ => None,
        }
    }

    fn validate(&self) -> Result<(), String> {
        known_refusals(
            "target",
            &self.refused,
            Self::FIELDS,
            &[
                ("pointer_bytes", self.pointer_bytes.is_some()),
                ("max_align", self.max_align.is_some()),
                ("heap_budget_bytes", self.heap_budget_bytes.is_some()),
            ],
        )
    }
}

/// `[[endpoint]]` — one endpoint, keyed the way both inventories key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub kind: EndpointKind,
    /// ROS `pkg/msg/Name` — the spelling the bound inventory prices and the
    /// entity inventory declares, so the two join with no second convention.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Topic, service or action name.
    pub topic: String,
    history: Option<History>,
    depth: Option<u32>,
    reliability: Option<Reliability>,
    durability: Option<Durability>,
    registration_path: Option<RegistrationPath>,
    storage_bytes: Option<usize>,
    wire_bound_bytes: Option<usize>,
    #[serde(default)]
    refused: BTreeMap<String, String>,
}

impl Endpoint {
    pub(crate) const FIELDS: &'static [&'static str] = &[
        "history",
        "depth",
        "reliability",
        "durability",
        "registration_path",
        "storage_bytes",
        "wire_bound_bytes",
    ];

    /// A row that states nothing but its identity.
    pub fn new(kind: EndpointKind, type_name: impl Into<String>, topic: impl Into<String>) -> Self {
        Self {
            kind,
            type_name: type_name.into(),
            topic: topic.into(),
            history: None,
            depth: None,
            reliability: None,
            durability: None,
            registration_path: None,
            storage_bytes: None,
            wire_bound_bytes: None,
            refused: BTreeMap::new(),
        }
    }

    pub fn history(&self) -> Fact<History> {
        fact(&self.history, "history", &self.refused)
    }

    /// The QoS history depth. **Refused, never defaulted, on `keep_all`.**
    pub fn depth(&self) -> Fact<u32> {
        fact(&self.depth, "depth", &self.refused)
    }

    pub fn reliability(&self) -> Fact<Reliability> {
        fact(&self.reliability, "reliability", &self.refused)
    }

    pub fn durability(&self) -> Fact<Durability> {
        fact(&self.durability, "durability", &self.refused)
    }

    /// Which of issue 1319's four registration paths this endpoint takes.
    pub fn registration_path(&self) -> Fact<RegistrationPath> {
        fact(&self.registration_path, "registration_path", &self.refused)
    }

    /// **Issue 1319's table, applied to this row** — the bytes ONE receive slot
    /// actually claims at registration.
    ///
    /// [`storage_bytes`](Self::storage_bytes) is the whole region priced at the
    /// TYPE'S bound, which is the right number for two of the four registration
    /// paths and 1,848 bytes per subscription too small for the other two. This
    /// is the per-slot half, and it is the half that turns on the path:
    ///
    /// | path | slot |
    /// | --- | --- |
    /// | `c_typed_hint` | the type's own `_RX` |
    /// | `rust_typed_descriptors` | `min(framed(bound), RX_BUF)` — at or below it |
    /// | `rust_typed_schemaless` | `closure_buffer_bytes` |
    /// | `c_raw_no_hint` | `closure_buffer_bytes` |
    ///
    /// `closure_buffer_bytes` is the consumer's `RX_BUF` — `DEFAULT_RX_BUF_SIZE`
    /// in the executor, `NROS_SUBSCRIPTION_BUFFER_SIZE` on the wire. It is not a
    /// descriptor fact and cannot be: it is derived over the whole LINK CLOSURE
    /// by the lane that builds the image, and a type this image only publishes
    /// still has to fit it. So the caller supplies it.
    ///
    /// `unpriced_type_fallback` stands in for a row whose `wire_bound_bytes` the
    /// producer could not state — the image-wide subscribed class, which is an
    /// upper bound over every type this image subscribes to and therefore over
    /// this one. **Absence is not zero**, at either level.
    ///
    /// # Why a refused path yields no number
    ///
    /// The two `closure_buffer_bytes` rows are an UNDER-size, which is the
    /// direction that ships `NodeError::BufferTooSmall` at a registration the
    /// arena oracle passed. A reader that guessed here would guess in that
    /// direction half the time, so this refuses and hands the refusal's prose to
    /// the caller — which then picks the safe direction LOUDLY, the way
    /// [`Fact::or_report`] is written for.
    ///
    /// `Fact::Absent` for any kind that receives no topic sample: a publisher
    /// serializes into a per-call transmit buffer and claims no slot at all, so
    /// there is nothing here to refuse.
    pub fn claimed_slot_bytes(
        &self,
        closure_buffer_bytes: usize,
        unpriced_type_fallback: usize,
    ) -> Fact<usize> {
        if !self.kind.receives_topic_sample() {
            return Fact::Absent;
        }
        match self.registration_path() {
            Fact::Stated(p) if p.claims_closure_buffer() => Fact::Stated(closure_buffer_bytes),
            Fact::Stated(_) => Fact::Stated(
                self.wire_bound_bytes()
                    .get()
                    .unwrap_or(unpriced_type_fallback),
            ),
            Fact::Refused(r) => Fact::Refused(r),
            Fact::Absent => Fact::Absent,
        }
    }

    /// Can this row's registration claim the CLOSURE buffer?
    ///
    /// `true` for the two paths that do — and for a path this row does not
    /// state, because "I cannot tell" and "it does" call for the same price.
    /// The asymmetry is issue 1319's: over-stating a slot costs bytes, and
    /// under-stating one costs the registration.
    ///
    /// `false` for a kind that claims no slot.
    pub fn may_claim_closure_buffer(&self) -> bool {
        self.kind.receives_topic_sample()
            && match self.registration_path() {
                Fact::Stated(p) => p.claims_closure_buffer(),
                Fact::Refused(_) | Fact::Absent => true,
            }
    }

    /// Bytes this endpoint's receive region claims in the executor arena.
    ///
    /// Target-ABI-dependent, so it is refused whenever `[target]` is — D6's
    /// fifth trigger, and the one that keeps a host build script from answering
    /// a cross build's question.
    pub fn storage_bytes(&self) -> Fact<usize> {
        fact(&self.storage_bytes, "storage_bytes", &self.refused)
    }

    /// The type's transport-framed serialized-size bound.
    pub fn wire_bound_bytes(&self) -> Fact<usize> {
        fact(&self.wire_bound_bytes, "wire_bound_bytes", &self.refused)
    }

    /// Builder setters. Each takes `Option` so a caller that has no answer
    /// leaves the field absent by writing what it actually knows.
    pub fn set_history(&mut self, v: Option<History>) -> &mut Self {
        self.history = v;
        self
    }
    pub fn set_depth(&mut self, v: Option<u32>) -> &mut Self {
        self.depth = v;
        self
    }
    pub fn set_reliability(&mut self, v: Option<Reliability>) -> &mut Self {
        self.reliability = v;
        self
    }
    pub fn set_durability(&mut self, v: Option<Durability>) -> &mut Self {
        self.durability = v;
        self
    }
    pub fn set_registration_path(&mut self, v: Option<RegistrationPath>) -> &mut Self {
        self.registration_path = v;
        self
    }
    pub fn set_storage_bytes(&mut self, v: Option<usize>) -> &mut Self {
        self.storage_bytes = v;
        self
    }
    pub fn set_wire_bound_bytes(&mut self, v: Option<usize>) -> &mut Self {
        self.wire_bound_bytes = v;
        self
    }

    /// Record that `field` could not be derived, and why.
    pub fn refuse(&mut self, field: &str, reason: impl Into<String>) -> &mut Self {
        debug_assert!(
            Self::FIELDS.contains(&field),
            "unknown [[endpoint]] field {field}"
        );
        self.refused.insert(field.to_string(), reason.into());
        self
    }

    pub(crate) fn refusals(&self) -> &BTreeMap<String, String> {
        &self.refused
    }

    pub(crate) fn raw_value(&self, field: &str) -> Option<String> {
        Some(match field {
            "history" => format!("\"{}\"", self.history?.tag()),
            "depth" => self.depth?.to_string(),
            "reliability" => format!("\"{}\"", self.reliability?.tag()),
            "durability" => format!("\"{}\"", self.durability?.tag()),
            "registration_path" => format!("\"{}\"", self.registration_path?.tag()),
            "storage_bytes" => self.storage_bytes?.to_string(),
            "wire_bound_bytes" => self.wire_bound_bytes?.to_string(),
            _ => return None,
        })
    }

    fn validate(&self) -> Result<(), String> {
        let section = format!("endpoint {} {}", self.kind.tag(), self.topic);
        known_refusals(
            &section,
            &self.refused,
            Self::FIELDS,
            &[
                ("history", self.history.is_some()),
                ("depth", self.depth.is_some()),
                ("reliability", self.reliability.is_some()),
                ("durability", self.durability.is_some()),
                ("registration_path", self.registration_path.is_some()),
                ("storage_bytes", self.storage_bytes.is_some()),
                ("wire_bound_bytes", self.wire_bound_bytes.is_some()),
            ],
        )
    }
}

/// `[types]` — Cyclone's whole appetite (RFC-0100 D5).
///
/// > Cyclone: reads `[types]`, `[target].heap_budget_bytes`. **Nothing else**.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Types {
    distinct_count: Option<usize>,
    max_fields: Option<usize>,
    max_kinds: Option<usize>,
    max_nested_depth: Option<usize>,
    #[serde(default)]
    refused: BTreeMap<String, String>,
}

impl Types {
    pub(crate) const FIELDS: &'static [&'static str] = &[
        "distinct_count",
        "max_fields",
        "max_kinds",
        "max_nested_depth",
    ];

    pub fn distinct_count(&self) -> Fact<usize> {
        fact(&self.distinct_count, "distinct_count", &self.refused)
    }
    pub fn max_fields(&self) -> Fact<usize> {
        fact(&self.max_fields, "max_fields", &self.refused)
    }
    pub fn max_kinds(&self) -> Fact<usize> {
        fact(&self.max_kinds, "max_kinds", &self.refused)
    }
    pub fn max_nested_depth(&self) -> Fact<usize> {
        fact(&self.max_nested_depth, "max_nested_depth", &self.refused)
    }

    pub fn new(
        distinct_count: Option<usize>,
        max_fields: Option<usize>,
        max_kinds: Option<usize>,
        max_nested_depth: Option<usize>,
    ) -> Self {
        Self {
            distinct_count,
            max_fields,
            max_kinds,
            max_nested_depth,
            refused: BTreeMap::new(),
        }
    }

    pub fn refuse(&mut self, field: &str, reason: impl Into<String>) -> &mut Self {
        debug_assert!(
            Self::FIELDS.contains(&field),
            "unknown [types] field {field}"
        );
        self.refused.insert(field.to_string(), reason.into());
        self
    }

    pub(crate) fn refusals(&self) -> &BTreeMap<String, String> {
        &self.refused
    }

    pub(crate) fn raw_value(&self, field: &str) -> Option<String> {
        Some(match field {
            "distinct_count" => self.distinct_count?.to_string(),
            "max_fields" => self.max_fields?.to_string(),
            "max_kinds" => self.max_kinds?.to_string(),
            "max_nested_depth" => self.max_nested_depth?.to_string(),
            _ => return None,
        })
    }

    fn validate(&self) -> Result<(), String> {
        known_refusals(
            "types",
            &self.refused,
            Self::FIELDS,
            &[
                ("distinct_count", self.distinct_count.is_some()),
                ("max_fields", self.max_fields.is_some()),
                ("max_kinds", self.max_kinds.is_some()),
                ("max_nested_depth", self.max_nested_depth.is_some()),
            ],
        )
    }
}

/// `[policy]` — the facts nobody can derive (RFC-0100 D1).
///
/// > Policy is its own kind because the tree already argues it correctly and
/// > would otherwise lose the argument. […] a policy fact is *stated*, and a
/// > derived fact may supply its default.
///
/// So a number here is STATED, and a consumer that finds one absent falls back to
/// its own default rather than to a derivation — the rung order is RFC-0049's and
/// is not re-litigated by this file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    graph_max_entities: Option<usize>,
    transport_mtu: Option<usize>,
    sessions: Option<usize>,
    #[serde(default)]
    refused: BTreeMap<String, String>,
}

impl Policy {
    pub(crate) const FIELDS: &'static [&'static str] =
        &["graph_max_entities", "transport_mtu", "sessions"];

    pub fn graph_max_entities(&self) -> Fact<usize> {
        fact(
            &self.graph_max_entities,
            "graph_max_entities",
            &self.refused,
        )
    }
    pub fn transport_mtu(&self) -> Fact<usize> {
        fact(&self.transport_mtu, "transport_mtu", &self.refused)
    }
    pub fn sessions(&self) -> Fact<usize> {
        fact(&self.sessions, "sessions", &self.refused)
    }

    pub fn new(
        graph_max_entities: Option<usize>,
        transport_mtu: Option<usize>,
        sessions: Option<usize>,
    ) -> Self {
        Self {
            graph_max_entities,
            transport_mtu,
            sessions,
            refused: BTreeMap::new(),
        }
    }

    pub fn refuse(&mut self, field: &str, reason: impl Into<String>) -> &mut Self {
        debug_assert!(
            Self::FIELDS.contains(&field),
            "unknown [policy] field {field}"
        );
        self.refused.insert(field.to_string(), reason.into());
        self
    }

    pub(crate) fn refusals(&self) -> &BTreeMap<String, String> {
        &self.refused
    }

    pub(crate) fn raw_value(&self, field: &str) -> Option<String> {
        Some(match field {
            "graph_max_entities" => self.graph_max_entities?.to_string(),
            "transport_mtu" => self.transport_mtu?.to_string(),
            "sessions" => self.sessions?.to_string(),
            _ => return None,
        })
    }

    fn validate(&self) -> Result<(), String> {
        known_refusals(
            "policy",
            &self.refused,
            Self::FIELDS,
            &[
                ("graph_max_entities", self.graph_max_entities.is_some()),
                ("transport_mtu", self.transport_mtu.is_some()),
                ("sessions", self.sessions.is_some()),
            ],
        )
    }
}

/// One entry's sizing descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizingDescriptor {
    /// Refused rather than assumed when it is not [`crate::SCHEMA_VERSION`] —
    /// a reader that kept going on a version it does not know would size from
    /// numbers whose meaning has moved, which is what bumped the entity
    /// inventory's own version to 5.
    pub schema_version: u32,
    pub meta: Meta,
    #[serde(default)]
    pub target: Target,
    /// Sorted by `(kind, type, topic)` — the same order both inventories use,
    /// and what makes the artifact byte-stable so a write-if-changed keeps
    /// mtimes still.
    #[serde(default, rename = "endpoint")]
    pub endpoints: Vec<Endpoint>,
    #[serde(default)]
    pub types: Types,
    #[serde(default)]
    pub policy: Policy,
}

impl SizingDescriptor {
    /// A descriptor stating only what it is.
    pub fn new(entry: impl Into<String>, status: Status, basis: Basis) -> Self {
        Self {
            schema_version: crate::SCHEMA_VERSION,
            meta: Meta {
                entry: entry.into(),
                status,
                basis,
                undeclared_endpoints: None,
                refused: BTreeMap::new(),
            },
            target: Target::default(),
            endpoints: Vec::new(),
            types: Types::default(),
            policy: Policy::default(),
        }
    }

    /// Put the endpoint rows in the one canonical order.
    ///
    /// Called by the writer; a caller building a descriptor by hand does not
    /// have to sort. Byte-stability is a property of the ARTIFACT, so it is the
    /// artifact's writer that owes it.
    pub fn sort_endpoints(&mut self) {
        self.endpoints.sort_by(|a, b| {
            (a.kind, &a.type_name, &a.topic).cmp(&(b.kind, &b.type_name, &b.topic))
        });
    }

    /// Every rule the wire format has, checked in one place.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema_version != crate::SCHEMA_VERSION {
            return Err(format!(
                "sizing descriptor schema_version {} -- this reader understands {}. \
                 Re-run `nros sync` to regenerate it.",
                self.schema_version,
                crate::SCHEMA_VERSION
            ));
        }
        self.meta.validate()?;
        self.target.validate()?;
        for ep in &self.endpoints {
            ep.validate()?;
        }
        self.types.validate()?;
        self.policy.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod slot_table_tests {
    use super::*;

    /// `RX_BUF` and the image-wide subscribed class, as the reference island
    /// measures them: 1,496 over the whole link closure, 880 over the types it
    /// subscribes to. The 616-byte difference is what issue 1319 is about.
    const CLOSURE: usize = 1496;
    const SUBSCRIBED_CLASS: usize = 880;

    fn sub(path: Option<RegistrationPath>, bound: Option<usize>) -> Endpoint {
        let mut ep = Endpoint::new(
            EndpointKind::Subscription,
            "std_msgs/msg/String",
            "/chatter",
        );
        ep.set_registration_path(path).set_wire_bound_bytes(bound);
        ep
    }

    /// The whole of issue 1319, as a table. Two rows take the type's bound and
    /// two take the closure buffer; nothing else decides it.
    #[test]
    fn each_registration_path_claims_what_issue_1319_measured() {
        let bound = 700;
        for (path, expected) in [
            (RegistrationPath::CTypedHint, bound),
            (RegistrationPath::RustTypedDescriptors, bound),
            (RegistrationPath::RustTypedSchemaless, CLOSURE),
            (RegistrationPath::CRawNoHint, CLOSURE),
        ] {
            let ep = sub(Some(path), Some(bound));
            assert_eq!(
                ep.claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS).stated(),
                Some(&expected),
                "{} claims the wrong slot",
                path.tag()
            );
            assert_eq!(
                ep.may_claim_closure_buffer(),
                expected == CLOSURE,
                "{} disagrees with its own slot size",
                path.tag()
            );
        }
    }

    /// The gap, priced. A schemaless row and a descriptor-carrying row over the
    /// SAME type differ by exactly what the issue measured, and the model used
    /// to charge both the smaller one.
    #[test]
    fn the_two_schemaless_rows_are_the_measured_gap_above_the_type_bound() {
        let typed = sub(
            Some(RegistrationPath::RustTypedDescriptors),
            Some(SUBSCRIBED_CLASS),
        );
        let schemaless = sub(
            Some(RegistrationPath::RustTypedSchemaless),
            Some(SUBSCRIBED_CLASS),
        );
        let typed = typed.claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS).get();
        let schemaless = schemaless
            .claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS)
            .get();
        assert_eq!(typed, Some(SUBSCRIBED_CLASS));
        assert_eq!(schemaless, Some(CLOSURE));
        // 3 slots at depth 1 -> 1,848 bytes per subscription, which is the
        // number issue 1319 reports.
        assert_eq!(3 * (schemaless.unwrap() - typed.unwrap()), 1848);
    }

    /// A row whose type the bound inventory could not price keeps the
    /// image-wide subscribed class -- the same "absence is not zero" rule
    /// `nros-node/build.rs` applies to `NROS_SUBSCRIBED_TYPE_BOUNDS`.
    #[test]
    fn an_unpriced_type_keeps_the_image_wide_class_rather_than_zero() {
        let mut ep = sub(Some(RegistrationPath::CTypedHint), None);
        ep.refuse("wire_bound_bytes", "`demo_msgs/msg/Mystery` was not priced");
        assert_eq!(
            ep.claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS).stated(),
            Some(&SUBSCRIBED_CLASS)
        );
    }

    /// The negative control for the whole fix: a path nobody could state yields
    /// NO number, so the caller has to choose the direction in the open.
    #[test]
    fn a_refused_path_yields_no_number_and_carries_its_prose() {
        let mut ep = sub(None, Some(700));
        ep.refuse(
            "registration_path",
            "cannot tell which registration path this endpoint takes -- issue 1319",
        );
        let slot = ep.claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS);
        assert!(slot.stated().is_none());
        assert!(slot.refusal().unwrap().contains("1319"));
        assert!(ep.may_claim_closure_buffer());
        // And the loud fallback a build script writes is the SAFE direction.
        let (v, why) = slot.or_report("endpoint.registration_path", CLOSURE);
        assert_eq!(v, CLOSURE);
        assert!(why.unwrap().contains("1319"));
    }

    /// An absent path is not a refused one, and neither is a number.
    #[test]
    fn an_absent_path_is_distinguishable_from_a_refused_one() {
        let ep = sub(None, Some(700));
        let slot = ep.claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS);
        assert_eq!(slot.tag(), "absent");
        assert!(slot.refusal().is_none());
        assert!(ep.may_claim_closure_buffer());
    }

    /// A publisher claims no receive slot at all, so there is nothing here to
    /// refuse -- and nothing for a consumer to charge it for.
    #[test]
    fn a_kind_that_receives_no_topic_sample_claims_no_slot() {
        let mut ep = Endpoint::new(EndpointKind::Publisher, "std_msgs/msg/String", "/chatter");
        ep.set_registration_path(Some(RegistrationPath::RustTypedSchemaless))
            .set_wire_bound_bytes(Some(700));
        assert_eq!(
            ep.claimed_slot_bytes(CLOSURE, SUBSCRIBED_CLASS).tag(),
            "absent"
        );
        assert!(!ep.may_claim_closure_buffer());
    }
}

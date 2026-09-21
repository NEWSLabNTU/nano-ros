//! phase-454 W4 (RFC-0100 D4) — building the one sizing descriptor.
//!
//! The SCHEMA and the reader live in [`nros_sizing_descriptor`], because every
//! consumer RFC-0100 D5 names has to read it and most of them are build scripts
//! that cannot depend on this crate. What lives HERE is the producer: the join
//! that turns an image's three existing inventories into the rows that file
//! carries.
//!
//! ```text
//!   entity inventory  --(kind, type, topic, qos)-.
//!   bound inventory   --(type -> wire bound)------+--> build/nros/sizing/<entry>.toml
//!   board descriptor  --(pointer, align, heap)---'
//! ```
//!
//! Nothing is re-derived. `EntityInventory` already counts entities and resolves
//! all four QoS policies (phase-454 W1–W3); `rosidl_codegen::bounds` already
//! prices types and refuses per type; the board descriptor already states the
//! rustc triple and the RFC-0049 memory rung. This module joins them and writes
//! the result down ONCE, which is the whole of D4:
//!
//! > `nros sync` writes one file per entry; consumers read it by path, never by
//! > environment.
//!
//! # The three rules this producer must not break
//!
//! **Refusal is per FIELD** (D6). A `keep_all` endpoint loses `depth` and the
//! `storage_bytes` that depends on it; its `history`, its `wire_bound_bytes` and
//! the image's `[types]` are untouched. That is why the refusals go on the ROW
//! rather than on the descriptor.
//!
//! **`[target]` comes from the BOARD** (D1). Build scripts run for the host
//! (phase-118-E), so the pointer width a `size_of` sees there is the host's. The
//! board states a rustc triple; [`target_facts`] reads the width off that, and
//! REFUSES a triple it does not know rather than guessing — a wrong pointer
//! width under-sizes a ring's length array, which is the direction that ships
//! `BufferTooSmall`.
//!
//! **Demand is UNFLOORED** (D7, issues 1015 + 1033). There is no
//! `c_array_pool_floor` in this file and there must not be one: the same
//! derivation feeds consumers with opposite right answers at zero, and 1015's
//! floor in a shared derivation silently defeated 1033's fix with every knob gate
//! green.
//!
//! # What this wave does NOT fill
//!
//! `[policy]` is empty, and that is the correct W4 answer rather than an
//! omission. D1: *"policy … who can answer: nobody — must be stated"*. No image
//! states a burst depth, a graph size or an MTU today; the first consumers that
//! want one (W6.a's `SUBSCRIBER_RING_DEPTH`, W6.b's XRCE stream history) bring
//! the rung that states it. Writing a derived number into `[policy]` now would be
//! the exact category error D1 exists to prevent.
//!
//! `[types]`'s `max_fields` / `max_kinds` / `max_nested_depth` were REFUSED here
//! through W4, with a reason naming W6.c as the wave that would reach them.
//! **phase-454 W6.c filled them**: codegen's own schema walk now records a
//! per-type shape into `nros_message_bounds.json`, and [`type_facts`] takes the
//! maximum over the image's types. The refusal survives for the case it was
//! always for — a type whose schema codegen could not build — and now names that
//! type instead of a wave.
//!
//! # Two producers, one composer (phase-454 W14)
//!
//! [`write_for_leaf`] is the LEAF road: a single-package cargo leaf, which has
//! all three inventories. [`write_for_model`] is the second, and it has ONE —
//! the resolved SystemModel. A workspace cargo image and every cmake / Zephyr
//! west / NuttX entry reach a model and nothing else, so through W12 they got no
//! descriptor at all and every D5 derivation was inert there.
//!
//! What a model-only descriptor may CLAIM is the decision W11 left open, and it
//! is settled by [`ModelHorizon`]: **emit what the SystemModel knows and REFUSE
//! every field it cannot source, naming the follow-up in each reason.** Nothing
//! is invented and nothing falls back — D6 is what makes a partial descriptor
//! safe to publish at all, because [`nros_sizing_descriptor::Fact::stated`] is
//! the only accessor that yields a value, so a consumer cannot read a refusal as
//! a default.
//!
//! The five fields it refuses, and why each is leaf-side:
//!
//! | field | needs |
//! | --- | --- |
//! | `wire_bound_bytes` | the bound inventory, which codegen writes beside a LEAF |
//! | `storage_bytes` | that bound, plus the board descriptor resolved for this image |
//! | `[types] max_fields` / `max_kinds` / `max_nested_depth` | codegen's own per-type schema walk |
//! | `registration_path` | which subscribe spelling the image writes |
//!
//! Every one of those reasons names [`MODEL_ONLY_ISSUE`], so the artifact itself
//! says what is missing and why — and so the day that issue closes, the
//! refusals in a written descriptor are the checklist.

use nros_sizing_descriptor::{
    Basis, CapacityNeed, Durability, Endpoint, EndpointKind, History, Params, RegistrationPath,
    Reliability, SizingDescriptor, Status, Target, Types,
};
use rosidl_codegen::bounds::BoundState;

use crate::entity_inventory::{
    Declaration, EntityInventory, EntityKind, ParamCapacity, ParamDeclarations, ParamServiceShape,
};

/// Which of issue 1319's registration paths an entry's Rust or C code takes.
///
/// The language half of the answer. The backend half is [`BackendSchema`]; the
/// two together select the path, and neither alone can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryLanguage {
    Rust,
    /// C and C++ share a path here: both register through the same typed hint
    /// (`nros::rx_size_bound<M>` / `rx_buffer_hint`), and both fall to the raw
    /// no-hint row when they do not supply one.
    CFamily,
}

/// Does the linked backend carry type descriptors?
///
/// Cyclone does, and `default_subscription_rx_bytes` can therefore reach a
/// type's bound at a type-erased site. zenoh and XRCE do not, and the arm that
/// serves them `returns None for every type — correctly, because MessageForRmw
/// carries no schema there` (issue 1319), so the registration takes `RX_BUF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendSchema {
    Descriptors,
    Schemaless,
}

/// Does the backend dispatch a received sample IN PLACE?
///
/// **The axis issue 1319's four-row table did not have, and phase-454 W5
/// measured.** `register_subscription_buffered_on` asks
/// `handle.supports_process_in_place()` BEFORE it computes a slot size, and
/// returns through `SubInplaceEntry` when the answer is yes — so a Rust typed
/// subscription on such a backend allocates no receive region at all, whatever
/// its type's bound or the image's `RX_BUF` say. Measured on
/// `contract-monitor-sub` over zenoh: 672 bytes of arena for the whole
/// registration.
///
/// It is a property of the BACKEND and not of the entry, which is why it sits
/// beside [`BackendSchema`] rather than inside [`EntryLanguage`]: both of the
/// schemaless backends this tree ships answer an unconditional `true`
/// (`nros-rmw-zenoh`'s `supports_process_in_place`, and XRCE's
/// `xrce_subscription_supports_in_place`), while Cyclone leaves both vtable
/// slots NULL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendDispatch {
    /// The sample is handed to the callback out of the backend's own ring.
    InPlace,
    /// The runtime copies into an arena receive region it must budget for.
    Buffered,
}

/// The follow-up every model-only refusal names (phase-454 W14).
///
/// ONE spelling, so the day it closes the refusals in a written descriptor are
/// the checklist and a grep over this constant is the work list. Written into
/// the artifact rather than only into a build log, because a log nobody kept is
/// exactly the place RFC-0100 D6 says a refusal must not go.
pub const MODEL_ONLY_ISSUE: &str = "issue 1393";

/// What a producer that has ONLY the resolved SystemModel cannot source.
///
/// [`write_for_leaf`] has three inventories; [`write_for_model`] has one. The
/// difference is not a degree of completeness, it is a set of named inputs that
/// live beside a LEAF — codegen's bound table, codegen's per-type schema walk,
/// and the call sites that decide a subscription's registration spelling.
///
/// Carrying it as a value rather than a `bool` is what lets a refusal say WHICH
/// road it was written on: "a workspace cargo image" and "a cmake entry" want
/// different remedies, and a reader holding the file has no other way to tell
/// which producer wrote it.
#[derive(Debug, Clone)]
pub struct ModelHorizon {
    road: String,
}

impl ModelHorizon {
    /// `road` names the producer in prose — "a workspace cargo image",
    /// "a cmake entry". It appears verbatim in every refusal.
    pub fn new(road: impl Into<String>) -> Self {
        Self { road: road.into() }
    }

    /// The `bounds_error` a model-only producer supplies.
    ///
    /// It reaches TWO families through the code that already exists:
    /// `wire_bound_bytes` on every row, and `[types]`'s three maxima. Neither
    /// needs a special case here, because "there is no bound inventory" is
    /// exactly what both of those refusals are already written to say — the
    /// only thing this adds is WHY there is none, and what tracks fixing it.
    pub fn bound_inventory(&self) -> String {
        format!(
            "this descriptor was written from the resolved SystemModel alone ({road}), which \
             carries no message-bound inventory -- codegen writes one beside a LEAF, and the \
             per-type schema walk that prices it with it. Tracked by {MODEL_ONLY_ISSUE}",
            road = self.road
        )
    }

    /// `registration_path`'s refusal.
    ///
    /// Not composed from [`registration_path_refusal`]'s missing halves,
    /// because on this road the halves are not what is missing: an image
    /// resolved from a model is a set of nodes from several packages, so "the
    /// entry's language" has no single answer even where the backend does.
    /// Saying "the language is unknown" would aim the reader at a fact nobody
    /// can supply.
    pub fn registration_path(&self) -> String {
        format!(
            "this descriptor was written from the resolved SystemModel alone ({road}), which \
             does not say which subscribe spelling each node writes -- and a model image is \
             several packages, so there is no one entry language to read it off. A Rust typed \
             registration on a schemaless backend claims the closure buffer rather than the \
             type's bound (issue 1319), so the path is refused rather than assumed. Tracked by \
             {MODEL_ONLY_ISSUE}",
            road = self.road
        )
    }

    /// `storage_bytes`'s refusal.
    ///
    /// Refused DIRECTLY rather than through [`set_storage_bytes`]'s chain,
    /// because on this road the chain's first test is the wrong diagnosis: a
    /// `[target]` this producer CAN state would leave the reader with
    /// "the type's wire bound is not available" and no road back to why. The
    /// region needs both halves and this producer has neither.
    pub fn storage_bytes(&self) -> String {
        format!(
            "this descriptor was written from the resolved SystemModel alone ({road}), so a \
             receive region has neither of its two sizes: the type's wire bound (no message-bound \
             inventory) nor the board descriptor resolved for THIS image, whose pointer width \
             sizes the per-slot length word. Tracked by {MODEL_ONLY_ISSUE}",
            road = self.road
        )
    }
}

/// Everything the producer needs, stated by the caller that HAS it.
///
/// A struct rather than nine arguments because every one of these is
/// independently refusable and a caller must be able to say "I do not know this"
/// without reordering a call.
#[derive(Debug, Default)]
pub struct DescriptorInputs<'a> {
    /// The entry this descriptor is for. Becomes the file stem and `[meta] entry`.
    pub entry: String,
    /// What the image declares. `None` when the inventory refused or nothing was
    /// probed — the descriptor is then `status = "refused"` with no endpoint rows,
    /// which is a different statement from "this image has no endpoints".
    pub inventory: Option<&'a EntityInventory>,
    /// `pkg/msg/Name -> bound`, from the leaf's `generated/` trees.
    pub bounds: Vec<(String, BoundState)>,
    /// phase-454 W6.c — `pkg/msg/Name -> CycloneDDS schema shape`, from the SAME
    /// rows as [`Self::bounds`].
    ///
    /// The inner `Option` is the type's own answer (`None` = codegen could not
    /// build that schema, or the tree predates the field); a type MISSING from
    /// this table is the table-level miss `bounds` has too, and the two are
    /// distinguished the same way — by a lookup returning `None` rather than by a
    /// sentinel.
    pub schema_shapes: Vec<(String, Option<rosidl_codegen::schema_value::SchemaShape>)>,
    /// Why there is no bound table, when there is none.
    pub bounds_error: Option<String>,
    /// The rustc target triple the board pins. `None` for a board that pins none
    /// (a host build), where the HOST is the target and the width is this
    /// process's own — which is the one case a host answer is the right answer.
    pub target_triple: Option<String>,
    /// Is the target the host? Decides whether an absent triple is an answer.
    pub host_build: bool,
    /// `[board.knobs.memory] heap_bytes`, the RFC-0049 board rung.
    pub heap_budget_bytes: Option<usize>,
    /// The entry's language, for the registration path.
    pub language: Option<EntryLanguage>,
    /// phase-454 (issue 1408) — what the contract says about this image's
    /// PARAMETERS, for `[params]`.
    ///
    /// A field of its own rather than a read off [`Self::inventory`], because
    /// the two arrive by different roads and only one road has both. The leaf
    /// road's inventory is PROBED out of the leaf's own metadata and carries no
    /// parameter declarations at all; its parameters come from the resolved
    /// SystemModel beside it. The model road's inventory carries them already,
    /// having been composed from the same model. Reading
    /// `inventory.param_declarations()` here would therefore answer
    /// [`ParamDeclarations::Absent`] on the leaf road for a leaf whose contract
    /// declares parameters — "nobody said" for a statement that was made, which
    /// is the one mistake this whole schema exists to prevent.
    ///
    /// `None` is the same statement as `Some(&ParamDeclarations::Absent)` and
    /// is what a caller with no model at all passes; both leave `[params]`
    /// empty.
    pub params: Option<&'a ParamDeclarations>,
    /// Whether the linked backend carries type descriptors.
    pub backend_schema: Option<BackendSchema>,
    /// Whether the linked backend dispatches in place (phase-454 W5). `None`
    /// for a backend this writer does not know — refused with the schema half,
    /// because the two together select the path and neither alone can.
    pub backend_dispatch: Option<BackendDispatch>,
    /// The backend's name, for prose only.
    pub rmw: Option<String>,
    /// phase-454 W14 — this producer's HORIZON, when it is narrower than the
    /// leaf road's. `None` on the leaf road, which has every input.
    ///
    /// `Some` switches two fields from "composed from what I was given" to
    /// "refused, and here is the road and the tracked follow-up". It does NOT
    /// switch off anything the SystemModel does answer: the counts and all four
    /// QoS policies are stated exactly as they are on the leaf road, because
    /// they come from the same [`EntityInventory`].
    pub horizon: Option<ModelHorizon>,
}

/// Build the descriptor.
pub fn build(inputs: &DescriptorInputs<'_>) -> SizingDescriptor {
    let mut desc = SizingDescriptor::new(&inputs.entry, Status::Derived, Basis::Contract);
    desc.target = target_facts(inputs);

    match inputs.inventory {
        None => {
            // D6: a refusal names what it refused and never widens the basis.
            // `Closure` here is not a fallback to a wider set of rows -- there
            // are no rows. It is the honest statement that whatever a consumer
            // sizes now describes the link closure, because nothing described
            // the image.
            desc.meta.status = Status::Refused;
            desc.meta.basis = Basis::Closure;
            desc.meta.refuse(
                "undeclared_endpoints",
                "the entity inventory did not compose for this entry, so how many endpoints \
                 stayed silent is not known -- absence is not zero",
            );
        }
        Some(inv) => {
            let rows = endpoint_rows(inv, inputs, &desc.target);
            desc.meta
                .set_undeclared_endpoints(Some(count_undeclared(&rows)));
            desc.endpoints = rows;
            desc.meta.status = overall_status(&desc);
        }
    }

    desc.image = image_facts(inputs);
    desc.types = type_facts(inputs, &desc.endpoints);
    // phase-454 (issue 1408) — `[params]`. On the SHARED composer, so both
    // producers fill it from the one derivation: a second `param_facts` beside
    // `write_for_model` is exactly how two producers of one schema come to
    // disagree, which is issue 1025 one artifact over.
    //
    // Deliberately NOT folded into `overall_status` — see `overall_status`.
    desc.params = param_facts(inputs);
    // `[policy]` stays empty. See the module docs: a policy fact is STATED, and
    // nothing states one yet.
    desc.sort_endpoints();
    desc
}

/// `[target]` — from the board, never from this process.
fn target_facts(inputs: &DescriptorInputs<'_>) -> Target {
    let mut t = Target::default();
    match (inputs.target_triple.as_deref(), inputs.host_build) {
        (Some(triple), _) => match abi_for_triple(triple) {
            Some((pointer, align)) => {
                t = Target::new(Some(pointer), Some(align), inputs.heap_budget_bytes);
            }
            None => {
                let reason = format!(
                    "the board pins rustc target `{triple}`, whose ABI this writer does not \
                     model -- add it to `abi_for_triple`. A guessed pointer width under-sizes \
                     a ring's length array, so it is refused rather than assumed"
                );
                t.refuse("pointer_bytes", reason.clone());
                t.refuse("max_align", reason);
            }
        },
        // No triple and a HOST build: the target IS this process, so this is the
        // one case where reading the width here answers the right question.
        (None, true) => {
            t = Target::new(
                Some(std::mem::size_of::<usize>()),
                Some(HOST_MAX_ALIGN),
                inputs.heap_budget_bytes,
            );
        }
        (None, false) => {
            let reason = "the board pins no rustc target and this is not a host build, so the \
                          target ABI is unknown. `[target]` must come from the board (RFC-0100 \
                          D1) -- a build script's own `size_of` answers for the HOST \
                          (phase-118-E)"
                .to_string();
            t.refuse("pointer_bytes", reason.clone());
            t.refuse("max_align", reason);
        }
    }
    if inputs.heap_budget_bytes.is_none() {
        t.refuse(
            "heap_budget_bytes",
            "the board states no `[board.knobs.memory] heap_bytes`, so there is no budget to \
             assert against (RFC-0100 D11)",
        );
    }
    t
}

/// The largest fundamental alignment a pool must satisfy.
///
/// `align_of::<u64>()` on every target this tree builds for, 32-bit ARM and
/// RISC-V included. Named rather than spelled inline so the one assumption in
/// [`abi_for_triple`] has somewhere to be argued with.
const HOST_MAX_ALIGN: usize = 8;

/// `(pointer_bytes, max_align)` for a rustc target triple.
///
/// Keyed on the ARCHITECTURE, which is the first dash-separated component and
/// the only part that decides either number. `None` for an architecture this
/// writer has not been told about — a refusal, per the module docs, because a
/// guessed width is an under-size in the direction that ships `BufferTooSmall`.
///
/// This is not `CARGO_CFG_TARGET_POINTER_WIDTH`: that variable exists only
/// inside a build script compiled for the target, which is exactly the place
/// RFC-0100 D1 says the answer must not come from.
fn abi_for_triple(triple: &str) -> Option<(usize, usize)> {
    let arch = triple.split('-').next()?;
    let pointer = match arch {
        a if a.starts_with("thumbv") => 4,
        a if a.starts_with("armv") || a == "arm" => 4,
        a if a.starts_with("riscv32") => 4,
        "i586" | "i686" | "x86" => 4,
        "xtensa" => 4,
        "x86_64" => 8,
        "aarch64" => 8,
        a if a.starts_with("riscv64") => 8,
        _ => return None,
    };
    // 8 on every one of them: `u64` and `f64` take 8-byte alignment on ARM,
    // RISC-V and x86_64 alike. i686 is the classic exception (4-byte `u64`
    // alignment in the SysV i386 ABI) and over-stating it there costs padding,
    // never correctness.
    Some((pointer, HOST_MAX_ALIGN))
}

/// One row per endpoint the image declares.
fn endpoint_rows(
    inv: &EntityInventory,
    inputs: &DescriptorInputs<'_>,
    target: &Target,
) -> Vec<Endpoint> {
    let mut rows = Vec::new();
    for comp in inv.components() {
        let Declaration::Stated(decls) = &comp.declaration else {
            continue;
        };
        for d in decls {
            let Some(kind) = endpoint_kind(d.kind) else {
                continue;
            };
            // A row with no type or no topic cannot be joined by anything --
            // both inventories key on `(kind, type, topic)`. Skipping it would
            // be an under-report, so it is counted as undeclared instead, by
            // being absent from a table whose `undeclared_endpoints` says so.
            let (Some(ty), Some(topic)) = (d.type_name.as_deref(), d.name.as_deref()) else {
                continue;
            };
            rows.push(endpoint_row(kind, ty, topic, d, inputs, target));
        }
    }
    rows
}

fn endpoint_kind(k: EntityKind) -> Option<EndpointKind> {
    Some(match k {
        EntityKind::Publisher => EndpointKind::Publisher,
        EntityKind::Subscription => EndpointKind::Subscription,
        EntityKind::ServiceServer => EndpointKind::ServiceServer,
        EntityKind::ServiceClient => EndpointKind::ServiceClient,
        EntityKind::ActionServer => EndpointKind::ActionServer,
        EntityKind::ActionClient => EndpointKind::ActionClient,
        // Not endpoints: they carry no type and no topic, so they have no
        // identity this table can key on. `EntityKind::carries_qos_depth`
        // already draws exactly this line.
        EntityKind::Timer | EntityKind::GuardCondition => return None,
    })
}

/// How many cache queryables an image's DECLARED ENTITIES imply — issue 1378.
///
/// **This is not a second rule.** It is
/// [`nros_sizing_descriptor::transient_local_publishers`]'s rule, fed from the
/// other kind of row: a `[[component]] entities = [...]` declaration rather
/// than a descriptor's `[[endpoint]]` table. The two mappings it goes through
/// ([`endpoint_kind`], [`map_durability`]) are the ones the descriptor writer
/// already uses, so a kind or a durability spelling cannot mean one thing here
/// and another there.
///
/// It exists because the road that FAILED has no descriptor to read. Measured
/// 2026-09-20 on `examples/qemu-armv7a-nuttx/c/action-server`, the image issue
/// 1378 was filed against: `nros ws entity-facts --leaf` answered
/// `NROS_DECLARED_SERVICE_SERVERS=3` / `INFRA_QUERYABLES=none`, the zenoh
/// backend sized `ZPICO_MAX_QUERYABLES=3` from exactly that, and the action
/// server's own `/status` cache queryable was the FOURTH declaration — `Full`,
/// at boot, with `nros_executor_add_action_server` returning -1. The cargo-leaf
/// road never saw it because `nros sync` writes that road a descriptor, whose
/// `action_server` row phase-455 W5 already counts.
///
/// A row with no type or no topic still counts: the rule reads only `kind` and
/// `durability`, and the two names are for the refusal prose. Skipping such a
/// row would be an under-report, which is the one direction this number must
/// never go.
pub fn transient_local_publishers_from_decls(
    decls: &[crate::entity_inventory::EntityDecl],
) -> nros_sizing_descriptor::Fact<usize> {
    let answer =
        nros_sizing_descriptor::transient_local_publishers_over(decls.iter().filter_map(|d| {
            endpoint_kind(d.kind).map(|kind| nros_sizing_descriptor::TlRow {
                kind,
                durability: match d.durability.and_then(map_durability) {
                    Some(v) => nros_sizing_descriptor::Fact::Stated(v),
                    None => nros_sizing_descriptor::Fact::Absent,
                },
                topic: d.name.as_deref().unwrap_or("<unnamed>"),
                type_name: d.type_name.as_deref().unwrap_or("<untyped>"),
            })
        }));
    // WHETHER A DECLARATION EXISTS is this adapter's question; WHAT IT IMPLIES
    // is the rule's. They come apart for exactly one input: a component that
    // declares only timers and guard conditions. `endpoint_kind` drops those
    // (they carry no topic and no type, so no endpoint table can key on them),
    // which leaves the rule with no rows and makes it answer `Absent` — "nobody
    // said". Nobody did say: such an image declared its whole surface and none
    // of it is a publisher, so the honest answer is ZERO, and reporting it as a
    // refusal would make a consumer warn about a number it has.
    match answer {
        nros_sizing_descriptor::Fact::Absent if !decls.is_empty() => {
            nros_sizing_descriptor::Fact::Stated(0)
        }
        other => other,
    }
}

fn endpoint_row(
    kind: EndpointKind,
    ty: &str,
    topic: &str,
    d: &crate::entity_inventory::EntityDecl,
    inputs: &DescriptorInputs<'_>,
    target: &Target,
) -> Endpoint {
    let mut ep = Endpoint::new(kind, ty, topic);

    // phase-454 W12 -- did the contract join attribute this row?
    //
    // `Some(why)` means a contract WAS authored for this image and this row
    // could not be matched to one of its endpoints WITH CERTAINTY. All four QoS
    // facts then refuse TOGETHER, because they come from one attribution and
    // half an attribution is the mis-key the refusal exists to prevent:
    // publishing a reliability against one endpoint and a depth against another
    // describes no image at all.
    //
    // REFUSED and not ABSENT, deliberately. `Absent` is "nobody said", which is
    // what a leaf with no contract gets and why such a leaf is byte-identical;
    // this reader LOOKED, so the fact carries its prose to the consumer that
    // would otherwise have defaulted silently (RFC-0100 D6). Either way the row
    // still counts toward `undeclared_endpoints`, so every per-endpoint
    // consumer keeps its worst case.
    let unattributed = d.contract_refusal.as_deref();
    let history = if unattributed.is_some() {
        None
    } else {
        d.history.and_then(map_history)
    };
    if let Some(why) = unattributed {
        for field in ["depth", "history", "reliability", "durability"] {
            ep.refuse(field, why.to_string());
        }
    } else {
        ep.set_history(history);
        ep.set_reliability(d.reliability.and_then(map_reliability));
        ep.set_durability(d.durability.and_then(map_durability));
        // `keep_all` kills the depth-derived fields and NOTHING else --
        // RFC-0100 D6, the one trigger that ships a too-small buffer rather than
        // a too-large one. phase-454 W3 implements the same refusal one level
        // up, for the whole depth table; carrying it per row is what lets the
        // rest of this image size.
        if history == Some(History::KeepAll) {
            ep.refuse(
                "depth",
                format!(
                    "history = keep_all on {} {topic}: a KEEP_ALL queue has no static bound, so \
                     a depth beside it prices nothing (RFC-0100 D6)",
                    kind.tag()
                ),
            );
        } else {
            ep.set_depth(d.depth);
        }
    }

    // Everything below is a property of the TYPE and of the IMAGE, not of the
    // attribution, so it survives a refusal above -- D6: a refusal never
    // degrades another consumer's facts.
    let bound = set_wire_bound(&mut ep, ty, inputs);

    // phase-454 W14 — a model-only producer refuses the path OUTRIGHT, with its
    // own reason. It does not fall through the composer below: that one reports
    // which HALF it is missing, and on this road neither half is the answer.
    match &inputs.horizon {
        Some(h) => {
            ep.refuse("registration_path", h.registration_path());
        }
        None => {
            ep.set_registration_path(registration_path(kind, inputs));
            if ep.registration_path().stated().is_none() {
                ep.refuse("registration_path", registration_path_refusal(inputs));
            }
        }
    }

    // Only a subscription claims a topic-sample receive region. Every other kind
    // leaves `storage_bytes` ABSENT, which is the accurate statement: nothing
    // refused it, there is simply no such region. A publisher serializes into a
    // per-call stack array, which is a transmit buffer and a different question.
    if kind.receives_topic_sample() {
        if let Some(h) = &inputs.horizon {
            // phase-454 W14 — same shape as the path above, and the same
            // reason: the chain in `set_storage_bytes` would report whichever
            // of its three inputs it tested first, which on this road is not
            // the diagnosis a reader needs.
            ep.refuse("storage_bytes", h.storage_bytes());
        } else if unattributed.is_some() {
            ep.refuse(
                "storage_bytes",
                "`depth` is refused (this endpoint was not attributed to a contract \
                 declaration), and a receive region is sized from it",
            );
        } else {
            set_storage_bytes(&mut ep, bound, target, history == Some(History::KeepAll));
        }
    }
    ep
}

/// The wire bound, joined from the bound inventory by the same `pkg/msg/Name`
/// spelling both tables use. Returns it, because the receive region is sized
/// from it.
///
/// A FUNCTION rather than an inline block because phase-454 W12 gave
/// [`endpoint_row`] a second exit: a row the contract could not be attributed to
/// still has a type, so it still has a bound, and refusing the QoS must not
/// refuse the payload class beside it (RFC-0100 D6 — a refusal never degrades
/// another consumer's facts).
fn set_wire_bound(ep: &mut Endpoint, ty: &str, inputs: &DescriptorInputs<'_>) -> Option<usize> {
    let bound = match lookup_bound(&inputs.bounds, ty) {
        Some(BoundState::Bounded { rx, .. }) => Some(*rx),
        Some(BoundState::Unbounded { reason }) => {
            ep.refuse(
                "wire_bound_bytes",
                format!("`{ty}` has no static bound: {reason}"),
            );
            None
        }
        Some(BoundState::Unresolved { reason }) => {
            ep.refuse(
                "wire_bound_bytes",
                format!("`{ty}` was not priced: {reason}"),
            );
            None
        }
        None => {
            ep.refuse(
                "wire_bound_bytes",
                match &inputs.bounds_error {
                    Some(e) => format!("no bound inventory for this entry: {e}"),
                    None => format!(
                        "`{ty}` is in no `nros_message_bounds.json` beside this entry -- run \
                         `nros sync` so codegen prices it"
                    ),
                },
            );
            None
        }
    };
    ep.set_wire_bound_bytes(bound);
    bound
}

/// The arena region one buffered subscription claims, in TARGET bytes.
///
/// MIRROR of `executor::arena::buffered_region_size`, the function the allocator
/// itself calls, and of `nros-node/build.rs`'s `buffered_region` which prices it
/// today. The difference -- and the reason this belongs in the descriptor -- is
/// the length word: that build script spells it `RING_LEN_BYTES = 8` with its own
/// comment saying *"taken at its 64-bit width. A 32-bit target spends 4, so this
/// over-states there"*. The board knows which.
fn set_storage_bytes(ep: &mut Endpoint, bound: Option<usize>, target: &Target, keep_all: bool) {
    let depth = ep.depth().get();
    let Some(pointer) = target.pointer_bytes().get() else {
        ep.refuse(
            "storage_bytes",
            format!(
                "`[target] pointer_bytes` is not available ({}), and a receive region's \
                 per-slot length word is target-ABI-sized (RFC-0100 D6)",
                target
                    .pointer_bytes()
                    .refusal()
                    .unwrap_or("nothing stated it")
            ),
        );
        return;
    };
    let Some(bound) = bound else {
        ep.refuse(
            "storage_bytes",
            "the type's wire bound is not available, and a receive region is sized from it"
                .to_string(),
        );
        return;
    };
    let Some(depth) = depth else {
        ep.refuse(
            "storage_bytes",
            if keep_all {
                "`depth` is refused (history = keep_all), and a receive region is sized from it"
                    .to_string()
            } else {
                "no `depth` was declared for this endpoint, and a receive region is sized from \
                 it -- a default is wrong by up to 10x in either direction"
                    .to_string()
            },
        );
        return;
    };
    ep.set_storage_bytes(Some(buffered_region(depth as usize, bound, pointer)));
}

/// The three QoS vocabularies, mapped onto the descriptor's.
///
/// EXHAUSTIVE, and each returns `None` for `SystemDefault`, which is the whole
/// reason these are functions rather than a `_ =>` arm. `SystemDefault` means
/// **the caller did not state a policy — the backend picks**; folding it into
/// `KeepAll` would refuse an endpoint's depth over a policy nobody wrote, and
/// folding it into `KeepLast` would claim a statement that was never made.
/// `None` puts it where it belongs: `Fact::Absent`, nobody said.
fn map_history(h: nros_orchestration_ir::qos_override::QoSHistoryPolicy) -> Option<History> {
    use nros_orchestration_ir::qos_override::QoSHistoryPolicy as H;
    match h {
        H::SystemDefault => None,
        H::KeepLast => Some(History::KeepLast),
        H::KeepAll => Some(History::KeepAll),
    }
}

fn map_reliability(
    r: nros_orchestration_ir::qos_override::QoSReliabilityPolicy,
) -> Option<Reliability> {
    use nros_orchestration_ir::qos_override::QoSReliabilityPolicy as R;
    match r {
        R::SystemDefault => None,
        R::Reliable => Some(Reliability::Reliable),
        R::BestEffort => Some(Reliability::BestEffort),
    }
}

fn map_durability(
    v: nros_orchestration_ir::qos_override::QoSDurabilityPolicy,
) -> Option<Durability> {
    use nros_orchestration_ir::qos_override::QoSDurabilityPolicy as D;
    match v {
        D::SystemDefault => None,
        D::Volatile => Some(Durability::Volatile),
        D::TransientLocal => Some(Durability::TransientLocal),
    }
}

/// `TripleBuffer::SLOT_COUNT` -- the slot count a `KEEP_LAST(<=1)` history uses.
const TRIPLE_BUFFER_SLOTS: usize = 3;

fn buffered_region(depth: usize, slot: usize, pointer_bytes: usize) -> usize {
    if depth <= 1 {
        TRIPLE_BUFFER_SLOTS * slot
    } else {
        (depth + 1) * slot + (depth + 1) * pointer_bytes
    }
}

fn lookup_bound<'a>(bounds: &'a [(String, BoundState)], ty: &str) -> Option<&'a BoundState> {
    bounds.iter().find(|(n, _)| n == ty).map(|(_, b)| b)
}

/// Which of issue 1319's four paths this endpoint's registration takes.
///
/// An IMAGE fact, composed from two halves the build script cannot see: the
/// entry's LANGUAGE and whether the linked backend carries type descriptors.
/// `None` when either half is missing -- refused, not guessed, because the two
/// schemaless rows are 1,848 bytes per subscription in the UNDER direction.
fn registration_path(
    kind: EndpointKind,
    inputs: &DescriptorInputs<'_>,
) -> Option<RegistrationPath> {
    // Only a subscription's slot size turns on this today. The field is written
    // for every kind anyway: a service server's request buffer takes the same
    // four paths, and W6.b prices it.
    let _ = kind;
    match (
        inputs.language?,
        inputs.backend_schema?,
        inputs.backend_dispatch?,
    ) {
        // A C/C++ entry that registers typed supplies `rx_size_bound<M>`; the
        // raw no-hint row is a property of an individual call site, not of the
        // image, and nothing this writer reads distinguishes them. The typed
        // hint is therefore what a C/C++ entry is credited with, and W10 --
        // which closes the C half of the declared-QoS check -- is where a
        // per-call-site answer becomes available.
        //
        // The C path does NOT consult the in-place capability
        // (`add_arena_subscription_c_callback` allocates a region
        // unconditionally), so the dispatch axis does not reach this arm.
        (EntryLanguage::CFamily, _, _) => Some(RegistrationPath::CTypedHint),
        // phase-454 W5 -- in-place wins over the schema question, because the
        // capability test in `register_subscription_buffered_on` happens BEFORE
        // the slot size is computed. A descriptor-carrying backend that also
        // dispatched in place would take this row too; none does today.
        (EntryLanguage::Rust, _, BackendDispatch::InPlace) => {
            Some(RegistrationPath::RustTypedInPlace)
        }
        (EntryLanguage::Rust, BackendSchema::Descriptors, BackendDispatch::Buffered) => {
            Some(RegistrationPath::RustTypedDescriptors)
        }
        (EntryLanguage::Rust, BackendSchema::Schemaless, BackendDispatch::Buffered) => {
            Some(RegistrationPath::RustTypedSchemaless)
        }
    }
}

fn registration_path_refusal(inputs: &DescriptorInputs<'_>) -> String {
    let mut missing: Vec<String> = Vec::new();
    if inputs.language.is_none() {
        missing.push("the entry's language".into());
    }
    if inputs.backend_schema.is_none() {
        missing.push(match &inputs.rmw {
            Some(r) => format!("whether backend `{r}` carries type descriptors"),
            None => "which backend this image links".into(),
        });
    }
    if inputs.backend_dispatch.is_none() && inputs.backend_schema.is_some() {
        missing.push(match &inputs.rmw {
            Some(r) => format!("whether backend `{r}` dispatches in place"),
            None => "how this image's backend dispatches".into(),
        });
    }
    format!(
        "cannot tell which registration path this endpoint takes -- {} unknown. A Rust typed \
         registration on a schemaless backend claims the closure buffer rather than the type's \
         bound (issue 1319), so the path is refused rather than assumed",
        missing.join(" and ")
    )
}

/// `[image]` — the three counts no endpoint row carries (phase-454 W6.e).
///
/// Every number here is one the entity inventory ALREADY derived; nothing is
/// re-computed, which is the same rule the rest of this module holds to. Two of
/// them are simply thrown away today: `DerivedEntityKnobs::max_nodes` reaches the
/// executor's node table and never the cffi shim's, and no producer of any kind
/// states how many backends an image links.
///
/// UNFLOORED, D7. `max_subscribers` is zero for a pub-only image and that is the
/// answer — 1 KiB a slot in the cffi pool (issue 1033's measurement, one backend
/// over). Whether zero is a legal SIZE is decided at each pool that names a knob.
fn image_facts(inputs: &DescriptorInputs<'_>) -> nros_sizing_descriptor::Image {
    use crate::entity_inventory::Derivation;

    let mut img = nros_sizing_descriptor::Image::default();

    // The backend half is independent of the inventory: it comes from what the
    // image DECLARES it links, not from what it declares it publishes.
    match inputs.rmw.as_deref() {
        // A leaf names exactly ONE backend (`LeafSystem::rmw` is a single
        // value), and `nros build` generates exactly one `register()` call for
        // it. A bridge binds several -- and a bridge leaf carries no
        // `system.toml`, so no descriptor is written for one and this arm is
        // never the answer for a multi-backend image.
        Some(_) => {
            img = nros_sizing_descriptor::Image::new(None, Some(1), None);
        }
        None => {
            img.refuse(
                "backend_count",
                "the image names no rmw, so what it links is not known here -- a short backend \
                 registry is a registration FAILURE, so it is refused rather than guessed",
            );
        }
    }

    match inputs.inventory.map(EntityInventory::derive) {
        Some(Derivation::Derived(k)) => {
            img.set_node_count(Some(k.max_nodes))
                .set_subscriber_count(Some(k.max_subscribers));
        }
        Some(Derivation::Refused { reason }) => {
            img.refuse("node_count", reason.clone())
                .refuse("subscriber_count", reason);
        }
        None => {
            let why = "the entity inventory did not compose for this entry, so the image's \
                       node and subscriber counts are not known -- absence is not zero";
            img.refuse("node_count", why)
                .refuse("subscriber_count", why);
        }
    }
    img
}

/// `[params]` — the parameter store, from the contract (issue 1408, RFC-0100 D4).
///
/// The three-state [`ParamDeclarations`] maps onto the section exactly:
///
/// | declaration | `[params]` |
/// | --- | --- |
/// | `Absent` | EMPTY — every field [`nros_sizing_descriptor::Fact::Absent`], "nobody said" |
/// | `Refused` | every field REFUSED, carrying the derivation's own prose |
/// | `Declared` | every field STATED |
///
/// `Absent` writes nothing rather than refusing, and that is the difference the
/// whole schema turns on: an image whose contract says nothing about parameters
/// has not been looked at and failed, it has not been asked. A refusal there
/// would put a reason in front of every reader of every descriptor in the tree
/// in order to say that a feature nobody used was not used — and, worse, would
/// make a fully-derived descriptor carry a refused field.
///
/// **The refusal prose is the derivation's, not this function's.**
/// [`ParamDeclarations::Refused`] already names the silent nodes and says what
/// declaring would buy; restating it here would be a second author for one
/// diagnosis. Where a field is refused only BECAUSE another is — the three
/// capacity needs and the service shape follow from the declarations that
/// `Refused` withheld — the reason says so first, the way
/// [`set_storage_bytes`] does when `depth` is refused.
fn param_facts(inputs: &DescriptorInputs<'_>) -> Params {
    let mut p = Params::default();
    let Some(decl) = inputs.params else {
        return p;
    };
    match decl {
        // Nobody said. `Fact::Absent` on every accessor, which is what an
        // untouched `Params` already answers.
        ParamDeclarations::Absent => {}
        ParamDeclarations::Refused { reason } => {
            // The two counts the refusal gates DIRECTLY: they are sums and
            // maxima over the declarations, and `Refused` is precisely the
            // statement that the declaration set is a subset of the image.
            p.refuse("declared", reason.clone());
            p.refuse("max_parameters", reason.clone());
            p.refuse("max_param_name_len", reason.clone());
            // And the four that are refused only BECAUSE those are. A reader
            // who finds `needs_max_array_len` refused must not go looking for
            // a second, independent reason.
            let consequent = |what: &str| {
                format!(
                    "the contract's parameter declarations are refused, and {what} is derived \
                     from them. The refusal: {reason}"
                )
            };
            p.refuse(
                "needs_max_string_value_len",
                consequent("whether any declared parameter is a `string` or a `string_array`"),
            );
            p.refuse(
                "needs_max_array_len",
                consequent("whether any declared parameter is an array"),
            );
            p.refuse(
                "needs_max_byte_array_len",
                consequent("whether any declared parameter is a `byte_array`"),
            );
            p.refuse(
                "service_shape",
                consequent("each node's parameter-service shape"),
            );
        }
        ParamDeclarations::Declared { .. } => {
            // Both of these are `Some` for a `Declared`, by their own
            // contracts. `expect` rather than a silent skip: a `None` here
            // would be this module quietly publishing an empty `[params]` for
            // an image that declared, which is the under-report D6 forbids.
            let z = decl
                .sizing()
                .expect("a `Declared` contract sizes its store");
            let shapes = decl
                .service_shapes()
                .expect("a `Declared` contract shapes its parameter services");
            p.set_declared(Some(z.declared))
                .set_max_parameters(Some(z.max_parameters))
                .set_max_param_name_len(Some(z.max_param_name_len))
                .set_needs_max_string_value_len(Some(capacity_need(&z.string_value_len)))
                .set_needs_max_array_len(Some(capacity_need(&z.array_len)))
                .set_needs_max_byte_array_len(Some(capacity_need(&z.byte_array_len)))
                // The TOKEN, not a re-spelling. `nros-node/build.rs` parses
                // exactly this grammar and `ParamServiceShape::token` is its
                // one author; see `Params::service_shape`'s own doc for why
                // the descriptor carries the string rather than nine numbers.
                .set_service_shape(Some(ParamServiceShape::token(&shapes)));
        }
    }
    p
}

/// [`ParamCapacity`] (the producer's) onto [`CapacityNeed`] (the schema's).
///
/// Two vocabularies for one fact, and they stay two because the crates are on
/// opposite sides of the file: `ParamCapacity` carries a whole
/// [`crate::entity_inventory::DeclaredParam`] (node, name AND type), and the
/// descriptor carries only the two names — the type is the derivation's input,
/// not a fact a consumer of the file needs. `Unused` maps to `Unused`, which is
/// a STATEMENT on both sides and never an absence.
fn capacity_need(c: &ParamCapacity) -> CapacityNeed {
    match c {
        ParamCapacity::Unused => CapacityNeed::Unused,
        ParamCapacity::NeededBy(p) => CapacityNeed::NeededBy {
            node: p.node.clone(),
            name: p.name.clone(),
        },
    }
}

/// `[types]` — Cyclone's whole appetite (RFC-0100 D5).
///
/// phase-454 W6.c filled the three sub-fields W4 left REFUSED. They are the
/// MAXIMUM over the image's distinct types of a per-type shape codegen derives
/// from the same schema walk it prices the bound with
/// (`schema_value::schema_shape_for`), carried through
/// `nros_message_bounds.json`. Per field independently: the widest type and the
/// deepest type need not be the same type.
///
/// A type in the endpoint set whose shape is missing REFUSES all three — it does
/// not drop out of the maximum. A maximum over a subset is a smaller number that
/// reads exactly like the right one, which is the shape of every silent
/// under-size this campaign exists to remove (RFC-0100 D6).
fn type_facts(inputs: &DescriptorInputs<'_>, endpoints: &[Endpoint]) -> Types {
    let mut t = Types::default();
    let Some(_) = inputs.inventory else {
        let why = "the entity inventory did not compose, so the image's type set is not known";
        t.refuse("distinct_count", why);
        t.refuse("max_fields", why);
        t.refuse("max_kinds", why);
        t.refuse("max_nested_depth", why);
        return t;
    };

    let mut names: Vec<&str> = endpoints.iter().map(|e| e.type_name.as_str()).collect();
    names.sort_unstable();
    names.dedup();

    // UNFLOORED -- D7. An image with no endpoints declares zero distinct
    // types, and zero is the answer rather than a number to round up.
    let distinct_count = names.len();

    let mut shape = rosidl_codegen::schema_value::SchemaShape::default();
    let mut unshaped: Vec<&str> = Vec::new();
    for ty in &names {
        match inputs
            .schema_shapes
            .iter()
            .find(|(n, _)| n == ty)
            .and_then(|(_, s)| *s)
        {
            Some(s) => shape = shape.max(s),
            None => unshaped.push(ty),
        }
    }

    if unshaped.is_empty() {
        t = Types::new(
            Some(distinct_count),
            Some(shape.fields),
            Some(shape.kinds),
            Some(shape.nested_depth),
        );
    } else {
        t = Types::new(Some(distinct_count), None, None, None);
        // Name the types, not the count: the remedy differs per type (an
        // unreachable nested package, or a `generated/` tree older than the
        // field), and a bare "3 types" sends the reader looking for which.
        let why = match &inputs.bounds_error {
            Some(e) => format!(
                "no bound inventory for this entry, so no type's schema shape is known: {e}"
            ),
            None => format!(
                "no CycloneDDS schema shape recorded for {} -- either codegen could not \
                 resolve a nested type, or the `generated/` tree predates this field. Run \
                 `nros sync` so codegen walks them; a maximum over the types that DO have \
                 one would under-size the descriptor builder's stack arrays silently",
                unshaped.join(", ")
            ),
        };
        t.refuse("max_fields", why.clone());
        t.refuse("max_kinds", why.clone());
        t.refuse("max_nested_depth", why);
    }
    t
}

/// Endpoints that state no per-endpoint QoS fact at all.
///
/// `DeclaredDepths::undeclared`'s question, asked over the rows this table
/// carries. A consumer that needs a PER-POLICY count reads the rows -- the field
/// it wants is right there, and counting `Fact::Absent` over one column is the
/// advantage a file has over an environment variable that could only carry a
/// scalar.
fn count_undeclared(rows: &[Endpoint]) -> usize {
    rows.iter()
        .filter(|e| {
            !e.depth().is_stated()
                && !e.history().is_stated()
                && !e.reliability().is_stated()
                && !e.durability().is_stated()
        })
        .count()
}

/// `[meta] status` — a SUMMARY of the per-field statuses, never a substitute.
///
/// # `[params]` is deliberately NOT folded in (issue 1408)
///
/// The rule this function holds is not "any refused field anywhere". It is a
/// short, named list — `[target] pointer_bytes`, and each endpoint's
/// `wire_bound_bytes` and `registration_path` — and the members have one thing
/// in common: **every image has them**. A pointer width and a per-endpoint
/// payload size are facts of any image that has endpoints at all, so a refusal
/// there is a gap in a fact that was always going to be needed, and `partial`
/// is the honest summary.
///
/// `[params]` is not like that. It is refused exactly when SOME node declared
/// `params:` and another did not — a state only an image that uses parameters
/// at all can reach — and it is ABSENT, not refused, for the overwhelming
/// majority of images, which declare no parameters. Folding it in would be
/// wrong in both directions:
///
/// * **`Absent` must not count.** If it did, every descriptor in the tree would
///   read `partial` the day this section landed, for a section nobody filled.
///   The brief for this wave states the requirement directly: *a refused
///   `[params]` on an image that declares no parameters must NOT make a
///   fully-derived descriptor read `partial`* — and `Absent` is precisely that
///   image's state.
/// * **`Refused` must not count either**, which is the less obvious half. A
///   summary status is read by consumers that size from the WHOLE file;
///   `nros-node`'s `descriptor_subscriptions` guards on `[meta]` before it will
///   look at a single endpoint row. Letting a parameter-store gap move that
///   summary would make one image's half-authored `params:` cost every
///   UNRELATED derivation in the file its status, with no refusal anywhere near
///   the field that lost it. That is the "silently widens the basis" failure D6
///   names, run in reverse.
///
/// The per-FIELD refusal is not lost by this: `Fact::stated()` is still the only
/// accessor that yields a value, so the parameter consumer keeps its own rung
/// and prints the reason. The summary says what a summary can say, and the
/// fields say the rest. That is D6's own division, and the reason `[image]`,
/// `[types]` and `[policy]` are not folded in either — this function has never
/// been "count the refusals", and adding `[params]` would be the first step to
/// making it that.
fn overall_status(desc: &SizingDescriptor) -> Status {
    let any_refused = !desc.target.pointer_bytes().is_stated()
        || desc
            .endpoints
            .iter()
            .any(|e| !e.wire_bound_bytes().is_stated() || !e.registration_path().is_stated());
    if any_refused {
        Status::Partial
    } else {
        Status::Derived
    }
}

// --- the leaf road: assembling the inputs and writing the file ---------------

/// `<leaf>/build/nros/sizing/<image>.toml`.
///
/// Under the leaf's own `build/` beside `build/<image>/nros-cargo.toml`, and at
/// the path [`nros_sizing_descriptor::descriptor_path`] computes — the one rule,
/// so a consumer that has only the build directory finds it without knowing this
/// road.
pub fn descriptor_path_for_leaf(leaf: &std::path::Path, image_id: &str) -> std::path::PathBuf {
    nros_sizing_descriptor::descriptor_path(&leaf.join("build"), image_id)
}

/// Build and write the descriptor for one cargo leaf image, returning its path.
///
/// Called from [`crate::cmd::leaf_settings::write`], which runs on every `nros
/// sync` and every `nros build` — so the artifact is as fresh as the settings
/// file beside it, and by the same writer.
///
/// Write-if-changed, and that is load-bearing rather than tidy: the cmake
/// consumer registers this file with `CMAKE_CONFIGURE_DEPENDS` (issue 1018), so
/// rewriting identical bytes on every sync would re-arm a reconfigure forever.
pub fn write_for_leaf(
    img: &crate::cmd::leaf_settings::LeafImage,
    path_env: &std::collections::BTreeMap<String, std::path::PathBuf>,
    who: &str,
) -> eyre::Result<WrittenDescriptor> {
    let leaf = img.leaf.as_path();
    let (inventory, inv_error) = match crate::leaf_entity_env::inventory_for_leaf(leaf) {
        Ok((inv, _unprobeable)) if !inv.is_empty() => (Some(inv), None),
        Ok(_) => (None, None),
        Err(e) => (None, Some(e.to_string())),
    };
    // phase-454 W12 (RFC-0100 D3) — the CONTRACT's facts, joined onto the rows
    // the probe found. Everything above this line describes what the image
    // CREATES; only the contract describes what it KEEPS, and until this wave
    // the descriptor carried the first and called it the second.
    //
    // A leaf with no contract sidecar reaches `contract_seen == false` and its
    // rows come back untouched, which is what keeps such a leaf byte-identical
    // to every build before this wave.
    // phase-454 (issue 1408) — the contract's PARAMETERS, from the same model
    // the join below reads, resolved ONCE and kept whether or not the probe
    // found any endpoints. A leaf may declare parameters and create nothing
    // else; the endpoint table being empty says nothing about the store.
    let model = crate::leaf_entity_env::leaf_model(leaf);
    let params = model.as_ref().map(ParamDeclarations::from_model);
    let inventory = match (inventory, model) {
        (Some(inv), Some(model)) => {
            let joined = crate::contract_join::join(&inv, &model);
            for note in &joined.notes {
                eprintln!(
                    "{who}: {}: sizing descriptor: {note}",
                    leaf.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            Some(joined.inventory)
        }
        (inv, _) => inv,
    };
    if let Some(e) = &inv_error {
        eprintln!(
            "{who}: {}: sizing descriptor has no endpoint rows ({e}); every consumer keeps its \
             own defaults",
            leaf.display()
        );
    }
    // ONE read of the leaf's `generated/` trees feeds both tables. Reading them
    // twice is how the bound and the shape come to describe different trees.
    let (bounds, schema_shapes, bounds_error) =
        match crate::leaf_payload_classes::leaf_bound_rows(leaf) {
            Ok(rows) => (
                rows.iter()
                    .map(|r| (r.type_name.clone(), r.bound.clone()))
                    .collect(),
                rows.into_iter().map(|r| (r.type_name, r.shape)).collect(),
                None,
            ),
            Err(e) => (Vec::new(), Vec::new(), Some(e)),
        };

    let rmw = img.decl.rmw.clone();
    let inputs = DescriptorInputs {
        entry: img.image_id.clone(),
        inventory: inventory.as_ref(),
        bounds,
        schema_shapes,
        bounds_error,
        target_triple: img.target.clone(),
        // A board that pins no rustc triple builds for the host, and that is
        // the ONE case where this process's own width is the target's.
        host_build: img.target.is_none(),
        heap_budget_bytes: heap_budget(path_env, &img.board),
        params: params.as_ref(),
        // A cargo leaf is a Rust entry by construction: this road is cargo, and
        // the C/C++ images go through cmake.
        language: Some(EntryLanguage::Rust),
        backend_schema: rmw.as_deref().and_then(backend_schema),
        backend_dispatch: rmw.as_deref().and_then(backend_dispatch),
        rmw,
        // The leaf road HAS every input, so it has no horizon to declare.
        horizon: None,
    };
    let desc = build(&inputs);
    let body = nros_sizing_descriptor::render(&desc);

    // Issue 0320 — asserted against the directories this producer actually read
    // from, which is the only exact answer available. A ROS topic is an
    // absolute-looking string too, so no predicate over the text alone could
    // separate a leak from `/localization/kinematic_state`; the leaf and the
    // nano-ros checkout are what a `Path::display()` in a refusal reason would
    // have begun at.
    if let Some(why) =
        nros_sizing_descriptor::portability_violation(&body, &[leaf, img.config_path.as_path()])
    {
        return Err(eyre::eyre!("{why}"));
    }

    let path = descriptor_path_for_leaf(leaf, &img.image_id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| eyre::eyre!("create `{}`: {e}", dir.display()))?;
    }
    crate::atomic_file::atomic_write(&path, &body)
        .map_err(|e| eyre::eyre!("write `{}`: {e}", path.display()))?;
    Ok(WrittenDescriptor { path, desc })
}

// --- the model road: the second producer (phase-454 W14) ---------------------

/// One image, as a road that has only the resolved SystemModel can describe it.
///
/// A struct for the same reason [`DescriptorInputs`] is one: every field is
/// independently unknown, and a caller must be able to say "I do not know this"
/// without reordering a call.
#[derive(Debug)]
pub struct ModelImage<'a> {
    /// The image's build directory. The descriptor lands at
    /// `<build_dir>/nros/sizing/<entry>.toml`, by
    /// [`nros_sizing_descriptor::descriptor_path`] — the ONE path rule, so a
    /// consumer holding only the build directory finds it without knowing this
    /// road.
    pub build_dir: &'a std::path::Path,
    /// `[meta] entry`, and the file stem.
    pub entry: &'a str,
    /// What the contract declares, resolved through `EntityInventory::from_model`.
    pub inventory: &'a EntityInventory,
    /// The rustc triple the board pins, when this road knows it.
    pub target_triple: Option<String>,
    /// Is the target the host? Decides whether an absent triple is an answer.
    pub host_build: bool,
    /// `[board.knobs.memory] heap_bytes`, when this road knows it.
    pub heap_budget_bytes: Option<usize>,
    /// The backend this image links, when the image names one.
    pub rmw: Option<String>,
    /// The road, in prose, for every refusal this producer writes.
    pub road: &'a str,
}

/// Build the descriptor a MODEL-only road can honestly write, and write it.
///
/// **The caller decides whether to call this at all, and the rule is the W12
/// control**: no contract, no descriptor. `EntityInventory::from_model` returns
/// `None` for a model that describes no wiring — 109 of 114 resolvable models
/// are in that state — and writing an all-refused file for one of those would
/// put an artifact in front of seven consumers in order to say nothing, while
/// changing the `[meta] basis` every one of them guards on. So this function
/// takes an inventory by reference rather than an `Option`: reaching it at all
/// is the statement that a contract was authored.
///
/// Write-if-changed through [`crate::atomic_file::atomic_write`], because the
/// cmake consumer registers the result in `CMAKE_CONFIGURE_DEPENDS` (issue 1018)
/// and identical bytes must keep their mtime.
pub fn write_for_model(img: &ModelImage<'_>) -> eyre::Result<WrittenDescriptor> {
    let horizon = ModelHorizon::new(img.road);
    let inputs = DescriptorInputs {
        entry: img.entry.to_string(),
        inventory: Some(img.inventory),
        // EMPTY, with the reason beside it. `bounds_error` is not an error
        // channel here -- it is the one place the existing composer already
        // asks "why is there no bound table", and answering it is what carries
        // the tracked issue into `wire_bound_bytes` and `[types]`'s three
        // maxima without a second refusal path for either.
        bounds: Vec::new(),
        schema_shapes: Vec::new(),
        bounds_error: Some(horizon.bound_inventory()),
        target_triple: img.target_triple.clone(),
        host_build: img.host_build,
        heap_budget_bytes: img.heap_budget_bytes,
        // phase-454 (issue 1408) — and this road needs no horizon for them.
        // `[params]` is derived from the contract ALONE, which is the one
        // inventory a model-only producer has, so it states exactly what the
        // leaf road states. It rides on the inventory here rather than beside
        // it because this road's inventory was composed from the same model
        // (`resolve_image` / `write_from_model` both attach it) — see
        // `DescriptorInputs::params` for why the leaf road cannot do that.
        params: Some(img.inventory.param_declarations()),
        // Refused through the horizon, not through these. See
        // `ModelHorizon::registration_path` for why naming a missing half here
        // would be the wrong diagnosis.
        language: None,
        backend_schema: None,
        backend_dispatch: None,
        rmw: img.rmw.clone(),
        horizon: Some(horizon),
    };
    let desc = build(&inputs);
    let body = nros_sizing_descriptor::render(&desc);

    // Issue 0320, the same rule the leaf road holds to and for the same reason:
    // two checkouts of one tree at different paths must render identical bytes,
    // or every freshness comparison against this file is a lie. The directories
    // this producer read from are the build dir and the model the inventory
    // names.
    let source = std::path::PathBuf::from(&img.inventory.source);
    let mut roots: Vec<&std::path::Path> = vec![img.build_dir];
    if let Some(parent) = source.parent() {
        roots.push(parent);
    }
    if let Some(why) = nros_sizing_descriptor::portability_violation(&body, &roots) {
        return Err(eyre::eyre!("{why}"));
    }

    let path = nros_sizing_descriptor::descriptor_path(img.build_dir, img.entry);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| eyre::eyre!("create `{}`: {e}", dir.display()))?;
    }
    crate::atomic_file::atomic_write(&path, &body)
        .map_err(|e| eyre::eyre!("write `{}`: {e}", path.display()))?;
    Ok(WrittenDescriptor { path, desc })
}

/// What [`write_for_leaf`] produced: the artifact, and the descriptor itself.
///
/// The caller needs the descriptor and not just its path because some consumers
/// cannot read a TOML file at build time — a `cc::Build` compiling a C++ TU has
/// only the compile line — so a few STATED facts are also forwarded as cargo
/// `[env]` rows (see [`WrittenDescriptor::cyclonedds_env`]). Returning the built
/// value rather than re-reading the file keeps that projection from becoming a
/// second parse of the schema.
pub struct WrittenDescriptor {
    pub path: std::path::PathBuf,
    pub desc: SizingDescriptor,
}

impl WrittenDescriptor {
    /// phase-454 W6.c (RFC-0100 D5) — the CycloneDDS knobs this descriptor
    /// states, as `[env]` rows for the image's cargo config.
    ///
    /// Cyclone reads `[types]` and `[target].heap_budget_bytes`, and nothing
    /// else; this is that list, and it is short for exactly that reason rather
    /// than by omission. The rows reach two halves of one backend: the Rust
    /// descriptor builder through `option_env!`, and the C++ TUs through
    /// `nros-rmw-cyclonedds-sys`'s build script, which turns each into a `-D`.
    ///
    /// **A REFUSED or ABSENT fact emits NO ROW.** That is D6 in cargo's
    /// vocabulary: the consumer then keeps the default it already has, and there
    /// is no value in this table that means "I looked and found nothing".
    ///
    /// `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` is deliberately NOT here. It is
    /// derived from the SystemModel beside its `MAX_TYPES` sibling
    /// (`model_ingest::resolve_cyclonedds_max_descriptor_types`) and written to
    /// the WORKSPACE `.cargo/config.toml` with `force = true`; emitting it from
    /// here as well would give one knob two writers with different precedence,
    /// which is the drift the single-writer rule in `manage_cyclonedds_env_knob`
    /// exists to prevent.
    pub fn cyclonedds_env(&self) -> std::collections::BTreeMap<String, String> {
        use nros_sizing_descriptor::Fact;
        let mut out = std::collections::BTreeMap::new();
        let mut put = |k: &str, f: Fact<usize>| {
            if let Some(v) = f.stated() {
                out.insert(k.to_string(), v.to_string());
            }
        };
        put(
            "NROS_CYCLONEDDS_MAX_FIELDS",
            self.desc.types.max_fields().clone(),
        );
        put(
            "NROS_CYCLONEDDS_MAX_KINDS",
            self.desc.types.max_kinds().clone(),
        );
        put(
            "NROS_CYCLONEDDS_MAX_NESTED_DEPTH",
            self.desc.types.max_nested_depth().clone(),
        );
        put(
            "NROS_CYCLONEDDS_HEAP_BUDGET_BYTES",
            self.desc.target.heap_budget_bytes().clone(),
        );
        out
    }
}

/// Does this backend carry type descriptors? `None` for a name this writer does
/// not know — refused rather than guessed, per issue 1319.
fn backend_schema(rmw: &str) -> Option<BackendSchema> {
    match rmw {
        // Cyclone registers a type descriptor per type, so
        // `default_subscription_rx_bytes` reaches the bound at a type-erased
        // site.
        "cyclonedds" | "cyclone" => Some(BackendSchema::Descriptors),
        // `MessageForRmw` carries no schema on either, so the schemaless arm
        // returns `None` for every type and the registration takes `RX_BUF`.
        "zenoh" | "xrce" => Some(BackendSchema::Schemaless),
        _ => None,
    }
}

/// Does this backend hand a sample to the callback IN PLACE? `None` for a name
/// this writer does not know — refused rather than guessed, like its sibling.
///
/// **Read off the backend's own answer, not off its family.** Both are
/// unconditional in source and both were measured in phase-454 W5:
///
/// * `nros-rmw-zenoh`'s `Subscription::supports_process_in_place` is
///   `fn(&self) -> bool { true }`;
/// * XRCE's `xrce_subscription_supports_in_place` writes `true` and its
///   `process_raw_in_place` slot is non-NULL — the capability is the
///   CONJUNCTION of the two, per `rmw_vtable.h`;
/// * Cyclone leaves both slots NULL, which the cffi adapter reads as
///   unsupported.
fn backend_dispatch(rmw: &str) -> Option<BackendDispatch> {
    match rmw {
        "cyclonedds" | "cyclone" => Some(BackendDispatch::Buffered),
        "zenoh" | "xrce" => Some(BackendDispatch::InPlace),
        _ => None,
    }
}

/// `[board.knobs.memory] heap_bytes`, the RFC-0049 board rung.
///
/// Read through the board's own descriptor, which `board_facts` has already
/// located as `NROS_BOARD_TOML` — so this does not re-discover the board and
/// cannot disagree with the settings file written beside it.
fn heap_budget(
    path_env: &std::collections::BTreeMap<String, std::path::PathBuf>,
    board: &str,
) -> Option<usize> {
    let toml = path_env.get("NROS_BOARD_TOML")?;
    nros_board_common::platform_config::BoardKnobsFile::load_for_board(toml, Some(board))
        .ok()?
        .knobs
        .memory
        .heap_bytes
}

// --- the cmake projection ----------------------------------------------------

/// Project a descriptor into CMake `set()` lines.
///
/// cmake does not parse TOML and must not learn to: the schema has ONE reader
/// (`nros-sizing-descriptor`), and a second parser in `NanoRosSizingDescriptor.cmake`
/// would be the drift `check-ffi-struct-mirrors` and `check-platform-abi-mirror`
/// both exist to police one layer down. So the cmake road reaches the same reader
/// through the verb, and this is the rendering.
///
/// **A refused field emits no value and a `_REFUSED` reason instead**, so a
/// `if(DEFINED NROS_SIZING_TARGET_POINTER_BYTES)` is the only way to a number and
/// there is no spelling of "read it, and if empty use 8" that does not go through
/// the check. That is D6 in CMake's vocabulary.
pub fn to_cmake(desc: &SizingDescriptor) -> String {
    let mut out = String::new();
    out.push_str(
        "# GENERATED by `nros ws sizing-descriptor` -- do not edit.\n\
         #\n\
         # RFC-0100 D4. A REFUSED field has no `set()` at all and a `_REFUSED`\n\
         # reason beside it, so `if(DEFINED ...)` is the only road to a number.\n\
         # Register the descriptor with CMAKE_CONFIGURE_DEPENDS (issue 1018) --\n\
         # `nros_sizing_descriptor_read()` does it for you.\n\n",
    );
    out.push_str(&format!(
        "set(NROS_SIZING_SCHEMA_VERSION {})\n\
         set(NROS_SIZING_ENTRY \"{}\")\n\
         set(NROS_SIZING_STATUS \"{}\")\n\
         set(NROS_SIZING_BASIS \"{}\")\n",
        desc.schema_version,
        desc.meta.entry,
        desc.meta.status.tag(),
        desc.meta.basis.tag(),
    ));
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_UNDECLARED_ENDPOINTS",
        &desc.meta.undeclared_endpoints(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TARGET_POINTER_BYTES",
        &desc.target.pointer_bytes(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TARGET_MAX_ALIGN",
        &desc.target.max_align(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TARGET_HEAP_BUDGET_BYTES",
        &desc.target.heap_budget_bytes(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TYPES_DISTINCT_COUNT",
        &desc.types.distinct_count(),
    );
    // phase-454 W6.c -- the descriptor builder's three stack-array demands. On
    // the cmake road as well as the cargo one: the Zephyr Cyclone lane compiles
    // the backend into the app library per image, so it is a consumer of these.
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TYPES_MAX_FIELDS",
        &desc.types.max_fields(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TYPES_MAX_KINDS",
        &desc.types.max_kinds(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_TYPES_MAX_NESTED_DEPTH",
        &desc.types.max_nested_depth(),
    );
    // phase-454 W6 — the three image counts. uORB reads the subscriber one and
    // the endpoint TOPIC column beside it; a C/C++ consumer of the cffi shim
    // reads all three. Same D6 shape as every fact above: a refused one has no
    // `set()` at all, so `if(DEFINED ...)` is the only road to a number.
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_IMAGE_NODE_COUNT",
        &desc.image.node_count(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_IMAGE_BACKEND_COUNT",
        &desc.image.backend_count(),
    );
    emit_cmake_fact(
        &mut out,
        "NROS_SIZING_IMAGE_SUBSCRIBER_COUNT",
        &desc.image.subscriber_count(),
    );
    out.push_str(&format!(
        "set(NROS_SIZING_ENDPOINT_COUNT {})\n",
        desc.endpoints.len()
    ));
    // One list per column rather than one variable per endpoint: cmake lists
    // index in parallel, and a per-endpoint variable name would have to encode a
    // topic, which is not a legal identifier.
    let mut kinds = Vec::new();
    let mut types = Vec::new();
    let mut topics = Vec::new();
    let mut depths = Vec::new();
    let mut paths = Vec::new();
    let mut storage = Vec::new();
    for ep in &desc.endpoints {
        kinds.push(ep.kind.tag().to_string());
        types.push(ep.type_name.clone());
        topics.push(ep.topic.clone());
        // `REFUSED` rather than an empty slot: an empty cmake list element
        // vanishes on the next `list()` operation, which would silently shorten
        // the column and mis-align every row after it.
        depths.push(cmake_slot(&ep.depth()));
        paths.push(cmake_slot(&ep.registration_path()));
        storage.push(cmake_slot(&ep.storage_bytes()));
    }
    for (name, col) in [
        ("KIND", kinds),
        ("TYPE", types),
        ("TOPIC", topics),
        ("DEPTH", depths),
        ("REGISTRATION_PATH", paths),
        ("STORAGE_BYTES", storage),
    ] {
        out.push_str(&format!(
            "set(NROS_SIZING_ENDPOINT_{name} \"{}\")\n",
            col.join(";")
        ));
    }
    out
}

fn emit_cmake_fact<T: std::fmt::Display + Clone>(
    out: &mut String,
    name: &str,
    f: &nros_sizing_descriptor::Fact<T>,
) {
    match f {
        nros_sizing_descriptor::Fact::Stated(v) => {
            out.push_str(&format!("set({name} {v})\n"));
        }
        nros_sizing_descriptor::Fact::Refused(r) => {
            out.push_str(&format!("set({name}_REFUSED \"{}\")\n", cmake_escape(r)));
        }
        nros_sizing_descriptor::Fact::Absent => {
            out.push_str(&format!("set({name}_ABSENT TRUE)\n"));
        }
    }
}

fn cmake_slot<T: std::fmt::Display + Clone>(f: &nros_sizing_descriptor::Fact<T>) -> String {
    match f {
        nros_sizing_descriptor::Fact::Stated(v) => v.to_string(),
        nros_sizing_descriptor::Fact::Refused(_) => "REFUSED".into(),
        nros_sizing_descriptor::Fact::Absent => "ABSENT".into(),
    }
}

fn cmake_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(';', ",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_inventory::{ComponentEntities, EntityDecl};
    use nros_orchestration_ir::qos_override::{QoSDurabilityPolicy, QoSHistoryPolicy};

    fn inventory(decls: Vec<EntityDecl>) -> EntityInventory {
        let mut inv = EntityInventory::new("metadata");
        inv.insert(ComponentEntities {
            pkg: "demo".into(),
            component: "talker".into(),
            class: "Talker".into(),
            declaration: Declaration::Stated(decls),
        });
        inv
    }

    fn sub(ty: &str, topic: &str, depth: Option<u32>) -> EntityDecl {
        let mut d = EntityDecl::bare(
            EntityKind::Subscription,
            Some(ty.into()),
            Some(topic.into()),
        );
        d.depth = depth;
        d.history = Some(QoSHistoryPolicy::KeepLast);
        d
    }

    /// Issue 1378 — the adapter answers with the DESCRIPTOR's rule, over rows a
    /// road with no descriptor can supply.
    #[test]
    fn declared_entities_price_their_transient_local_cache_queryables() {
        use nros_sizing_descriptor::Fact;
        let decl = |k: EntityKind, ty: &str, name: &str| {
            EntityDecl::bare(k, Some(ty.into()), Some(name.into()))
        };

        // The image issue 1378 was filed against: one action server, whose
        // `/status` publisher is TRANSIENT_LOCAL below the declaration.
        assert_eq!(
            transient_local_publishers_from_decls(&[decl(
                EntityKind::ActionServer,
                "example_interfaces/action/Fibonacci",
                "/fibonacci",
            )]),
            Fact::Stated(1),
        );

        // A service server publishes nothing, so it owes no cache slot. The two
        // terms are carried separately for exactly this reason.
        assert_eq!(
            transient_local_publishers_from_decls(&[decl(
                EntityKind::ServiceServer,
                "example_interfaces/srv/AddTwoInts",
                "/add",
            )]),
            Fact::Stated(0),
        );

        // A publisher that states no durability REFUSES: a count over the rows
        // that answered is not a bound on the row that did not.
        assert!(matches!(
            transient_local_publishers_from_decls(&[decl(
                EntityKind::Publisher,
                "std_msgs/msg/String",
                "/chatter",
            )]),
            Fact::Refused(_),
        ));

        // A stated one is counted or not, as stated.
        let tl = |d: QoSDurabilityPolicy| {
            let mut p = decl(EntityKind::Publisher, "std_msgs/msg/String", "/chatter");
            p.durability = Some(d);
            transient_local_publishers_from_decls(&[p])
        };
        assert_eq!(tl(QoSDurabilityPolicy::TransientLocal), Fact::Stated(1));
        assert_eq!(tl(QoSDurabilityPolicy::Volatile), Fact::Stated(0));

        // A component whose whole surface is timers declared it, and none of it
        // is a publisher: that is a measured ZERO, not a refusal. `Absent` here
        // would make the consumer warn about a number it has.
        assert_eq!(
            transient_local_publishers_from_decls(&[EntityDecl::bare(
                EntityKind::Timer,
                None,
                None
            )]),
            Fact::Stated(0),
        );

        // Nothing declared at all is the one case that IS `Absent`.
        assert_eq!(transient_local_publishers_from_decls(&[]), Fact::Absent);
    }

    fn bounded(ty: &str, rx: usize) -> (String, BoundState) {
        (ty.into(), BoundState::Bounded { tx: rx - 4, rx })
    }

    fn shaped(
        ty: &str,
        fields: usize,
        kinds: usize,
        nested_depth: usize,
    ) -> (String, Option<rosidl_codegen::schema_value::SchemaShape>) {
        (
            ty.into(),
            Some(rosidl_codegen::schema_value::SchemaShape {
                fields,
                kinds,
                nested_depth,
            }),
        )
    }

    fn base<'a>(inv: &'a EntityInventory) -> DescriptorInputs<'a> {
        DescriptorInputs {
            entry: "talker".into(),
            inventory: Some(inv),
            bounds: vec![bounded("std_msgs/msg/String", 1170)],
            schema_shapes: vec![shaped("std_msgs/msg/String", 1, 1, 1)],
            bounds_error: None,
            target_triple: Some("thumbv7em-none-eabihf".into()),
            host_build: false,
            heap_budget_bytes: Some(65536),
            // The DEFAULT for these tests is "this image says nothing about
            // parameters", which is the state almost every image is in. Tests
            // about `[params]` supply their own, so the rest of the file keeps
            // asserting a descriptor whose `[params]` is empty -- which is the
            // byte-identity control (issue 1408).
            params: None,
            language: Some(EntryLanguage::Rust),
            backend_schema: Some(BackendSchema::Schemaless),
            backend_dispatch: Some(BackendDispatch::InPlace),
            rmw: Some("zenoh".into()),
            horizon: None,
        }
    }

    /// The same image, as the MODEL-only road can describe it (phase-454 W14).
    ///
    /// Built by SUBTRACTION from [`base`] rather than spelled independently, so
    /// a new input the leaf road gains cannot be silently absent here: the two
    /// producers differ by exactly the fields named below.
    fn model_only<'a>(inv: &'a EntityInventory) -> DescriptorInputs<'a> {
        let horizon = ModelHorizon::new("a workspace cargo image");
        DescriptorInputs {
            bounds: Vec::new(),
            schema_shapes: Vec::new(),
            bounds_error: Some(horizon.bound_inventory()),
            language: None,
            backend_schema: None,
            backend_dispatch: None,
            horizon: Some(horizon),
            ..base(inv)
        }
    }

    #[test]
    fn a_declared_subscription_prices_its_region_at_the_target_pointer_width() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let d = build(&base(&inv));
        let ep = &d.endpoints[0];
        assert_eq!(ep.depth().stated(), Some(&10));
        assert_eq!(ep.wire_bound_bytes().stated(), Some(&1170));
        // thumbv7em: 4-byte length word, not the 8 a host build script would
        // have measured (RFC-0100 D1, phase-118-E).
        assert_eq!(d.target.pointer_bytes().stated(), Some(&4));
        assert_eq!(ep.storage_bytes().stated(), Some(&(11 * 1170 + 11 * 4)));
        // phase-454 W5 — zenoh dispatches IN PLACE, measured, so a Rust typed
        // registration on it is that row and not the schemaless one issue
        // 1319's table assigned it.
        assert_eq!(
            ep.registration_path().stated(),
            Some(&RegistrationPath::RustTypedInPlace)
        );
        assert_eq!(d.meta.status, Status::Derived);
    }

    /// The dispatch axis decides, and it decides BEFORE the schema question —
    /// the same order `register_subscription_buffered_on` asks them in.
    #[test]
    fn the_registration_path_reads_dispatch_before_schema() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        for (schema, dispatch, want) in [
            (
                BackendSchema::Schemaless,
                BackendDispatch::InPlace,
                RegistrationPath::RustTypedInPlace,
            ),
            (
                BackendSchema::Descriptors,
                BackendDispatch::InPlace,
                RegistrationPath::RustTypedInPlace,
            ),
            (
                BackendSchema::Schemaless,
                BackendDispatch::Buffered,
                RegistrationPath::RustTypedSchemaless,
            ),
            (
                BackendSchema::Descriptors,
                BackendDispatch::Buffered,
                RegistrationPath::RustTypedDescriptors,
            ),
        ] {
            let mut i = base(&inv);
            i.backend_schema = Some(schema);
            i.backend_dispatch = Some(dispatch);
            let d = build(&i);
            assert_eq!(
                d.endpoints[0].registration_path().stated(),
                Some(&want),
                "{schema:?} + {dispatch:?}"
            );
        }
        // A C/C++ entry is credited the typed hint whatever the backend does:
        // its registration path never consults the capability.
        for dispatch in [BackendDispatch::InPlace, BackendDispatch::Buffered] {
            let mut i = base(&inv);
            i.language = Some(EntryLanguage::CFamily);
            i.backend_dispatch = Some(dispatch);
            let d = build(&i);
            assert_eq!(
                d.endpoints[0].registration_path().stated(),
                Some(&RegistrationPath::CTypedHint)
            );
        }
    }

    /// An unknown backend refuses BOTH halves and says so by name — the same
    /// rule as the schema half, because a guessed dispatch is worth a whole
    /// receive region per subscription in either direction.
    #[test]
    fn an_unknown_dispatch_refuses_the_registration_path_by_name() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.backend_dispatch = None;
        let d = build(&i);
        let path = d.endpoints[0].registration_path();
        let r = path.refusal().unwrap();
        assert!(r.contains("dispatches in place"), "{r}");
        assert!(r.contains("zenoh"), "{r}");
    }

    /// The name-keyed halves must agree about every backend this writer knows:
    /// a name that answers one and not the other refuses the whole path, which
    /// is a silent loss of the saving rather than a wrong number.
    #[test]
    fn every_known_backend_answers_both_halves() {
        for rmw in ["cyclonedds", "cyclone", "zenoh", "xrce"] {
            assert!(
                backend_schema(rmw).is_some() && backend_dispatch(rmw).is_some(),
                "backend `{rmw}` answers only one half of the registration path"
            );
        }
        assert!(backend_schema("uorb").is_none());
        assert!(backend_dispatch("uorb").is_none());
    }

    #[test]
    fn the_same_image_on_a_64_bit_board_prices_the_length_word_wider() {
        // The number `[target]` exists to move. Same declarations, same bound,
        // different board -- and a host build script would have answered the
        // host's width for both.
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.target_triple = Some("aarch64-unknown-linux-gnu".into());
        let d = build(&i);
        assert_eq!(d.target.pointer_bytes().stated(), Some(&8));
        assert_eq!(
            d.endpoints[0].storage_bytes().stated(),
            Some(&(11 * 1170 + 11 * 8))
        );
    }

    #[test]
    fn keep_all_refuses_depth_and_storage_and_degrades_nothing_else() {
        let mut ka = sub("sensor_msgs/msg/Image", "/image", Some(1));
        ka.history = Some(QoSHistoryPolicy::KeepAll);
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10)), ka]);
        let mut i = base(&inv);
        i.bounds.push(bounded("sensor_msgs/msg/Image", 4096));
        let d = build(&i);

        let image = d.endpoints.iter().find(|e| e.topic == "/image").unwrap();
        assert!(image.depth().refusal().unwrap().contains("keep_all"));
        assert!(image.storage_bytes().refusal().is_some());
        // Untouched: its own history and bound, the other endpoint, the image's
        // type table.
        assert_eq!(image.history().stated(), Some(&History::KeepAll));
        assert_eq!(image.wire_bound_bytes().stated(), Some(&4096));
        let chatter = d.endpoints.iter().find(|e| e.topic == "/chatter").unwrap();
        assert_eq!(
            chatter.storage_bytes().stated(),
            Some(&(11 * 1170 + 11 * 4))
        );
        assert_eq!(d.types.distinct_count().stated(), Some(&2));
    }

    /// phase-454 W6.c — the three `[types]` sub-fields W4 refused are STATED,
    /// and each is the max over the image's types of its own column.
    #[test]
    fn the_type_table_states_the_schema_shape_as_a_per_field_maximum() {
        let inv = inventory(vec![
            sub("std_msgs/msg/String", "/chatter", Some(10)),
            sub("sensor_msgs/msg/Image", "/image", Some(4)),
        ]);
        let mut i = base(&inv);
        i.bounds.push(bounded("sensor_msgs/msg/Image", 4096));
        // The wide type and the deep type are different types, which is the
        // whole reason the maximum is taken per column.
        i.schema_shapes = vec![
            shaped("std_msgs/msg/String", 7, 7, 1),
            shaped("sensor_msgs/msg/Image", 2, 9, 4),
        ];
        let d = build(&i);
        assert_eq!(d.types.distinct_count().stated(), Some(&2));
        assert_eq!(d.types.max_fields().stated(), Some(&7), "from String");
        assert_eq!(d.types.max_kinds().stated(), Some(&9), "from Image");
        assert_eq!(d.types.max_nested_depth().stated(), Some(&4), "from Image");
    }

    /// A type with no recorded shape REFUSES all three and NAMES itself.
    ///
    /// The alternative — a maximum over the types that do have one — is a
    /// smaller number that reads exactly like the right one, and it would
    /// under-size the descriptor builder's stack arrays silently. `distinct_count`
    /// is untouched, because refusal is per FIELD (RFC-0100 D6).
    #[test]
    fn a_type_with_no_recorded_shape_refuses_the_maximum_and_names_itself() {
        let inv = inventory(vec![
            sub("std_msgs/msg/String", "/chatter", Some(10)),
            sub("sensor_msgs/msg/Image", "/image", Some(4)),
        ]);
        let mut i = base(&inv);
        i.bounds.push(bounded("sensor_msgs/msg/Image", 4096));
        i.schema_shapes = vec![
            shaped("std_msgs/msg/String", 7, 7, 1),
            // Codegen could not build this one's schema.
            ("sensor_msgs/msg/Image".into(), None),
        ];
        let d = build(&i);
        assert_eq!(d.types.distinct_count().stated(), Some(&2));
        for f in [
            d.types.max_fields().refusal(),
            d.types.max_kinds().refusal(),
            d.types.max_nested_depth().refusal(),
        ] {
            let why = f.expect("refused, not stated");
            assert!(why.contains("sensor_msgs/msg/Image"), "{why}");
            assert!(
                !why.contains("W6.c"),
                "the wave is landed; name the type: {why}"
            );
        }
    }

    /// The `[env]` projection carries only what the descriptor STATES.
    ///
    /// A refused fact emitting a row would be a silent default wearing a
    /// derived number's clothes — the exact shape D6 forbids, one transport over.
    #[test]
    fn the_cyclonedds_env_projection_omits_a_refused_fact() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.schema_shapes = vec![shaped("std_msgs/msg/String", 3, 5, 2)];
        let full = WrittenDescriptor {
            path: std::path::PathBuf::from("x.toml"),
            desc: build(&i),
        };
        let env = full.cyclonedds_env();
        assert_eq!(
            env.get("NROS_CYCLONEDDS_MAX_FIELDS").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            env.get("NROS_CYCLONEDDS_MAX_KINDS").map(String::as_str),
            Some("5")
        );
        assert_eq!(
            env.get("NROS_CYCLONEDDS_MAX_NESTED_DEPTH")
                .map(String::as_str),
            Some("2")
        );
        assert_eq!(
            env.get("NROS_CYCLONEDDS_HEAP_BUDGET_BYTES")
                .map(String::as_str),
            Some("65536")
        );
        // The knob with the OTHER writer is never emitted here -- one knob, one
        // writer, or the two disagree about precedence.
        assert!(!env.contains_key("NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES"));

        // Now refuse the shape and the heap, and watch the rows disappear
        // rather than turn into zeros.
        i.schema_shapes = vec![("std_msgs/msg/String".into(), None)];
        i.heap_budget_bytes = None;
        let bare = WrittenDescriptor {
            path: std::path::PathBuf::from("x.toml"),
            desc: build(&i),
        };
        assert!(
            bare.cyclonedds_env().is_empty(),
            "a consumer with no declaration keeps its own default: {:?}",
            bare.cyclonedds_env()
        );
    }

    #[test]
    fn an_unknown_triple_refuses_the_target_rather_than_guessing() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.target_triple = Some("mos6502-unknown-none".into());
        let d = build(&i);
        assert!(d.target.pointer_bytes().stated().is_none());
        assert!(
            d.target
                .pointer_bytes()
                .refusal()
                .unwrap()
                .contains("mos6502")
        );
        // And the storage field that depends on it goes with it -- per field,
        // and the wire bound beside it survives.
        assert!(d.endpoints[0].storage_bytes().refusal().is_some());
        assert_eq!(d.endpoints[0].wire_bound_bytes().stated(), Some(&1170));
    }

    #[test]
    fn an_unpriced_type_refuses_its_bound_by_name() {
        let inv = inventory(vec![sub("demo_msgs/msg/Mystery", "/m", Some(1))]);
        let d = build(&base(&inv));
        let bound = d.endpoints[0].wire_bound_bytes();
        let r = bound.refusal().unwrap();
        assert!(r.contains("demo_msgs/msg/Mystery"), "{r}");
        assert!(r.contains("nros sync"), "{r}");
    }

    #[test]
    fn an_undeclared_depth_refuses_the_region_and_is_counted() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", None)]);
        let d = build(&base(&inv));
        assert!(d.endpoints[0].depth().stated().is_none());
        assert!(d.endpoints[0].storage_bytes().refusal().is_some());
        // The row DID state a history, so it is not "undeclared" in the
        // whole-row sense -- the per-policy answer is the column, which is what
        // a table transport buys over a scalar.
        assert_eq!(d.meta.undeclared_endpoints().stated(), Some(&0));
    }

    #[test]
    fn a_refused_inventory_says_so_and_does_not_report_zero_endpoints() {
        let mut i = DescriptorInputs {
            entry: "talker".into(),
            ..Default::default()
        };
        i.host_build = true;
        let d = build(&i);
        assert_eq!(d.meta.status, Status::Refused);
        assert_eq!(d.meta.basis, Basis::Closure);
        // The load-bearing distinction: "no endpoints" and "no idea" are not the
        // same statement, and a 0 here would have said the first.
        assert!(d.meta.undeclared_endpoints().stated().is_none());
        assert!(
            d.meta
                .undeclared_endpoints()
                .refusal()
                .unwrap()
                .contains("absence is not zero")
        );
    }

    #[test]
    fn an_unknown_backend_refuses_the_registration_path() {
        // Issue 1319's fact. Guessing it is worth 1,848 bytes per subscription
        // in the UNDER direction, so an image whose backend is not known must
        // not get a path.
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.backend_schema = None;
        let d = build(&i);
        let path = d.endpoints[0].registration_path();
        let r = path.refusal().unwrap();
        assert!(r.contains("1319"), "{r}");
        assert_eq!(d.meta.status, Status::Partial);
    }

    #[test]
    fn the_render_of_a_built_descriptor_parses_back_identically() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let d = build(&base(&inv));
        let text = nros_sizing_descriptor::render(&d);
        let back = nros_sizing_descriptor::parse(&text, std::path::Path::new("t.toml")).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn nothing_here_floors_a_demand() {
        // RFC-0100 D7 / issues 1015 + 1033. An image that declares no endpoint
        // publishes zero distinct types, and the consumer whose pool is illegal
        // at zero floors it at ITS pool.
        let inv = inventory(vec![]);
        let d = build(&base(&inv));
        assert_eq!(d.types.distinct_count().stated(), Some(&0));
        assert_eq!(d.meta.undeclared_endpoints().stated(), Some(&0));
        // phase-454 W6.d/W6.e — and the same for the count uORB's push-wake
        // pool and cffi's slot pool both read. Zero subscriptions IS zero
        // demand; `_nros_c_array_pool_floor` raises it where the storage
        // refuses zero, and `[T; 0]` keeps it where the storage does not.
        assert_eq!(d.image.subscriber_count().stated(), Some(&0));
    }

    #[test]
    fn the_image_counts_come_from_the_derivation_and_not_from_the_rows() {
        // phase-454 W6.e. The subscriber count is `max_subscribers`, which is
        // declared subscriptions PLUS the feedback subscription each action
        // client opens -- so it is NOT the number of `kind = "subscription"`
        // rows, and an image with an action client proves the difference.
        let mut ac = EntityDecl::bare(
            EntityKind::ActionClient,
            Some("test_msgs/action/Fibonacci".into()),
            Some("/fib".into()),
        );
        ac.history = Some(QoSHistoryPolicy::KeepLast);
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10)), ac]);
        let d = build(&base(&inv));
        let subs = d
            .endpoints
            .iter()
            .filter(|e| e.kind == EndpointKind::Subscription)
            .count();
        assert_eq!(subs, 1, "one row says `subscription`");
        // ...and the session opens two slots. A consumer that counted rows and
        // multiplied would need a third mirror of a multiplier that already has
        // two, in a build script no gate scans.
        assert_eq!(d.image.subscriber_count().stated(), Some(&2));
        assert_eq!(d.image.node_count().stated(), Some(&1));
    }

    #[test]
    fn an_image_that_names_no_backend_refuses_the_backend_count() {
        // A short backend registry is a REGISTRATION FAILURE and not a
        // truncation, so the safe direction is the builtin 8 and a guess is
        // never it.
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.rmw = None;
        let d = build(&i);
        let backends = d.image.backend_count();
        let r = backends.refusal().unwrap();
        assert!(r.contains("names no rmw"), "{r}");
        // Degrades nothing else, D6.
        assert_eq!(d.image.node_count().stated(), Some(&1));
        assert_eq!(d.image.subscriber_count().stated(), Some(&1));
    }

    #[test]
    fn a_refused_inventory_refuses_both_counts_rather_than_reporting_zero() {
        let mut i = DescriptorInputs {
            entry: "talker".into(),
            ..Default::default()
        };
        i.host_build = true;
        i.rmw = Some("zenoh".into());
        let d = build(&i);
        assert!(d.image.node_count().stated().is_none());
        assert!(
            d.image
                .subscriber_count()
                .refusal()
                .unwrap()
                .contains("absence is not zero")
        );
        // The backend half is independent of the inventory and survives.
        assert_eq!(d.image.backend_count().stated(), Some(&1));
    }

    /// phase-454 W12 — a row the contract could not be attributed to REFUSES
    /// its four QoS facts, carrying the reason to whoever sizes a buffer from
    /// it, and KEEPS COUNTING toward `undeclared_endpoints`.
    ///
    /// The second half is the load-bearing one: `undeclared_endpoints != 0` is
    /// the guard that switches every per-endpoint consumer back to its worst
    /// case (RFC-0100 D6, *"absence is not zero"*), so a refusal that quietly
    /// left the count at zero would publish a partial table as a complete one —
    /// which is the exact shape an UNDER-size takes.
    #[test]
    fn an_unattributable_row_refuses_its_qos_and_still_counts_as_undeclared() {
        let mut d = sub("std_msgs/msg/String", "on_chatter", Some(10));
        d.contract_refusal = Some(
            "no subscription in this image's contract carries \
                                   `std_msgs/msg/String` on `/chatter`"
                .to_string(),
        );
        let inv = inventory(vec![d]);
        let desc = build(&base(&inv));
        let ep = &desc.endpoints[0];
        for f in [
            ep.depth().refusal(),
            ep.history().refusal(),
            ep.reliability().refusal(),
            ep.durability().refusal(),
        ] {
            assert!(
                f.expect("refused, not absent").contains("contract"),
                "every QoS fact carries the attribution refusal"
            );
        }
        // The declared `depth: 10` on the row is NOT published: it came from a
        // declaration this reader could not attribute, and publishing it would
        // be the mis-attribution the refusal exists to prevent.
        assert_eq!(ep.depth().stated(), None);
        assert_eq!(
            desc.meta.undeclared_endpoints().stated(),
            Some(&1),
            "the guard every per-endpoint consumer reads"
        );
        // A refusal never degrades another consumer's facts (D6): the payload
        // class beside it is a property of the TYPE and survives.
        assert_eq!(ep.wire_bound_bytes().stated(), Some(&1170));
        assert_eq!(desc.types.distinct_count().stated(), Some(&1));
    }

    /// The receive region goes with the depth, and says WHY — but only on the
    /// kind that has one. A publisher's row must not gain a `storage_bytes`
    /// refusal it could never have had a value for.
    #[test]
    fn an_unattributable_publisher_refuses_no_receive_region() {
        let mut p = EntityDecl::bare(
            EntityKind::Publisher,
            Some("std_msgs/msg/String".into()),
            Some("/chatter".into()),
        );
        p.contract_refusal = Some("the contract describes no publisher".to_string());
        let inv = inventory(vec![p]);
        let desc = build(&base(&inv));
        let ep = &desc.endpoints[0];
        assert!(ep.storage_bytes().refusal().is_none());
        assert!(ep.storage_bytes().stated().is_none());
        assert!(ep.depth().refusal().is_some());
    }

    #[test]
    fn the_cmake_projection_carries_the_image_counts() {
        // uORB and every C/C++ consumer of the cffi shim read these through the
        // projection; a refused one gets no `set()` at all, so `if(DEFINED ...)`
        // stays the only road to a number.
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let mut i = base(&inv);
        i.rmw = None;
        let out = to_cmake(&build(&i));
        assert!(out.contains("set(NROS_SIZING_IMAGE_NODE_COUNT 1)"), "{out}");
        assert!(
            out.contains("set(NROS_SIZING_IMAGE_SUBSCRIBER_COUNT 1)"),
            "{out}"
        );
        assert!(
            !out.contains("set(NROS_SIZING_IMAGE_BACKEND_COUNT "),
            "{out}"
        );
        assert!(
            out.contains("set(NROS_SIZING_IMAGE_BACKEND_COUNT_REFUSED "),
            "{out}"
        );
    }

    // --- phase-454 W14: the model-only producer -----------------------------

    /// THE RULING, as one assertion: emit what the SystemModel knows, refuse
    /// every field that needs a leaf, and NAME the follow-up in each reason.
    ///
    /// The five refused fields are enumerated rather than counted. A count
    /// would pass if a refusal moved from one field to another, which is the
    /// shape of every silent mis-description this model exists to remove.
    #[test]
    fn a_model_only_descriptor_states_the_declaration_and_refuses_the_five_leaf_facts() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let d = build(&model_only(&inv));
        let ep = &d.endpoints[0];

        // STATED — the SystemModel carries every one of these.
        assert_eq!(ep.depth().stated(), Some(&3));
        assert_eq!(ep.history().stated(), Some(&History::KeepLast));
        assert_eq!(d.image.node_count().stated(), Some(&1));
        assert_eq!(d.image.backend_count().stated(), Some(&1));
        assert_eq!(d.image.subscriber_count().stated(), Some(&1));
        assert_eq!(d.types.distinct_count().stated(), Some(&1));
        // `[target]` comes from the BOARD, which this road also has.
        assert_eq!(d.target.pointer_bytes().stated(), Some(&4));
        assert_eq!(d.meta.undeclared_endpoints().stated(), Some(&0));
        assert_eq!(d.meta.basis, Basis::Contract);

        // REFUSED — and every reason names the tracked follow-up, so the
        // artifact is the checklist the day it closes.
        for (what, reason) in [
            ("wire_bound_bytes", ep.wire_bound_bytes().refusal()),
            ("storage_bytes", ep.storage_bytes().refusal()),
            ("registration_path", ep.registration_path().refusal()),
            ("types.max_fields", d.types.max_fields().refusal()),
            ("types.max_kinds", d.types.max_kinds().refusal()),
            (
                "types.max_nested_depth",
                d.types.max_nested_depth().refusal(),
            ),
        ] {
            let reason =
                reason.unwrap_or_else(|| panic!("`{what}` must be REFUSED, not stated or absent"));
            assert!(
                reason.contains(MODEL_ONLY_ISSUE),
                "`{what}`'s refusal must name {MODEL_ONLY_ISSUE}: {reason}"
            );
        }
        // A partial descriptor says so in `[meta]`, so a reader who only
        // glances is not told "derived".
        assert_eq!(d.meta.status, Status::Partial);
    }

    /// The refusals say WHICH ROAD wrote the file.
    ///
    /// Two producers reach this code and their remedies differ; a reader
    /// holding the artifact has no other way to tell which one wrote it.
    #[test]
    fn a_model_only_refusal_names_the_road_it_was_written_on() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let mut i = model_only(&inv);
        let horizon = ModelHorizon::new("a cmake entry");
        i.bounds_error = Some(horizon.bound_inventory());
        i.horizon = Some(horizon);
        let d = build(&i);
        let ep = &d.endpoints[0];
        for reason in [
            ep.wire_bound_bytes().refusal().expect("refused"),
            ep.storage_bytes().refusal().expect("refused"),
            ep.registration_path().refusal().expect("refused"),
        ] {
            assert!(reason.contains("a cmake entry"), "{reason}");
        }
    }

    /// The horizon refuses exactly THREE per-row fields and degrades nothing
    /// else — RFC-0100 D6's "a refusal never degrades another consumer's
    /// facts", asserted against the leaf road's own answer for the same image.
    ///
    /// The negative control for the whole wave: if the horizon ever started
    /// switching off a QoS policy or a count, this is what reds.
    #[test]
    fn the_horizon_refuses_only_the_leaf_facts_and_the_declaration_is_identical() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let leaf = build(&base(&inv));
        let model = build(&model_only(&inv));
        assert_eq!(leaf.endpoints.len(), model.endpoints.len());
        let (l, m) = (&leaf.endpoints[0], &model.endpoints[0]);
        assert_eq!(l.kind, m.kind);
        assert_eq!(l.topic, m.topic);
        assert_eq!(l.type_name, m.type_name);
        assert_eq!(l.depth(), m.depth());
        assert_eq!(l.history(), m.history());
        assert_eq!(l.reliability(), m.reliability());
        assert_eq!(l.durability(), m.durability());
        assert_eq!(leaf.image.node_count(), model.image.node_count());
        assert_eq!(leaf.image.backend_count(), model.image.backend_count());
        assert_eq!(
            leaf.image.subscriber_count(),
            model.image.subscriber_count()
        );
        assert_eq!(leaf.target.pointer_bytes(), model.target.pointer_bytes());
        assert_eq!(
            leaf.meta.undeclared_endpoints(),
            model.meta.undeclared_endpoints()
        );
        assert_eq!(leaf.types.distinct_count(), model.types.distinct_count());
        // And the leaf road still STATES the three the horizon refuses, so
        // this test cannot pass by both sides being empty.
        assert!(l.wire_bound_bytes().is_stated());
        assert!(l.storage_bytes().is_stated());
        assert!(l.registration_path().is_stated());
    }

    /// `keep_all` still refuses `depth` on this road, with its OWN reason.
    ///
    /// The horizon must not swallow a refusal the DECLARATION earns: a
    /// `keep_all` queue has no static bound whoever wrote the file, and a
    /// reader told "no bound inventory" there would go looking for codegen.
    #[test]
    fn keep_all_keeps_its_own_refusal_under_a_horizon() {
        let mut d = sub("std_msgs/msg/String", "/image", Some(1));
        d.history = Some(QoSHistoryPolicy::KeepAll);
        let inv = inventory(vec![d]);
        let desc = build(&model_only(&inv));
        let ep = &desc.endpoints[0];
        let depth = ep.depth();
        let why = depth.refusal().expect("keep_all refuses depth");
        assert!(why.contains("KEEP_ALL"), "{why}");
        assert_eq!(ep.history().stated(), Some(&History::KeepAll));
    }

    /// CYCLONE, acceptance row 2: a model-only descriptor emits NO `[types]`
    /// cargo `[env]` row, so the descriptor builder keeps `dynamic_type.rs`'s
    /// `option_env!` defaults exactly as it did with no descriptor at all.
    ///
    /// Cyclone reads `[types]`'s three maxima and `[target].heap_budget_bytes`
    /// and nothing else. The three maxima are REFUSED here and the projection
    /// drops a non-stated fact — there is no value in that table meaning "I
    /// looked and found nothing" — so the three literals stand.
    ///
    /// The heap budget is the exception and is NOT a refusal leaking through:
    /// it is the BOARD's `[board.knobs.memory] heap_bytes`, which this road
    /// does have. It is asserted rather than excluded, because a test that
    /// only said "empty" would also pass if the board rung stopped travelling.
    ///
    /// Asserted here rather than in a build because a build cannot show an
    /// ABSENT row — only that nothing changed, which is also what a projection
    /// broken for everyone looks like. Hence the leaf-road control below.
    #[test]
    fn a_model_only_descriptor_emits_no_cyclonedds_type_row() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let written = WrittenDescriptor {
            path: std::path::PathBuf::from("<test>"),
            desc: build(&model_only(&inv)),
        };
        let rows = written.cyclonedds_env();
        for refused in [
            "NROS_CYCLONEDDS_MAX_FIELDS",
            "NROS_CYCLONEDDS_MAX_KINDS",
            "NROS_CYCLONEDDS_MAX_NESTED_DEPTH",
        ] {
            assert!(
                !rows.contains_key(refused),
                "a refused fact must emit no row: {refused}"
            );
        }
        assert_eq!(
            rows.get("NROS_CYCLONEDDS_HEAP_BUDGET_BYTES")
                .map(String::as_str),
            Some("65536"),
            "the BOARD's heap rung is stated on this road and must still travel"
        );
        // The leaf road, same image, emits all four — so this test cannot pass
        // by the projection being broken for everyone.
        let leaf = WrittenDescriptor {
            path: std::path::PathBuf::from("<test>"),
            desc: build(&base(&inv)),
        };
        assert_eq!(leaf.cyclonedds_env().len(), 4);
    }

    /// A model-only descriptor round-trips through the shared reader.
    ///
    /// Not a formality: the renderer enforces three parse rules (D4), one of
    /// which is that a key must not appear in BOTH the value slot and
    /// `refused`. The horizon writes refusals for fields the composer might
    /// also have set, so this is the test that would catch it.
    #[test]
    fn a_model_only_descriptor_parses_back() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let d = build(&model_only(&inv));
        let body = nros_sizing_descriptor::render(&d);
        let back = nros_sizing_descriptor::parse(&body, std::path::Path::new("<test>"))
            .expect("a model-only descriptor is a legal descriptor");
        assert_eq!(back, d);
    }

    // --- phase-454 (issue 1408): `[params]` ---------------------------------

    fn declared_param(
        node: &str,
        name: &str,
        ty: ros_launch_manifest_model::ParamType,
    ) -> crate::entity_inventory::DeclaredParam {
        crate::entity_inventory::DeclaredParam {
            node: node.into(),
            name: name.into(),
            ty,
        }
    }

    /// Two nodes, one `string` parameter and one `integer`.
    fn declared_params() -> ParamDeclarations {
        use ros_launch_manifest_model::ParamType as T;
        ParamDeclarations::Declared {
            nodes: vec!["/a".into(), "/b".into()],
            params: vec![
                declared_param("/a", "greeting", T::String),
                declared_param("/b", "rate", T::Integer),
            ],
        }
    }

    /// THE RULING for `Declared`, as one assertion: every field STATED, each
    /// from the derivation that already owns it.
    ///
    /// The numbers are spelled out rather than recomputed from `sizing()`,
    /// because a test that calls the producer to check the producer asserts
    /// only that the call happened.
    #[test]
    fn a_declared_contract_states_every_parameter_fact() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let decl = declared_params();
        let d = build(&DescriptorInputs {
            params: Some(&decl),
            ..base(&inv)
        });

        assert_eq!(d.params.declared().stated(), Some(&2));
        // Per node: `/a` has `greeting` + the seeded `use_sim_time`, `/b` has
        // `rate` + the seed. `ParamStoreSizing::max_parameters` SUMS them.
        assert_eq!(d.params.max_parameters().stated(), Some(&4));
        // `use_sim_time` is 12 bytes, `greeting` 8, `rate` 4.
        assert_eq!(d.params.max_param_name_len().stated(), Some(&12));

        // The capacity NEEDS. `Unused` is a STATEMENT, not an absence -- the
        // distinction this section exists to carry.
        assert_eq!(
            d.params.needs_max_string_value_len().stated(),
            Some(&CapacityNeed::NeededBy {
                node: "/a".into(),
                name: "greeting".into()
            })
        );
        assert_eq!(
            d.params.needs_max_array_len().stated(),
            Some(&CapacityNeed::Unused)
        );
        assert_eq!(
            d.params.needs_max_byte_array_len().stated(),
            Some(&CapacityNeed::Unused)
        );

        // The TOKEN, byte-identical to the one
        // `NROS_DECLARED_PARAM_SERVICE_SHAPE` carries -- which is the whole
        // reason `service_shape` is a string. `nros-node/build.rs` parses
        // exactly this grammar and nothing re-spells it.
        let shapes = decl.service_shapes().expect("a declared contract shapes");
        assert_eq!(
            d.params.service_shape().stated().map(String::as_str),
            Some(ParamServiceShape::token(&shapes).as_str())
        );
    }

    /// `Absent` writes NOTHING, and that is the byte-identity control.
    ///
    /// An image whose contract says nothing about parameters has not been
    /// looked at and failed -- it has not been asked. A refusal here would put
    /// a reason in front of every reader of every descriptor in the tree to say
    /// that a feature nobody used was not used, and would make a fully-derived
    /// descriptor carry a refused field.
    #[test]
    fn an_absent_contract_leaves_the_section_empty_and_the_file_unchanged() {
        use nros_sizing_descriptor::Fact;
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let none = nros_sizing_descriptor::render(&build(&base(&inv)));
        let absent = build(&DescriptorInputs {
            params: Some(&ParamDeclarations::Absent),
            ..base(&inv)
        });
        assert_eq!(
            none,
            nros_sizing_descriptor::render(&absent),
            "`None` and `Absent` are the same statement and must render alike"
        );
        // The HEADER is unconditional, exactly as `[policy]`'s is -- the
        // renderer writes every section and `emit_values` skips the keys with
        // no value. So an image that says nothing about parameters carries an
        // EMPTY `[params]`, with no key and no refusal, which is the honest
        // artifact: the section exists in the schema and this image filled none
        // of it.
        assert!(none.contains("\n[params]\n\n[policy]\n"), "{none}");
        assert!(!none.contains("[params.refused]"), "{none}");
        assert_eq!(absent.params.declared(), Fact::Absent);
        assert_eq!(
            absent.params.needs_max_array_len(),
            Fact::Absent,
            "ABSENT, never `Unused` -- nobody said is not the same as `no array`"
        );
        // And the SUMMARY is untouched: a fully-derived descriptor that says
        // nothing about parameters still reads `derived`.
        assert_eq!(absent.meta.status, Status::Derived);
    }

    /// `Refused` refuses every field, carries the derivation's OWN prose, and
    /// says which refusals are consequences of which.
    #[test]
    fn a_refused_contract_refuses_every_field_and_keeps_the_derivations_reason() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        // The real prose: `ParamDeclarations::from_model` names the silent
        // nodes. Restating it in this module would be a second author for one
        // diagnosis, so the composer CARRIES it and this test asserts it did.
        let decl = ParamDeclarations::Refused {
            reason: "1 of 2 nodes in this image declare no `params:` in their contract: /b."
                .to_string(),
        };
        let d = build(&DescriptorInputs {
            params: Some(&decl),
            ..base(&inv)
        });

        for (what, r) in [
            ("declared", d.params.declared().refusal()),
            ("max_parameters", d.params.max_parameters().refusal()),
            (
                "max_param_name_len",
                d.params.max_param_name_len().refusal(),
            ),
            (
                "needs_max_string_value_len",
                d.params.needs_max_string_value_len().refusal(),
            ),
            (
                "needs_max_array_len",
                d.params.needs_max_array_len().refusal(),
            ),
            (
                "needs_max_byte_array_len",
                d.params.needs_max_byte_array_len().refusal(),
            ),
            ("service_shape", d.params.service_shape().refusal()),
        ] {
            let r = r.unwrap_or_else(|| panic!("`{what}` must be REFUSED"));
            assert!(
                r.contains("/b"),
                "`{what}` must carry the derivation's own reason, which names the node: {r}"
            );
        }
        // The four CONSEQUENT refusals say they are consequences, the way
        // `set_storage_bytes` does when `depth` is refused -- so a reader does
        // not go looking for a second, independent cause.
        assert!(
            d.params
                .needs_max_array_len()
                .refusal()
                .is_some_and(|r| r.starts_with("the contract's parameter declarations are refused")),
            "{:?}",
            d.params.needs_max_array_len().refusal()
        );
        // And the SUMMARY is STILL `derived`: a parameter-store gap must not
        // cost every unrelated derivation in the file its status. See
        // `overall_status` for the argument.
        assert_eq!(d.meta.status, Status::Derived);
    }

    /// A `[params]`-bearing descriptor round-trips through the shared reader.
    ///
    /// The same reason the model-only round trip exists: the renderer enforces
    /// the D4 parse rules, and a field that were both set AND refused would be
    /// caught only here.
    #[test]
    fn a_params_descriptor_parses_back() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        for decl in [
            declared_params(),
            ParamDeclarations::Refused {
                reason: "/b said nothing".into(),
            },
            ParamDeclarations::Absent,
        ] {
            let d = build(&DescriptorInputs {
                params: Some(&decl),
                ..base(&inv)
            });
            let body = nros_sizing_descriptor::render(&d);
            let back = nros_sizing_descriptor::parse(&body, std::path::Path::new("<test>"))
                .unwrap_or_else(|e| panic!("`{}` must render a legal descriptor: {e}", decl.tag()));
            assert_eq!(back, d, "{}", decl.tag());
        }
    }

    /// BOTH producers fill it, from the ONE composer.
    ///
    /// `[params]` is derived from the contract ALONE, which is the one
    /// inventory a model-only road has -- so unlike `wire_bound_bytes` it is
    /// NOT narrowed by the horizon, and the two roads must agree exactly.
    #[test]
    fn the_model_only_road_states_the_same_parameter_facts_as_the_leaf_road() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let decl = declared_params();
        let leaf = build(&DescriptorInputs {
            params: Some(&decl),
            ..base(&inv)
        });
        let model = build(&DescriptorInputs {
            params: Some(&decl),
            ..model_only(&inv)
        });
        assert_eq!(leaf.params, model.params);
        // ...and the model road really is narrower elsewhere, so this test
        // cannot pass by the horizon having stopped working.
        assert!(model.endpoints[0].wire_bound_bytes().refusal().is_some());
    }
}

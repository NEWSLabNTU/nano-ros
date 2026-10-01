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
//! W14 refused five fields there, every reason naming issue 1393. **Issue 1393
//! is closed, and none of the five is refused for the ROAD any more** — each is
//! composed from inputs the model roads now carry, by the same code the leaf
//! road runs, and refuses only on the input that is actually missing:
//!
//! | field | input, and who supplies it on a model road |
//! | --- | --- |
//! | `wire_bound_bytes` | the bound tables the image's closure REGISTERED (phase-457-payload W2) |
//! | `[types] max_fields` / `max_kinds` / `max_nested_depth` | the schema shapes in those same tables (W2) |
//! | `storage_bytes` | that bound, the board's pointer width (W4's triple) and the declared depth — [`set_storage_bytes`], the leaf road's own chain |
//! | `registration_path` | the backend (from `rmw`) and, for a backend that dispatches in place, an OBSERVATION of the endpoint's registration (phase-457 W3) |
//!
//! What stays refused is narrower than a road and is said in the reason: a road
//! handed no bound table at all ([`ModelHorizon::bound_inventory`]), and an
//! in-place-dispatching backend whose rows no probe observed
//! ([`ModelHorizon::registration_path`] — on the model road that is every row,
//! because the composed inventory carries no observation; issue 1594).

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
///
/// # Why this is not [`nros_lang::Language`] (phase-469)
///
/// It is a NARROWING, in the sense RFC-0091 §1 and `nros-lang`'s own module
/// docs give the word: two values where the enumeration has three, because
/// C and C++ give the SAME answer to this question and saying so is
/// information. Replacing it with `Language` would make
/// [`registration_path`]'s table spell `(Language::C, _)` and
/// `(Language::Cpp, _)` as two arms that must agree, and nothing would then
/// state that they must.
///
/// What it was MISSING is the derivation — a narrowing is supposed to derive
/// from the enumeration rather than exist beside it, and this one had no
/// relationship to `Language` at all. [`From<nros_lang::Language>`] is that
/// relationship, and it is what makes a fourth language a compile error here:
/// whoever adds one has to answer "does it register like Rust or like C?"
/// rather than have the question not come up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryLanguage {
    Rust,
    /// C and C++ share a path here: both register through the same typed hint
    /// (`nros::rx_size_bound<M>` / `rx_buffer_hint`), and both fall to the raw
    /// no-hint row when they do not supply one.
    CFamily,
}

impl From<nros_lang::Language> for EntryLanguage {
    fn from(lang: nros_lang::Language) -> Self {
        match lang {
            nros_lang::Language::Rust => EntryLanguage::Rust,
            nros_lang::Language::C | nros_lang::Language::Cpp => EntryLanguage::CFamily,
        }
    }
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
/// measured.** `Executor::open_subscription` asks
/// `handle.supports_process_in_place()` BEFORE it computes a slot size, and
/// returns through an in-place entry when the answer is yes — so a subscription
/// on such a backend allocates no receive region at all, whatever its type's
/// bound or the image's `RX_BUF` say. Measured on `contract-monitor-sub` over
/// zenoh: 672 bytes of arena for the whole registration.
///
/// **phase-456 W8 made the question apply to every LANGUAGE.** Until W8 the
/// capability was consulted in exactly one registration path — the Rust typed
/// one — so this axis reached only Rust entries and the C arm of
/// [`registration_path`] had to say so. The executor now has one consulting
/// site, `open_subscription`, which every registration that creates its own
/// subscriber goes through.
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

/// The follow-up a model-road `registration_path` refusal names.
///
/// issue 1393 closed the FIELD axis: every field a model road used to refuse
/// for being a model road is now composed from an input that road carries. The
/// one per-row refusal left that is about the ROAD is an unobserved
/// registration on a backend that dispatches in place — the model road's
/// composed inventory (the SystemModel plus `nros-metadata.json`) carries no
/// per-endpoint observation, although the probe sidecars a workspace's
/// `nros sync` writes do. That is issue 1594, and ONE spelling of it keeps the
/// refusals in a written descriptor greppable for the day it closes.
pub const UNOBSERVED_ON_MODEL_ROAD_ISSUE: &str = "issue 1594";

/// What a producer that has no LEAF inventories reads, and what that costs.
///
/// [`write_for_leaf`] reads a probed leaf; [`write_for_model`] reads a resolved
/// SystemModel (composed with `nros-metadata.json`) or a leaf's `system.toml`
/// declaration. Since issue 1393 closed, both roads receive the bound tables
/// their interface closure registered and the board's triple, so the FIELDS
/// they can state are the leaf road's. What a horizon still changes is PROSE:
/// which input a refusal was written from, and — for the one input the model
/// road genuinely lacks, an observation of each registration — why.
///
/// Carrying it as a value rather than a `bool` is what lets a refusal say WHICH
/// road it was written on: "a workspace cargo image" and "a cmake entry" want
/// different remedies, and a reader holding the file has no other way to tell
/// which producer wrote it.
#[derive(Debug, Clone)]
pub struct ModelHorizon {
    road: String,
    from: &'static str,
    unobserved: &'static str,
}

/// What a model-only producer reads, in the words its refusals use.
///
/// phase-457 W0.b — the horizon gained a SECOND source, and the prose is not
/// interchangeable. A standalone leaf has no resolved SystemModel at all; a
/// refusal telling its author that "the resolved SystemModel carries no
/// observation" aims them at an artifact that does not exist on their road,
/// which is the diagnostic failure issue 1033 records one layer down ("a
/// diagnostic that survives the mechanism it describes aims the next reader at
/// a wall").
const FROM_MODEL: &str = "the resolved SystemModel and `nros-metadata.json`";
const FROM_LEAF_DECLARATION: &str =
    "the leaf's own `system.toml` `[[component]] entities` declaration alone";

/// Why an endpoint's registration is UNOBSERVED, per SOURCE.
///
/// The two sources lack the observation for different reasons, and the
/// remedies differ. The model road's rows describe endpoints whose code a
/// workspace probe DID run — the sidecars carry `in_place` — but the inventory
/// this producer composes reads neither sidecar, so the fact exists and is not
/// joined (issue 1594). A leaf DECLARATION names an entity and no call site at
/// all; issue 1522 ruled that population a refusal by design, because there is
/// no registrar for a classifier to be consistent with.
const MODEL_ROWS_ARE_NOT_JOINED_TO_THE_PROBE: &str = "the inventory composed here carries no \
     per-endpoint observation: the SystemModel names endpoints from the launch tree and the \
     contract, and `nros-metadata.json` names components, while the observation lives in the \
     probe's per-component sidecars, which this producer does not read";
const A_DECLARATION_IS_NOT_A_CALL_SITE: &str = "a declaration names the entity, never the CALL \
     SITE that registers it, and nine of the executor's eleven subscription entry points cannot \
     dispatch in place -- which one a site calls is visible only in the source (issue 1522 rules \
     this population a refusal by design)";

impl ModelHorizon {
    /// `road` names the producer in prose — "a workspace cargo image",
    /// "a cmake entry". It appears verbatim in every refusal.
    pub fn new(road: impl Into<String>) -> Self {
        Self {
            road: road.into(),
            from: FROM_MODEL,
            unobserved: MODEL_ROWS_ARE_NOT_JOINED_TO_THE_PROBE,
        }
    }

    /// phase-457 W0.b (issues 1407 / 1378) — the STANDALONE LEAF road.
    ///
    /// Same horizon, different input: a copy-out cmake project has no bringup
    /// and no SystemModel, so `EntityDecl::parse` over its `system.toml` is the
    /// whole of what it declares. Only the first clause of each reason, and the
    /// account of why a registration is unobserved, move.
    pub fn for_leaf_declaration(road: impl Into<String>) -> Self {
        Self {
            road: road.into(),
            from: FROM_LEAF_DECLARATION,
            unobserved: A_DECLARATION_IS_NOT_A_CALL_SITE,
        }
    }

    /// The `bounds_error` a producer supplies when its caller handed it NO
    /// bound table.
    ///
    /// Since phase-457-payload W2 both model roads pass every table their
    /// closure registered (`--bound-inventory`, from
    /// `NROS_MESSAGE_BOUNDS_FRAGMENTS` on cmake and `generated_bound_tables` on
    /// a cargo workspace), so reaching this means the closure registered none —
    /// a narrower and actionable statement than "this road has no bound
    /// inventory", which stopped being true when W2 landed. It reaches TWO
    /// families through the code that already exists: `wire_bound_bytes` on
    /// every row and `[types]`'s three maxima.
    pub fn bound_inventory(&self) -> String {
        format!(
            "this descriptor was written from {from} ({road}), and that road handed it no \
             message-bound table: no `nros_message_bounds.json` is registered for this image's \
             interface closure. Codegen writes one beside every interface package it generates \
             (`nros sync`, or the build that runs codegen), so a type nothing generated is a type \
             nothing priced",
            from = self.from,
            road = self.road
        )
    }

    /// `registration_path`'s refusal for an endpoint whose registration nothing
    /// OBSERVED, on a backend that dispatches in place.
    ///
    /// The one per-row refusal that is still about the road. On such a backend
    /// the row's answer turns on which entry point the endpoint's code calls —
    /// in place, or one of the buffered rows — and crediting it with in-place
    /// dispatch prices it at NO receive region (issue 1340), so it is refused
    /// and keeps its region: the over-statement, never the under-size.
    pub fn registration_path(&self) -> String {
        format!(
            "this descriptor was written from {from} ({road}), which does not say how each \
             endpoint REGISTERS -- {why}. The backend dispatches in place, so the row turns on the \
             entry point, and crediting an unobserved endpoint with in-place dispatch would price \
             it at no receive region; refused, so it keeps its region. {tracked}",
            from = self.from,
            road = self.road,
            why = self.unobserved,
            tracked = if self.unobserved == MODEL_ROWS_ARE_NOT_JOINED_TO_THE_PROBE {
                format!("Tracked by {UNOBSERVED_ON_MODEL_ROAD_ISSUE}")
            } else {
                "Refused by design on this road".to_string()
            },
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
    /// `Some` changes PROSE, never which fields are composed (issue 1393): the
    /// `registration_path` refusal for an unobserved row on an in-place backend
    /// says why this road has no observation, and the `bounds_error` a caller
    /// supplies says which road handed over no table. Every field is composed
    /// by the same code on every road, from whatever inputs it was given.
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
    let answer = nros_sizing_descriptor::transient_local_publishers_over(tl_rows(decls));
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

/// The declarations as the rule's rows -- ONE mapping, shared by the count and
/// its bound, so the two cannot disagree about which row is a publisher.
fn tl_rows(
    decls: &[crate::entity_inventory::EntityDecl],
) -> impl Iterator<Item = nros_sizing_descriptor::TlRow<'_>> {
    decls.iter().filter_map(|d| {
        endpoint_kind(d.kind).map(|kind| nros_sizing_descriptor::TlRow {
            kind,
            durability: match d.durability.and_then(map_durability) {
                Some(v) => nros_sizing_descriptor::Fact::Stated(v),
                None => nros_sizing_descriptor::Fact::Absent,
            },
            topic: d.name.as_deref().unwrap_or("<unnamed>"),
            type_name: d.type_name.as_deref().unwrap_or("<untyped>"),
        })
    })
}

/// The slots a pool must hold for these declarations' transient-local
/// publishers: the count when [`transient_local_publishers_from_decls`] states
/// one, its WORST CASE when it refuses -- issue 1572, RFC-0100 D6. See
/// [`nros_sizing_descriptor::transient_local_publishers_bound_over`].
///
/// `None` only for no declarations at all, where the count is `Absent` too.
/// Declarations with no endpoint rows (timers only) bound at zero, for the
/// reason the count's adapter gives.
pub fn transient_local_publishers_bound_from_decls(
    decls: &[crate::entity_inventory::EntityDecl],
) -> Option<usize> {
    if decls.is_empty() {
        return None;
    }
    Some(nros_sizing_descriptor::transient_local_publishers_bound_over(tl_rows(decls)).unwrap_or(0))
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
    let bound = set_wire_bound(&mut ep, kind, ty, inputs);

    // phase-457 W3 — ONE composer, on every road, because the path is no longer
    // one answer per image.
    //
    // phase-454 W14 had the model road refuse the field OUTRIGHT here: with the
    // in-place row inferred from the entry's LANGUAGE, a road with no single
    // language could say nothing at all. Now that the row is OBSERVED per
    // endpoint the model road can state it — the two halves it needs are the
    // backend (from `rmw`, which it has) and the observation (from the probe's
    // metadata, which W0 composed in) — while the two buffered rows still need
    // the language and still refuse with the horizon's own prose. So the
    // road-specific refusal moved INSIDE the composer, where the arm that
    // actually runs out of inputs is the one that reports.
    match registration_path(kind, inputs, d.in_place_capable) {
        Ok(path) => {
            ep.set_registration_path(Some(path));
        }
        Err(why) => {
            ep.refuse("registration_path", why);
        }
    }

    // Only a subscription claims a topic-sample receive region. Every other kind
    // leaves `storage_bytes` ABSENT, which is the accurate statement: nothing
    // refused it, there is simply no such region. A publisher serializes into a
    // per-call stack array, which is a transmit buffer and a different question.
    if kind.receives_topic_sample() {
        // issue 1393 — ONE chain on every road. phase-454 W14 refused this
        // field outright under a horizon, with a reason saying the region had
        // "neither of its two sizes"; once phase-457-payload W2 handed the model
        // roads their bound tables and W4 their triple, that reason was FALSE on
        // a row stating both `wire_bound_bytes` and `[target] pointer_bytes`
        // beside it (measured: `examples/workspaces/cpp`'s `native_entry`). The
        // chain refuses on whichever input is actually missing and names the
        // refusal it inherits, so the diagnosis is the row's own.
        if unattributed.is_some() {
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
fn set_wire_bound(
    ep: &mut Endpoint,
    kind: EndpointKind,
    ty: &str,
    inputs: &DescriptorInputs<'_>,
) -> Option<usize> {
    // phase-461 W3 -- a service or action row's `type` is the INTERFACE
    // (`pkg/srv/Name`), and no such type ever crosses a wire. What crosses it,
    // and what a server's inbox slot has to hold, is the REQUEST
    // (`pkg/srv/Name_Request`); codegen prices that one since W3 and priced
    // neither before it, which is why W6.a's own header records this join
    // refusing on every in-tree image.
    //
    // phase-457 W1 (issue 1506) -- a SET, because an action server's three
    // queryables share one ring and receive three different request types. The
    // spellings come from `rosidl_codegen` and never from a `format!` here,
    // because several consumers join on them.
    //
    // ONE unpriced member refuses the whole row, exactly as
    // `declared_service_request_bytes` refuses a whole family: the slot is
    // shared, so a maximum over the members that answered is not a bound on the
    // ones that did not.
    let mut bound: Option<usize> = None;
    for member in received_types_of(kind, ty) {
        let member = member.as_str();
        match lookup_bound(&inputs.bounds, member) {
            Some(BoundState::Bounded { rx, .. }) => {
                bound = Some(bound.unwrap_or(0).max(*rx));
            }
            Some(BoundState::Unbounded { reason }) => {
                ep.refuse(
                    "wire_bound_bytes",
                    format!("`{member}` has no static bound: {reason}"),
                );
                bound = None;
                break;
            }
            Some(BoundState::Unresolved { reason }) => {
                ep.refuse(
                    "wire_bound_bytes",
                    format!("`{member}` was not priced: {reason}"),
                );
                bound = None;
                break;
            }
            None => {
                ep.refuse(
                    "wire_bound_bytes",
                    match &inputs.bounds_error {
                        Some(e) => format!("no bound inventory for this entry: {e}"),
                        None => format!(
                            "`{member}` is in no `nros_message_bounds.json` beside this entry -- \
                             run `nros sync` so codegen prices it"
                        ),
                    },
                );
                bound = None;
                break;
            }
        }
    }
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
        // issue 1393 — carry the bound's OWN refusal, so a reader holding the
        // row is not sent looking for why: on a model road with no registered
        // table that reason names the road, and on any road a type codegen
        // could not price names the type.
        let why = ep
            .wire_bound_bytes()
            .refusal()
            .unwrap_or("nothing stated it")
            .to_string();
        ep.refuse(
            "storage_bytes",
            format!(
                "the type's wire bound is not available ({why}), and a receive region is sized \
                 from it"
            ),
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
///
/// issue 1437 — `Unknown` reaches `None` by the SAME rule and not by accident:
/// it is what a `*_get_actual_qos` read-back writes for a policy the backend
/// could not report, so it is an absence too. It cannot arrive here today (a
/// descriptor is built from an AUTHORED profile, and `validate_against`
/// refuses a read-back profile as a request), which is exactly why it is
/// written out rather than swept into a `_ =>`: if a path ever does deliver
/// one, it must land on "nobody said" and not on a depth somebody derived a
/// buffer from.
fn map_history(h: nros_orchestration_ir::qos_override::QoSHistoryPolicy) -> Option<History> {
    use nros_orchestration_ir::qos_override::QoSHistoryPolicy as H;
    match h {
        H::SystemDefault => None,
        H::KeepLast => Some(History::KeepLast),
        H::KeepAll => Some(History::KeepAll),
        H::Unknown => None,
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
        R::Unknown => None,
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
        D::Unknown => None,
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

/// phase-461 W3 -- every type whose bound sizes THIS endpoint's receive side.
///
/// A topic endpoint receives its own type. A service or action endpoint does
/// not: it receives a REQUEST, and those are separate generated types with
/// separate bounds. The server side is what every pool in this tree is sized
/// from -- an inbox slot, a request buffer -- so the request is what is named
/// here for all four kinds, a client included. A client's reply buffer is the
/// other half and has no pool of its own yet; when it gets one it takes
/// `service_reply_type` and the response envelopes beside it.
///
/// # phase-457 W1 (issue 1506) -- an action's set is THREE, and one of them is
/// somebody else's type
///
/// An action server's `send_goal`, `cancel_goal` and `get_result` queryables all
/// draw from one ring (`nros-rmw-zenoh`'s `ACTION_INBOX`, selected by the
/// `/_action/` infix), so the slot has to hold the largest of the three. Naming
/// only `SendGoal_Request` was an UNDER-size and not a corner case: measured on
/// `examples/native/rust/action-server`, `CancelGoal_Request` is 44 bytes rx
/// against SendGoal's 36, because `GoalInfo` is bigger than a small goal
/// struct. `rosidl_codegen::action_received_types` names the three.
fn received_types_of(kind: EndpointKind, ty: &str) -> Vec<String> {
    match kind {
        EndpointKind::ServiceServer | EndpointKind::ServiceClient => {
            vec![rosidl_codegen::service_request_type(ty)]
        }
        EndpointKind::ActionServer | EndpointKind::ActionClient => {
            rosidl_codegen::action_received_types(ty).to_vec()
        }
        _ => vec![ty.to_string()],
    }
}

/// phase-457 W1 -- every type this endpoint's registration puts a DESCRIPTOR in
/// the image for, which is a wider set than [`received_types_of`].
///
/// `[types]`'s three maxima size the descriptor BUILDER's stack arrays
/// (`nros-rmw-cyclonedds`'s `MAX_FIELDS` / `MAX_KINDS` / `MAX_NESTED_DEPTH`),
/// and that builder runs once per REGISTERED type — not once per received one.
/// So the question here is "what schemas does this endpoint bring", and the
/// answer for an interface endpoint is never its interface name: `pkg/srv/Name`
/// and `pkg/action/Name` are not message types, codegen prices neither, and a
/// join on them refused both maxima on every service and action image while
/// telling the reader to run `nros sync` — a remedy that could not work,
/// because no amount of codegen produces a shape for a type codegen does not
/// emit.
///
/// A service brings its request AND its reply. An action brings its eight own
/// members plus the `action_msgs` protocol types `RosAction::register_protocol_types`
/// registers for it: `CancelGoal`'s two halves and the `GoalStatusArray` its
/// transient-local `~/_action/status` publisher carries (issue 1378). Omitting
/// those three would be the same under-size one level up — measured, they are
/// the DEEPEST schemas an action image holds (`CancelGoal_Response` kinds 11,
/// `GoalStatusArray` nested_depth 6, against 7 and 3 for the envelopes).
fn registered_types_of(kind: EndpointKind, ty: &str) -> Vec<String> {
    match kind {
        EndpointKind::ServiceServer | EndpointKind::ServiceClient => {
            rosidl_codegen::service_member_types(ty).to_vec()
        }
        EndpointKind::ActionServer | EndpointKind::ActionClient => {
            let mut v = rosidl_codegen::action_member_types(ty).to_vec();
            v.extend(rosidl_codegen::service_member_types(
                rosidl_codegen::ACTION_CANCEL_SERVICE,
            ));
            v.push(rosidl_codegen::ACTION_STATUS_TYPE.to_string());
            v
        }
        _ => vec![ty.to_string()],
    }
}

fn lookup_bound<'a>(bounds: &'a [(String, BoundState)], ty: &str) -> Option<&'a BoundState> {
    bounds.iter().find(|(n, _)| n == ty).map(|(_, b)| b)
}

/// Which of issue 1319's paths this endpoint's registration takes — three since
/// phase-456 W8, and none of them named for a caller.
///
/// # Two halves, and only one of them is an image fact
///
/// | half | who answers it |
/// | --- | --- |
/// | does the linked backend dispatch a sample IN PLACE? | the image — its `rmw` name |
/// | does the backend carry type descriptors? | the image — the same name |
/// | can THIS registration use an in-place dispatch? | the CALL SITE |
/// | did this site state the type's bound? | the call site, with the entry's LANGUAGE as evidence |
///
/// **phase-457 W3 made the third row a per-ENDPOINT fact, and it was the one
/// that was wrong.** Until this wave the in-place row was credited to every
/// endpoint of an image whose backend dispatches in place — the third question
/// was never asked. Measured against the executor, that is wrong for nine of its
/// eleven registration entry points: a Rust GENERIC subscription, a
/// `.message_info()` one, a `.safety()` one, a borrowed view, and four of the
/// five C/C++ entries all buffer. The over-statement was free while the row was
/// priced at the type's bound (issue 1319's fix); pricing it at what it actually
/// claims — nothing — makes it an UNDER-size, which is what issue 1340 was
/// waiting on.
///
/// So the third row is [`EntityDecl::in_place_capable`], OBSERVED by the probe
/// at `Executor::open_subscription`, and `None` REFUSES. It is never inferred
/// from the language: that inference is what this wave removes.
///
/// # What the language still buys, and where it runs out
///
/// The two BUFFERED rows are unchanged — the language remains evidence for
/// whether the site stated a bound, per phase-456 W7/W8. So a model-only
/// producer, which has no single entry language, can now state the in-place row
/// (it needs only the backend and the observation) and still refuses the other
/// two. That asymmetry is why the horizon is consulted HERE rather than
/// short-circuiting the whole field one layer up.
///
/// # The kinds this gate applies to
///
/// Every kind that RECEIVES, which is every kind but a publisher. Only a
/// subscription is PRICED from the row today (`Endpoint::claimed_slot_bytes` is
/// `Absent` for the rest), so gating subscriptions alone would cost nothing
/// measurable — and would be a reach narrower than the rule, which is the shape
/// issue 0196 keeps finding. A service server's request buffer is the same
/// question one kind over and has NO observation site, so those rows refuse too;
/// that is issue 1522's second half, and it is better said out loud than left as
/// a stated value nobody checked. A publisher serializes into a per-call
/// transmit buffer and claims no receive slot at all, so it has no registration
/// path to get wrong.
fn registration_path(
    kind: EndpointKind,
    inputs: &DescriptorInputs<'_>,
    observed_in_place_capable: Option<bool>,
) -> Result<RegistrationPath, String> {
    let Some(dispatch) = inputs.backend_dispatch else {
        return Err(registration_path_refusal(inputs));
    };
    // phase-454 W5 -- in-place wins over every other question, because the
    // capability test happens BEFORE any slot size is computed and returns
    // through an entry that carries no receive region.
    //
    // phase-457 W3 -- and it is now asked of the ENDPOINT. `Some(false)` falls
    // through to the buffered rows below, which is the whole point: a
    // registration that cannot dispatch in place on an in-place backend takes an
    // ordinary buffered region, and on a schemaless backend that is the
    // `unbounded` row at `RX_BUF` -- 1,848 bytes per subscription MORE than the
    // in-place row is priced at.
    if dispatch == BackendDispatch::InPlace {
        if kind == EndpointKind::Publisher {
            // The ONE kind with nothing to get wrong: a publisher serializes
            // into a per-call transmit buffer and claims no receive slot of any
            // shape, which is why `claimed_slot_bytes` is `Absent` for it. The
            // composed answer stands, so a publisher row reads the same on both
            // roads as it did before this wave.
            return Ok(RegistrationPath::InPlace);
        }
        match observed_in_place_capable {
            Some(true) => return Ok(RegistrationPath::InPlace),
            Some(false) => {}
            // Nobody observed it. A producer with a HORIZON says so in its own
            // words -- it already names the road and what that road reads, which
            // is the more useful diagnosis than "nothing observed it" for a
            // reader holding the file. Only the full probe road, which HAS an
            // observation channel and did not get an answer down it, falls to
            // the fact-specific reason.
            None => {
                return Err(match &inputs.horizon {
                    Some(h) => h.registration_path(),
                    None => unobserved_registration_refusal(inputs),
                });
            }
        }
    }
    let Some(schema) = inputs.backend_schema else {
        return Err(registration_path_refusal(inputs));
    };
    // issue 1393 — a descriptor-carrying backend answers WITHOUT the language.
    // Both language arms of the table below give `TypedBound` there (a C/C++
    // site supplies `rx_size_bound<M>`; a Rust one reaches the bound through
    // `MessageForRmw`'s descriptor), so asking for the language first refused a
    // fact every possible answer agreed on. That was the model road's whole
    // refusal for a Cyclone image: several packages, no ONE entry language, and
    // no language needed.
    if schema == BackendSchema::Descriptors {
        return Ok(RegistrationPath::TypedBound);
    }
    let Some(language) = inputs.language else {
        // A schemaless backend, and the language decides the row: a Rust
        // registration takes `RX_BUF`, a C/C++ one its typed hint. On the model
        // road that is unreachable today -- every row there is UNOBSERVED, so it
        // refused above -- and when issue 1594 joins the probe's observation in,
        // it is the probe that knows the component's language, not this road.
        return Err(registration_path_refusal(inputs));
    };
    Ok(match (language, schema) {
        // A C/C++ entry that registers typed supplies `rx_size_bound<M>`, so it
        // is credited with a stated bound. That credit is an ASSUMPTION about
        // call sites this writer cannot see, and phase-456 W7 narrowed how far
        // it reaches: every registration site in the nros-cpp headers now states
        // a bound, enforced by `check-cpp-subscription-bound-supplied`, so a C++
        // entry earns it unless it calls the one deliberately type-erased site
        // (`nros::bind_subscription_raw`, which passes the named
        // `nros::rx_bound_unknown`). The C API's own helper
        // (`nros_cpp_subscription_register_hinted`) has always required the
        // hint. What remains untrue is CONSUMER code passing `options = NULL`
        // with the type in scope -- seven C example listeners do, which is
        // issue 1376.
        (EntryLanguage::CFamily, _) => RegistrationPath::TypedBound,
        // Rust against a descriptor-carrying backend: the bound is reachable
        // from `MessageForRmw`, so the registration is priced at the type.
        (EntryLanguage::Rust, BackendSchema::Descriptors) => RegistrationPath::TypedBound,
        // Rust against a schemaless backend where this registration does NOT
        // dispatch in place: no schema, so no bound exists to state at the
        // type-erased site, and the registration takes `RX_BUF`.
        (EntryLanguage::Rust, BackendSchema::Schemaless) => RegistrationPath::Unbounded,
    })
}

/// phase-457 W3 — the refusal when the backend dispatches in place and NOTHING
/// OBSERVED this endpoint's registration.
///
/// Separate prose from [`registration_path_refusal`] because the missing input is
/// a different kind of thing: the halves that one reports are facts about the
/// IMAGE that some road can supply, while this one is a fact about the SOURCE
/// that only a probe which actually registers can report. Telling a reader "the
/// backend is unknown" when the backend is perfectly well known would aim them
/// at the wrong artifact.
fn unobserved_registration_refusal(inputs: &DescriptorInputs<'_>) -> String {
    let rmw = inputs.rmw.as_deref().unwrap_or("this image's backend");
    format!(
        "backend `{rmw}` dispatches a received sample IN PLACE, but nothing observed whether THIS \
         endpoint's registration can use that -- nine of the executor's eleven subscription entry \
         points cannot, and which one an endpoint calls is a property of the source that no \
         declaration carries. The fact is reported by the metadata probe at \
         `Executor::open_subscription`; a road whose probe DECLARES without registering leaves it \
         unobserved. Refused rather than assumed, because an endpoint credited with in-place \
         dispatch is priced at NO receive region at all (issue 1340). Tracked by issue 1522"
    )
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
            // Issue 1577 — the per-kind counts the executor arena's model sums,
            // from the SAME `per_kind` the cmake road emits as
            // `NROS_ENTITY_COUNT_*`. `derive` seeds every kind at zero, so a
            // missing tag is a kind this derivation does not know — refused,
            // never stated as a zero it did not count.
            for (field, kind) in ENTITY_COUNT_FIELDS {
                match k.per_kind.get(kind.tag()) {
                    Some(&n) => set_entity_count(&mut img, field, n),
                    None => {
                        img.refuse(
                            field,
                            format!(
                                "the entity inventory's per-kind table has no `{}` row",
                                kind.tag()
                            ),
                        );
                    }
                }
            }
        }
        Some(Derivation::Refused { reason }) => {
            img.refuse("node_count", reason.clone())
                .refuse("subscriber_count", reason.clone());
            for (field, _) in ENTITY_COUNT_FIELDS {
                img.refuse(field, reason.clone());
            }
        }
        None => {
            let why = "the entity inventory did not compose for this entry, so the image's \
                       node, subscriber and entity counts are not known -- absence is not zero";
            img.refuse("node_count", why)
                .refuse("subscriber_count", why);
            for (field, _) in ENTITY_COUNT_FIELDS {
                img.refuse(field, why);
            }
        }
    }
    img
}

/// `[image] *_entities` ↔ the entity kind each counts — issue 1577. The seven
/// kinds `nros-node`'s arena model sums (issue 0810 added service clients and
/// guard conditions, which do claim arena); a publisher costs it nothing.
const ENTITY_COUNT_FIELDS: [(&str, EntityKind); 7] = [
    ("subscription_entities", EntityKind::Subscription),
    ("timer_entities", EntityKind::Timer),
    ("service_server_entities", EntityKind::ServiceServer),
    ("action_client_entities", EntityKind::ActionClient),
    ("action_server_entities", EntityKind::ActionServer),
    // Issue 0810 — both claim an arena entry; the model that summed five kinds
    // priced these two at zero.
    ("service_client_entities", EntityKind::ServiceClient),
    ("guard_condition_entities", EntityKind::GuardCondition),
];

fn set_entity_count(img: &mut nros_sizing_descriptor::Image, field: &str, n: usize) {
    let v = Some(n);
    match field {
        "subscription_entities" => img.set_subscription_entities(v),
        "timer_entities" => img.set_timer_entities(v),
        "service_server_entities" => img.set_service_server_entities(v),
        "action_client_entities" => img.set_action_client_entities(v),
        "action_server_entities" => img.set_action_server_entities(v),
        "service_client_entities" => img.set_service_client_entities(v),
        "guard_condition_entities" => img.set_guard_condition_entities(v),
        _ => unreachable!("not an entity-count field: {field}"),
    };
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

    // `distinct_count` counts the DECLARED interfaces, one per distinct
    // `[[endpoint]] type`, and deliberately not the registered message types
    // `schemas` walks below. The registered COUNT has its own producer —
    // `nros_orchestration_ir::cyclonedds_type_sizing`, which resolves
    // `NROS_CYCLONEDDS_MAX_TYPES` from the SystemModel (a srv is 2, an action
    // 8 + 3) — and a second answer to that question here is what the
    // single-writer rule beside `cyclonedds_env` exists to refuse.
    let mut names: Vec<&str> = endpoints.iter().map(|e| e.type_name.as_str()).collect();
    names.sort_unstable();
    names.dedup();

    // UNFLOORED -- D7. An image with no endpoints declares zero distinct
    // types, and zero is the answer rather than a number to round up.
    let distinct_count = names.len();

    // phase-457 W1 -- the SHAPE is joined on the types the image REGISTERS, not
    // on the interfaces it declares. See `registered_types_of`: an interface
    // name has no schema and never will, so before this the maxima were refused
    // on every service and action image.
    let mut schemas: Vec<String> = endpoints
        .iter()
        .flat_map(|e| registered_types_of(e.kind, &e.type_name))
        .collect();
    schemas.sort_unstable();
    schemas.dedup();

    let mut shape = rosidl_codegen::schema_value::SchemaShape::default();
    let mut unshaped: Vec<&str> = Vec::new();
    for ty in &schemas {
        match inputs
            .schema_shapes
            .iter()
            .find(|(n, _)| n == ty)
            .and_then(|(_, s)| *s)
        {
            Some(s) => shape = shape.max(s),
            None => unshaped.push(ty.as_str()),
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
                 resolve a nested type, or the `generated/` tree predates this field. These \
                 are the MESSAGE types this entry's endpoints register (a service's two \
                 halves, an action's eight members plus the `action_msgs` protocol types), \
                 not the interface names the `[[endpoint]]` rows carry. Run `nros sync` so \
                 codegen walks them; a maximum over the types that DO have one would \
                 under-size the descriptor builder's stack arrays silently",
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
            Ok(rows) => {
                let (bounds, shapes) = crate::leaf_payload_classes::project_bound_rows(rows);
                (bounds, shapes, None)
            }
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
    /// phase-457-payload W2 — the per-package BOUND tables this image's
    /// interface closure links: the `nros_message_bounds.json` codegen emitted
    /// beside each registered `nros_message_bounds.cmake` fragment.
    ///
    /// Read through [`crate::leaf_payload_classes::bound_rows_from_tables`] —
    /// the SAME reader the leaf road uses over its `generated/` tree — so the
    /// two roads cannot come to describe one table differently. Exporting the
    /// closure codegen already walked, rather than re-deriving a bound here, is
    /// the decision the phase recorded: a second derivation is a second opinion
    /// about a bound (issue 0196's class).
    ///
    /// EMPTY means "this road was handed no tables", and refuses the payload
    /// class with [`ModelHorizon::bound_inventory`]'s reason — the closure
    /// registered none. It is not a claim that the closure has no types.
    pub bound_inventories: &'a [std::path::PathBuf],
    /// This producer's HORIZON — which input it read, and the road, in prose,
    /// for every refusal it writes.
    ///
    /// phase-457 W0.b — a field rather than a `road: &str` the function turns
    /// into a horizon, because there are now TWO inputs a road can have and only
    /// the caller knows which it holds. A `bool` beside the road would leave the
    /// two spellings one negation apart at every call site.
    pub horizon: ModelHorizon,
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
    let horizon = img.horizon.clone();
    // phase-457-payload W2 — the closure's bound tables, when the caller has
    // them, through the SAME reader and projection the leaf road uses. None
    // handed over refuses with the horizon's reason (nothing registered); a pending
    // or malformed table is a refusal naming THAT table (see
    // `bound_rows_from_tables`), never an error, because a table not built yet
    // is the ordinary first-configure state of the non-Zephyr cmake lane.
    let (bounds, schema_shapes, bounds_error) = if img.bound_inventories.is_empty() {
        (Vec::new(), Vec::new(), Some(horizon.bound_inventory()))
    } else {
        match crate::leaf_payload_classes::bound_rows_from_tables(img.bound_inventories) {
            Ok(rows) => {
                let (bounds, shapes) = crate::leaf_payload_classes::project_bound_rows(rows);
                (bounds, shapes, None)
            }
            Err(e) => (Vec::new(), Vec::new(), Some(e)),
        }
    };
    let inputs = DescriptorInputs {
        entry: img.entry.to_string(),
        inventory: Some(img.inventory),
        // `bounds_error` is not an error channel -- it is the one place the
        // existing composer already asks "why is there no bound table", and
        // answering it is what carries the reason into `wire_bound_bytes` and
        // `[types]`'s three maxima without a second refusal path for either.
        bounds,
        schema_shapes,
        bounds_error,
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
        // No ONE entry language: this road is several packages. Since issue
        // 1393 that costs nothing a backend can answer without it — on a
        // descriptor-carrying backend every language gives `typed_bound`, and
        // on an in-place one the row turns on the OBSERVATION first (which
        // this road does not have yet: issue 1594).
        language: None,
        // phase-457 W3 — the BACKEND halves are supplied, and they always could
        // have been: they are a function of `rmw` alone, which this road has.
        // Withholding them was W14 being conservative about a field whose
        // in-place row was inferred from the language; now that the row is
        // OBSERVED per endpoint, the backend plus the observation is the whole
        // answer, so a model image states `in_place` for exactly the endpoints
        // an observation reaches it for.
        backend_schema: backend_schema(img.rmw.as_deref().unwrap_or_default()),
        backend_dispatch: backend_dispatch(img.rmw.as_deref().unwrap_or_default()),
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

    /// A subscription row whose registration WAS observed, and was in-place
    /// capable.
    ///
    /// phase-457 W3 -- the default is `Some(true)` rather than `None` because
    /// almost every test in this file is about some OTHER fact and wants the
    /// `in_place` row it has always asserted. `Some(true)` is the honest shape of
    /// a probed image whose subscription takes the typed Rust path or the C++
    /// plain one; a test about the OBSERVATION itself states its own value
    /// through [`sub_observed`].
    fn sub(ty: &str, topic: &str, depth: Option<u32>) -> EntityDecl {
        sub_observed(ty, topic, depth, Some(true))
    }

    /// phase-457 W3 -- a subscription row with its registration fact stated
    /// explicitly. `None` is "nobody observed it", which REFUSES the path.
    fn sub_observed(
        ty: &str,
        topic: &str,
        depth: Option<u32>,
        in_place_capable: Option<bool>,
    ) -> EntityDecl {
        let mut d = EntityDecl::bare(
            EntityKind::Subscription,
            Some(ty.into()),
            Some(topic.into()),
        );
        d.depth = depth;
        d.history = Some(QoSHistoryPolicy::KeepLast);
        d.in_place_capable = in_place_capable;
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

    /// Issue 1572 -- the declared road's BOUND. A silent publisher counts as
    /// transient-local; a stated volatile one costs nothing; a stated count is
    /// its own bound.
    #[test]
    fn declared_entities_bound_a_refused_transient_local_count_from_above() {
        let decl = |k: EntityKind, ty: &str, name: &str| {
            EntityDecl::bare(k, Some(ty.into()), Some(name.into()))
        };
        let with = |name: &str, d: QoSDurabilityPolicy| {
            let mut p = decl(EntityKind::Publisher, "std_msgs/msg/String", name);
            p.durability = Some(d);
            p
        };
        let silent = decl(EntityKind::Publisher, "std_msgs/msg/String", "/quiet");
        let rows = [
            with("/latched", QoSDurabilityPolicy::TransientLocal),
            with("/chatter", QoSDurabilityPolicy::Volatile),
            silent,
            decl(
                EntityKind::ServiceServer,
                "example_interfaces/srv/AddTwoInts",
                "/add",
            ),
        ];
        assert!(matches!(
            transient_local_publishers_from_decls(&rows),
            nros_sizing_descriptor::Fact::Refused(_)
        ));
        assert_eq!(transient_local_publishers_bound_from_decls(&rows), Some(2));
        assert_eq!(
            transient_local_publishers_bound_from_decls(&rows[..2]),
            Some(1),
            "a stated count is its own bound"
        );
        assert_eq!(
            transient_local_publishers_bound_from_decls(&[EntityDecl::bare(
                EntityKind::Timer,
                None,
                None
            )]),
            Some(0)
        );
        assert_eq!(transient_local_publishers_bound_from_decls(&[]), None);
    }

    /// phase-461 W3 -- the join W6.a's own header records as refusing on every
    /// in-tree image: a service row's `type` is the INTERFACE, and the bound
    /// belongs to the REQUEST. Now that codegen prices `_Request`, the row is
    /// STATED and `declared_service_request_bytes` in the zenoh build script
    /// has a number to derive from.
    #[test]
    fn a_service_row_joins_on_its_request_type_not_its_interface() {
        use nros_sizing_descriptor::Fact;
        let srv = EntityDecl::bare(
            EntityKind::ServiceServer,
            Some("tier4_system_msgs/srv/OperateMrm".into()),
            Some("/operate_mrm".into()),
        );
        let act = EntityDecl::bare(
            EntityKind::ActionServer,
            Some("example_interfaces/action/Fibonacci".into()),
            Some("/fibonacci".into()),
        );
        let inv = inventory(vec![srv, act]);
        let mut inputs = base(&inv);
        inputs.bounds = vec![
            // The INTERFACE spelling, which nothing prices and nothing should
            // read -- present here precisely so the assertion below is about
            // the join and not about an empty table.
            bounded("tier4_system_msgs/srv/OperateMrm", 9999),
            bounded("tier4_system_msgs/srv/OperateMrm_Request", 24),
            // phase-457 W1 (issue 1506) -- an action row is a MAXIMUM over the
            // three requests its queryables share one ring for, and the biggest
            // of them here is somebody else's type. Before W1 the row read 28.
            bounded("example_interfaces/action/Fibonacci_SendGoal_Request", 28),
            bounded("action_msgs/srv/CancelGoal_Request", 44),
            bounded("example_interfaces/action/Fibonacci_GetResult_Request", 20),
        ];
        let desc = build(&inputs);
        let row = |topic: &str| {
            desc.endpoints
                .iter()
                .find(|e| e.topic == topic)
                .unwrap_or_else(|| panic!("no row for {topic}"))
                .wire_bound_bytes()
        };
        assert_eq!(row("/operate_mrm"), Fact::Stated(24));
        assert_eq!(
            row("/fibonacci"),
            Fact::Stated(44),
            "the cancel queryable draws from the same ring, so the slot holds its request"
        );
    }

    /// phase-457 W1 (issue 1506) -- the reproduction that fails first: an action
    /// row whose `CancelGoal_Request` is unpriced REFUSES, rather than stating a
    /// maximum over the two members that answered.
    ///
    /// The direction matters. A maximum over a subset of a SHARED pool's
    /// population is an under-size, and an under-sized zenoh inbox slot drops
    /// the request as `TransportError::MessageTooLarge` — so the refusal is the
    /// only answer, and it names the member the reader has to price.
    #[test]
    fn an_action_row_refuses_when_one_of_its_three_requests_is_unpriced() {
        let inv = inventory(vec![EntityDecl::bare(
            EntityKind::ActionServer,
            Some("p/action/A".into()),
            Some("/a".into()),
        )]);
        let mut inputs = base(&inv);
        inputs.bounds = vec![
            bounded("p/action/A_SendGoal_Request", 36),
            bounded("p/action/A_GetResult_Request", 28),
            // `action_msgs/srv/CancelGoal_Request` deliberately absent.
        ];
        let desc = build(&inputs);
        let reason = desc.endpoints[0]
            .wire_bound_bytes()
            .refusal()
            .expect("an unpriced cancel request refuses the whole row")
            .to_string();
        assert!(
            reason.contains("action_msgs/srv/CancelGoal_Request"),
            "{reason}"
        );
    }

    /// A service whose REQUEST is unpriced still refuses, and the refusal names
    /// the request type rather than the interface -- which is where the reader
    /// has to go to fix it.
    #[test]
    fn an_unpriced_request_refuses_naming_the_request_type() {
        let inv = inventory(vec![EntityDecl::bare(
            EntityKind::ServiceServer,
            Some("p/srv/S".into()),
            Some("/s".into()),
        )]);
        let inputs = base(&inv);
        let desc = build(&inputs);
        let bound = desc.endpoints[0].wire_bound_bytes();
        let reason = bound.refusal().expect("an unpriced request refuses");
        assert!(reason.contains("p/srv/S_Request"), "{reason}");
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

    /// The narrowing, asserted (phase-469).
    ///
    /// Both halves matter. Rust must stay its own answer, and C and C++ must
    /// land on ONE — that collapse is the information [`EntryLanguage`]
    /// carries over [`nros_lang::Language`], and a `From` that spread them
    /// apart would be a silently different registration table.
    #[test]
    fn the_registration_narrowing_folds_c_and_cpp_and_keeps_rust_apart() {
        assert_eq!(
            EntryLanguage::from(nros_lang::Language::Rust),
            EntryLanguage::Rust
        );
        assert_eq!(
            EntryLanguage::from(nros_lang::Language::C),
            EntryLanguage::CFamily
        );
        assert_eq!(
            EntryLanguage::from(nros_lang::Language::Cpp),
            EntryLanguage::CFamily
        );
        // Every language in the enumeration has an answer — the `From` is
        // total, so a new variant is a compile error there rather than a
        // registration path nobody chose.
        assert_eq!(
            nros_lang::Language::ALL
                .iter()
                .filter(|l| EntryLanguage::from(**l) == EntryLanguage::CFamily)
                .count(),
            2
        );
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
            // phase-457 W3 -- the BACKEND halves are NOT subtracted, because
            // `write_for_model` supplies them: they are a function of `rmw`
            // alone, which that road has. The two producers now differ by the
            // LANGUAGE and the leaf's three inventories, and nothing else.
            horizon: Some(horizon),
            ..base(inv)
        }
    }

    /// The model road as it runs since phase-457-payload W2: handed the bound
    /// tables its closure registered. Differs from [`base`] by the language and
    /// the horizon ONLY, which is what issue 1393's closure claims.
    fn model_with_tables<'a>(inv: &'a EntityInventory) -> DescriptorInputs<'a> {
        let b = base(inv);
        DescriptorInputs {
            bounds: b.bounds,
            schema_shapes: b.schema_shapes,
            bounds_error: None,
            ..model_only(inv)
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
            Some(&RegistrationPath::InPlace)
        );
        assert_eq!(d.meta.status, Status::Derived);
    }

    /// The dispatch axis decides, and it decides BEFORE the schema question —
    /// the same order the executor's one consulting site asks them in. Since
    /// phase-456 W8 it also decides BEFORE the language, because the C
    /// registration path consults the same capability.
    #[test]
    fn the_registration_path_reads_dispatch_before_schema() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        for (schema, dispatch, want) in [
            (
                BackendSchema::Schemaless,
                BackendDispatch::InPlace,
                RegistrationPath::InPlace,
            ),
            (
                BackendSchema::Descriptors,
                BackendDispatch::InPlace,
                RegistrationPath::InPlace,
            ),
            (
                BackendSchema::Schemaless,
                BackendDispatch::Buffered,
                RegistrationPath::Unbounded,
            ),
            (
                BackendSchema::Descriptors,
                BackendDispatch::Buffered,
                RegistrationPath::TypedBound,
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
        // phase-456 W8 — a C/C++ entry takes the SAME rows a Rust one does. It
        // is credited with a stated bound when the backend buffers (W7), and it
        // reaches the in-place row when the backend dispatches in place, which
        // it could not before W8 gave the capability one consulting site.
        for (dispatch, want) in [
            (BackendDispatch::InPlace, RegistrationPath::InPlace),
            (BackendDispatch::Buffered, RegistrationPath::TypedBound),
        ] {
            let mut i = base(&inv);
            i.language = Some(EntryLanguage::CFamily);
            i.backend_dispatch = Some(dispatch);
            let d = build(&i);
            assert_eq!(
                d.endpoints[0].registration_path().stated(),
                Some(&want),
                "C family + {dispatch:?}"
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

    /// phase-457 W1 -- a SERVICE row's schema shape comes from its two member
    /// messages, never from its interface name.
    ///
    /// The reproduction that fails first: before this wave the join used
    /// `pkg/srv/Name`, which codegen prices under no circumstances, so all three
    /// maxima were refused on every service image — with a reason that told the
    /// reader to run `nros sync`, a remedy that could not work.
    #[test]
    fn a_service_row_takes_its_shape_from_both_member_messages() {
        let inv = inventory(vec![EntityDecl::bare(
            EntityKind::ServiceServer,
            Some("p/srv/S".into()),
            Some("/s".into()),
        )]);
        let mut i = base(&inv);
        i.bounds = vec![bounded("p/srv/S_Request", 24)];
        i.schema_shapes = vec![
            // The interface name, which nothing should read.
            shaped("p/srv/S", 99, 99, 99),
            shaped("p/srv/S_Request", 4, 4, 1),
            // The REPLY is registered too, and here it is the deeper of the two.
            shaped("p/srv/S_Response", 3, 6, 3),
        ];
        let d = build(&i);
        assert_eq!(d.types.max_fields().stated(), Some(&4), "from the request");
        assert_eq!(d.types.max_kinds().stated(), Some(&6), "from the reply");
        assert_eq!(
            d.types.max_nested_depth().stated(),
            Some(&3),
            "from the reply"
        );
    }

    /// phase-457 W1 -- an ACTION row's shape covers the eleven types its
    /// registration puts a descriptor in the image for.
    ///
    /// Measured on `examples/native/rust/action-server`, the DEEPEST of those is
    /// never one of the action's own: `action_msgs/srv/CancelGoal_Response` has
    /// 11 kinds and `GoalStatusArray` nests 6 deep, against 7 and 3 for the
    /// envelopes. So a maximum over the action's own members alone is an
    /// under-size, in the direction that fails type registration at boot.
    #[test]
    fn an_action_row_takes_its_shape_from_the_protocol_types_too() {
        let inv = inventory(vec![EntityDecl::bare(
            EntityKind::ActionServer,
            Some("p/action/A".into()),
            Some("/a".into()),
        )]);
        let mut i = base(&inv);
        i.bounds = vec![
            bounded("p/action/A_SendGoal_Request", 36),
            bounded("p/action/A_GetResult_Request", 28),
            bounded("action_msgs/srv/CancelGoal_Request", 44),
        ];
        let own = [
            "_SendGoal_Request",
            "_SendGoal_Response",
            "_GetResult_Request",
            "_GetResult_Response",
            "_FeedbackMessage",
            "_Goal",
            "_Result",
            "_Feedback",
        ];
        i.schema_shapes = own
            .iter()
            .map(|s| shaped(&format!("p/action/A{s}"), 4, 7, 3))
            .chain([
                shaped("action_msgs/srv/CancelGoal_Request", 3, 9, 4),
                shaped("action_msgs/srv/CancelGoal_Response", 4, 11, 5),
                shaped("action_msgs/msg/GoalStatusArray", 1, 10, 6),
            ])
            .collect();
        let d = build(&i);
        assert_eq!(d.types.max_fields().stated(), Some(&4));
        assert_eq!(
            d.types.max_kinds().stated(),
            Some(&11),
            "from CancelGoal_Response, which no envelope reaches"
        );
        assert_eq!(
            d.types.max_nested_depth().stated(),
            Some(&6),
            "from GoalStatusArray, the status publisher's type (issue 1378)"
        );

        // And a missing protocol shape refuses rather than quietly reporting the
        // envelopes' maximum.
        i.schema_shapes
            .retain(|(n, _)| n != "action_msgs/msg/GoalStatusArray");
        let why = build(&i)
            .types
            .max_nested_depth()
            .refusal()
            .expect("an unshaped registered type refuses")
            .to_string();
        assert!(why.contains("action_msgs/msg/GoalStatusArray"), "{why}");
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
        // BOTH halves, which is what an unknown `rmw` really gives: they are two
        // reads of one name (`backend_schema` / `backend_dispatch`), so a
        // configuration with one and not the other is not an input any producer
        // can hand this code. phase-457 W3 made that matter -- clearing only the
        // SCHEMA leaves the in-place row perfectly answerable, because that row
        // is the dispatch axis plus the endpoint's own observation and the schema
        // takes no part in it.
        i.backend_schema = None;
        i.backend_dispatch = None;
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
        // Issue 1577 — the ENTITY counts the executor arena sums are the other
        // reading of the same image: one subscription, one action client, and
        // the action client's feedback slot counted nowhere a second time.
        // Reading `subscriber_count` as the subscription count would price that
        // action client twice.
        assert_eq!(d.image.subscription_entities().stated(), Some(&1));
        assert_eq!(d.image.action_client_entities().stated(), Some(&1));
        assert_eq!(d.image.timer_entities().stated(), Some(&0));
        assert_eq!(d.image.service_server_entities().stated(), Some(&0));
        assert_eq!(d.image.action_server_entities().stated(), Some(&0));
        // Issue 0810 — the two kinds the five-kind model priced at zero are
        // STATED too, so a consumer can tell "none" from "not counted".
        assert_eq!(d.image.service_client_entities().stated(), Some(&0));
        assert_eq!(d.image.guard_condition_entities().stated(), Some(&0));
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
        // Issue 1577 — and the five entity counts, which the arena model reads
        // as "declared nothing" only if they were STATED as zero.
        for f in [
            d.image.subscription_entities(),
            d.image.timer_entities(),
            d.image.service_server_entities(),
            d.image.action_client_entities(),
            d.image.action_server_entities(),
            d.image.service_client_entities(),
            d.image.guard_condition_entities(),
        ] {
            assert!(
                f.refusal().unwrap().contains("absence is not zero"),
                "{f:?}"
            );
        }
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

    // --- phase-457 W3: the registration path, per ENDPOINT ------------------

    /// **The reproduction, and it FAILS FIRST.** Two subscriptions, one image,
    /// one backend that dispatches in place — and they do NOT take the same
    /// registration path.
    ///
    /// Before W3 the in-place row was composed from the `rmw` name alone, so both
    /// rows read `in_place` and both would be priced at NO receive region.
    /// `executor::tests::a_generic_subscription_on_an_in_place_backend_still_claims_a_full_region`
    /// is what the second one costs at run time: `NodeError::BufferTooSmall` at a
    /// registration the arena oracle passed. Mutation-tested by restoring the
    /// pre-W3 arm (`(_, _, InPlace) => InPlace`), which reds on `/generic`.
    ///
    /// The observed `false` does not become a refusal: it FALLS THROUGH to the
    /// buffered rows, and on a schemaless backend that is `unbounded` at `RX_BUF`
    /// — 1,848 bytes per subscription MORE than the in-place row is priced at.
    /// Refusing it would have been the safe direction too and a worse answer,
    /// because a consumer can size from a stated row and cannot size from a
    /// refused one.
    #[test]
    fn two_subscriptions_on_one_in_place_backend_take_different_paths() {
        let inv = inventory(vec![
            sub_observed("std_msgs/msg/String", "/typed", Some(1), Some(true)),
            sub_observed("std_msgs/msg/String", "/generic", Some(1), Some(false)),
        ]);
        let d = build(&base(&inv));
        let by_topic = |t: &str| {
            d.endpoints
                .iter()
                .find(|e| e.topic == t)
                .unwrap_or_else(|| panic!("no row for {t}"))
        };
        assert_eq!(
            by_topic("/typed").registration_path().stated(),
            Some(&RegistrationPath::InPlace),
            "an OBSERVED in-place-capable registration on an in-place backend is \
             the in-place row"
        );
        assert_eq!(
            by_topic("/generic").registration_path().stated(),
            Some(&RegistrationPath::Unbounded),
            "the same backend, a shape that cannot dispatch in place, and a site \
             that states no bound: `RX_BUF`. NOT `in_place`, which is the \
             under-size issue 1340 was blocked on"
        );
        // And the pricing follows, which is where the bytes are.
        assert!(
            by_topic("/typed").claims_no_receive_region(),
            "the in-place row claims no receive region -- issue 1340's saving"
        );
        assert!(
            !by_topic("/generic").claims_no_receive_region(),
            "and the generic row keeps its region, which is what stops the \
             saving from being an under-size"
        );
    }

    /// An endpoint NOBODY OBSERVED is refused, not credited.
    ///
    /// The direction that matters: `None` must never read as "in place". It is the
    /// state of every row on a road whose probe declares without registering, and
    /// of every row the `ENTITIES` grammar or a launch declaration produced.
    #[test]
    fn an_unobserved_endpoint_refuses_the_path_rather_than_claiming_in_place() {
        let inv = inventory(vec![sub_observed(
            "std_msgs/msg/String",
            "/chatter",
            Some(1),
            None,
        )]);
        let d = build(&base(&inv));
        let ep = &d.endpoints[0];
        let path = ep.registration_path();
        let why = path
            .refusal()
            .expect("an unobserved endpoint on an in-place backend must REFUSE");
        assert!(why.contains("issue 1522"), "{why}");
        assert!(
            !ep.claims_no_receive_region(),
            "a refused path must budget the region -- the one direction that \
             cannot ship BufferTooSmall"
        );
        // The safe direction, loudly: the slot falls to the closure buffer.
        assert!(ep.may_claim_closure_buffer());
    }

    /// The BUFFERED backend arm is untouched by the observation.
    ///
    /// A backend that buffers buffers whatever the delivery shape, so the two
    /// non-in-place rows are decided exactly as phase-456 W8 left them. Asserted
    /// over all three observation states so a future reader cannot mistake the
    /// new input for a fourth axis on every row.
    #[test]
    fn a_buffering_backend_ignores_the_observation() {
        for observed in [Some(true), Some(false), None] {
            let inv = inventory(vec![sub_observed(
                "std_msgs/msg/String",
                "/chatter",
                Some(1),
                observed,
            )]);
            let mut i = base(&inv);
            i.backend_dispatch = Some(BackendDispatch::Buffered);
            let d = build(&i);
            assert_eq!(
                d.endpoints[0].registration_path().stated(),
                Some(&RegistrationPath::Unbounded),
                "Rust + schemaless + buffering is the `unbounded` row whatever \
                 the observation says ({observed:?})"
            );
        }
    }

    /// **The W3 unlock.** A MODEL-only producer can now state the in-place row,
    /// and it needs no entry language to do it.
    ///
    /// phase-454 W14 refused `registration_path` outright on this road because the
    /// in-place row was inferred from the entry's LANGUAGE and a model image is
    /// several packages. The row is an OBSERVATION now, so the two halves that
    /// remain are the backend (a function of `rmw`, which this road has) and the
    /// endpoint's own fact (from the probe's metadata, which phase-457 W0
    /// composed in). The two BUFFERED rows still need the language and still
    /// refuse with the horizon's own prose — `a_buffering_backend_ignores_the_observation`
    /// pins the leaf road's answer for the same pair.
    ///
    /// This is what makes issue 1340's saving reachable at all: the arena's
    /// per-endpoint sum runs only where every `NROS_ENTITY_COUNT_*` arrives, which
    /// is the cmake/Zephyr road — and that road's descriptor is written by this
    /// producer.
    #[test]
    fn an_observed_row_lets_the_model_road_state_the_in_place_path() {
        let inv = inventory(vec![
            sub_observed("std_msgs/msg/String", "/typed", Some(1), Some(true)),
            sub_observed("std_msgs/msg/String", "/generic", Some(1), Some(false)),
        ]);
        let d = build(&model_only(&inv));
        let by_topic = |t: &str| d.endpoints.iter().find(|e| e.topic == t).expect(t);
        assert_eq!(
            by_topic("/typed").registration_path().stated(),
            Some(&RegistrationPath::InPlace),
            "the observation plus the backend is the whole of the in-place row, \
             and this road has both"
        );
        // The other row is a BUFFERED row on a schemaless backend, where the
        // language decides it (Rust `unbounded`, C/C++ `typed_bound`) and this
        // road has none -- so it refuses, naming the missing half. No road-wide
        // reason: issue 1393 closed that.
        let path = by_topic("/generic").registration_path();
        let why = path
            .refusal()
            .expect("a schemaless buffered row still needs the entry language");
        assert!(why.contains("the entry's language"), "{why}");
        assert!(!why.contains("issue 1393"), "{why}");
    }

    /// issue 1393 — a descriptor-carrying backend answers `registration_path`
    /// WITHOUT the language, on every road, and the two roads AGREE.
    ///
    /// Both language arms give `typed_bound` on Cyclone, so the model road's
    /// refusal there ("several packages, no one entry language") refused a fact
    /// every possible answer agreed on. Asserted against the leaf road for BOTH
    /// languages and every observation, so the shortcut cannot drift from the
    /// table it short-circuits.
    #[test]
    fn a_descriptor_backend_states_the_path_without_a_language_and_both_roads_agree() {
        for observed in [Some(true), Some(false), None] {
            let inv = inventory(vec![sub_observed(
                "std_msgs/msg/String",
                "/chatter",
                Some(2),
                observed,
            )]);
            let cyclone = |mut i: DescriptorInputs<'_>| {
                i.backend_schema = Some(BackendSchema::Descriptors);
                i.backend_dispatch = Some(BackendDispatch::Buffered);
                i.rmw = Some("cyclonedds".into());
                build(&i).endpoints[0].registration_path()
            };
            let model = cyclone(model_only(&inv));
            assert_eq!(
                model.stated(),
                Some(&RegistrationPath::TypedBound),
                "{observed:?}: {:?}",
                model.refusal()
            );
            for language in [EntryLanguage::Rust, EntryLanguage::CFamily] {
                let mut leaf = base(&inv);
                leaf.language = Some(language);
                assert_eq!(cyclone(leaf), model, "{language:?} / {observed:?}");
            }
        }
    }

    /// issue 1393 — `storage_bytes` is composed by ONE chain on every road, so a
    /// model road handed the bound tables and the triple states exactly the
    /// region the leaf road states.
    ///
    /// The reproduction, watched failing first: phase-454 W14's horizon refused
    /// this field outright, with a reason claiming the region had "neither of its
    /// two sizes", on a row that stated `wire_bound_bytes` and whose descriptor
    /// stated `[target] pointer_bytes` — `examples/workspaces/cpp`'s
    /// `native_entry.toml` carried exactly that pair.
    #[test]
    fn handed_the_tables_the_model_road_states_the_region_the_leaf_road_states() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let leaf = build(&base(&inv));
        let model = build(&model_with_tables(&inv));
        let (l, m) = (&leaf.endpoints[0], &model.endpoints[0]);
        assert_eq!(m.storage_bytes().stated(), Some(&(11 * 1170 + 11 * 4)));
        assert_eq!(l.storage_bytes(), m.storage_bytes());
        assert_eq!(l.wire_bound_bytes(), m.wire_bound_bytes());
        assert_eq!(leaf.types.max_fields(), model.types.max_fields());
        assert_eq!(leaf.types.max_kinds(), model.types.max_kinds());
        assert_eq!(
            leaf.types.max_nested_depth(),
            model.types.max_nested_depth()
        );
    }

    /// With NO table, the region's refusal carries the bound's own reason, so a
    /// reader holding the row is told which input is missing and where it comes
    /// from rather than "not available".
    #[test]
    fn a_region_refused_for_its_bound_names_why_the_bound_is_missing() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(10))]);
        let d = build(&model_only(&inv));
        let ep = &d.endpoints[0];
        let storage = ep.storage_bytes();
        let why = storage.refusal().expect("no table, no bound, no region");
        assert!(why.contains("wire bound is not available"), "{why}");
        assert!(why.contains("no message-bound table"), "{why}");
        assert!(why.contains("a workspace cargo image"), "{why}");
    }

    // --- phase-454 W14: the model-only producer -----------------------------

    /// THE RULING, as one assertion: emit what the inputs support, refuse every
    /// field whose input is missing, and NAME that input in each reason.
    ///
    /// The refused fields are enumerated rather than counted. A count would pass
    /// if a refusal moved from one field to another, which is the shape of every
    /// silent mis-description this model exists to remove.
    ///
    /// issue 1393 -- this caller hands over NO bound table, so the payload class
    /// refuses naming that; `handed_the_tables_the_model_road_states_the_region_the_leaf_road_states`
    /// is the same image with the tables, stating all of it.
    ///
    /// phase-457 W3 -- the row is deliberately UNOBSERVED (`None`), which is what
    /// every model-road row carries today: a launch declaration says an endpoint
    /// exists and nothing about which of the executor's eleven registration entry
    /// points its code calls, and the probe's observation is not joined in (issue
    /// 1594). `an_observed_row_lets_the_model_road_state_the_in_place_path` is the
    /// case once it is.
    #[test]
    fn a_model_only_descriptor_with_no_table_refuses_each_field_on_its_missing_input() {
        let inv = inventory(vec![sub_observed(
            "std_msgs/msg/String",
            "/chatter",
            Some(3),
            None,
        )]);
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

        // REFUSED — this caller handed over no bound table and observed no
        // registration, and each refusal names THAT input. issue 1393 closed
        // the road-wide refusal, so none of them names it.
        for (what, reason, input) in [
            (
                "wire_bound_bytes",
                ep.wire_bound_bytes().refusal(),
                "no message-bound table",
            ),
            (
                "storage_bytes",
                ep.storage_bytes().refusal(),
                "no message-bound table",
            ),
            (
                "registration_path",
                ep.registration_path().refusal(),
                UNOBSERVED_ON_MODEL_ROAD_ISSUE,
            ),
            (
                "types.max_fields",
                d.types.max_fields().refusal(),
                "no message-bound table",
            ),
            (
                "types.max_kinds",
                d.types.max_kinds().refusal(),
                "no message-bound table",
            ),
            (
                "types.max_nested_depth",
                d.types.max_nested_depth().refusal(),
                "no message-bound table",
            ),
        ] {
            let reason =
                reason.unwrap_or_else(|| panic!("`{what}` must be REFUSED, not stated or absent"));
            assert!(
                reason.contains(input),
                "`{what}`'s refusal must name its missing input ({input}): {reason}"
            );
            assert!(!reason.contains("issue 1393"), "`{what}`: {reason}");
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
        // Unobserved, for the reason the test above states: the road's own prose
        // is what a reader holding the artifact needs, and phase-457 W3 keeps the
        // horizon's wording for an unobserved row rather than replacing it with
        // the fact-specific one.
        let inv = inventory(vec![sub_observed(
            "std_msgs/msg/String",
            "/chatter",
            Some(3),
            None,
        )]);
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

    /// A road with no table refuses exactly THREE per-row fields and degrades
    /// nothing else — RFC-0100 D6's "a refusal never degrades another consumer's
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
        assert_eq!(
            leaf.image.subscription_entities(),
            model.image.subscription_entities()
        );
        assert_eq!(leaf.image.timer_entities(), model.image.timer_entities());
        assert_eq!(
            leaf.image.service_server_entities(),
            model.image.service_server_entities()
        );
        assert_eq!(
            leaf.image.action_client_entities(),
            model.image.action_client_entities()
        );
        assert_eq!(
            leaf.image.action_server_entities(),
            model.image.action_server_entities()
        );
        assert_eq!(
            leaf.image.service_client_entities(),
            model.image.service_client_entities()
        );
        assert_eq!(
            leaf.image.guard_condition_entities(),
            model.image.guard_condition_entities()
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

    /// issue 1393 — every row `cyclonedds_env` WRITES has a reader.
    ///
    /// `NROS_CYCLONEDDS_HEAP_BUDGET_BYTES` was written as a cargo `[env]` row
    /// from phase-454 W6.c on and read by nothing: the C++ half takes these
    /// numbers only through `nros-rmw-cyclonedds-sys`'s `KNOBS` forward list,
    /// which did not name it, so RFC-0100 D11's boot assertion was inert on the
    /// cargo road. Asserted against that list's SOURCE, over the rows a fully
    /// stated descriptor emits, so a fifth row added here without a reader reds
    /// the same way.
    #[test]
    fn every_cyclonedds_env_row_is_forwarded_by_the_sys_build_script() {
        let inv = inventory(vec![sub("std_msgs/msg/String", "/chatter", Some(3))]);
        let written = WrittenDescriptor {
            path: std::path::PathBuf::from("<test>"),
            desc: build(&base(&inv)),
        };
        let rows = written.cyclonedds_env();
        assert_eq!(rows.len(), 4, "precondition: every row is stated: {rows:?}");
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../rmw/cyclonedds/nros-rmw-cyclonedds-sys/build.rs"),
        )
        .expect("read nros-rmw-cyclonedds-sys/build.rs");
        let start = src
            .find("const KNOBS: &[&str] = &[")
            .expect("the KNOBS list");
        let list = &src[start..start + src[start..].find("];").expect("its end")];
        for key in rows.keys() {
            assert!(
                list.contains(&format!("\"{key}\"")),
                "`{key}` is written as a cargo [env] row and `KNOBS` does not forward it -- \
                 a fact with no reader"
            );
        }
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

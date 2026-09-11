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
//! `[types]`'s `max_fields` / `max_kinds` / `max_nested_depth` are REFUSED with a
//! reason rather than left absent: they are derivable, from the schema walk
//! codegen already does, and W6.c is the wave that reaches it. A refusal says
//! that; an absence would say nobody ever asked.

use nros_sizing_descriptor::{
    Basis, Durability, Endpoint, EndpointKind, History, RegistrationPath, Reliability,
    SizingDescriptor, Status, Target, Types,
};
use rosidl_codegen::bounds::BoundState;

use crate::entity_inventory::{Declaration, EntityInventory, EntityKind};

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
    /// Whether the linked backend carries type descriptors.
    pub backend_schema: Option<BackendSchema>,
    /// The backend's name, for prose only.
    pub rmw: Option<String>,
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

    desc.types = type_facts(inputs, &desc.endpoints);
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

fn endpoint_row(
    kind: EndpointKind,
    ty: &str,
    topic: &str,
    d: &crate::entity_inventory::EntityDecl,
    inputs: &DescriptorInputs<'_>,
    target: &Target,
) -> Endpoint {
    let mut ep = Endpoint::new(kind, ty, topic);
    let history = d.history.and_then(map_history);
    ep.set_history(history);
    ep.set_reliability(d.reliability.and_then(map_reliability));
    ep.set_durability(d.durability.and_then(map_durability));

    // `keep_all` kills the depth-derived fields and NOTHING else -- RFC-0100 D6,
    // the one trigger that ships a too-small buffer rather than a too-large one.
    // phase-454 W3 implements the same refusal one level up, for the whole depth
    // table; carrying it per row is what lets the rest of this image size.
    let keep_all = history == Some(History::KeepAll);
    if keep_all {
        ep.refuse(
            "depth",
            format!(
                "history = keep_all on {} {topic}: a KEEP_ALL queue has no static bound, so a \
                 depth beside it prices nothing (RFC-0100 D6)",
                kind.tag()
            ),
        );
    } else {
        ep.set_depth(d.depth);
    }

    // The wire bound, joined from the bound inventory by the same `pkg/msg/Name`
    // spelling both tables use.
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

    ep.set_registration_path(registration_path(kind, inputs));
    if ep.registration_path().stated().is_none() {
        ep.refuse("registration_path", registration_path_refusal(inputs));
    }

    // Only a subscription claims a topic-sample receive region. Every other kind
    // leaves `storage_bytes` ABSENT, which is the accurate statement: nothing
    // refused it, there is simply no such region. A publisher serializes into a
    // per-call stack array, which is a transmit buffer and a different question.
    if kind.receives_topic_sample() {
        set_storage_bytes(&mut ep, bound, target, keep_all);
    }
    ep
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
    match (inputs.language?, inputs.backend_schema?) {
        // A C/C++ entry that registers typed supplies `rx_size_bound<M>`; the
        // raw no-hint row is a property of an individual call site, not of the
        // image, and nothing this writer reads distinguishes them. The typed
        // hint is therefore what a C/C++ entry is credited with, and W10 --
        // which closes the C half of the declared-QoS check -- is where a
        // per-call-site answer becomes available.
        (EntryLanguage::CFamily, _) => Some(RegistrationPath::CTypedHint),
        (EntryLanguage::Rust, BackendSchema::Descriptors) => {
            Some(RegistrationPath::RustTypedDescriptors)
        }
        (EntryLanguage::Rust, BackendSchema::Schemaless) => {
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
    format!(
        "cannot tell which registration path this endpoint takes -- {} unknown. A Rust typed \
         registration on a schemaless backend claims the closure buffer rather than the type's \
         bound (issue 1319), so the path is refused rather than assumed",
        missing.join(" and ")
    )
}

/// `[types]` — Cyclone's whole appetite (RFC-0100 D5).
fn type_facts(inputs: &DescriptorInputs<'_>, endpoints: &[Endpoint]) -> Types {
    let mut t = Types::default();
    if inputs.inventory.is_some() {
        let mut names: Vec<&str> = endpoints.iter().map(|e| e.type_name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        // UNFLOORED -- D7. An image with no endpoints declares zero distinct
        // types, and zero is the answer rather than a number to round up.
        t = Types::new(Some(names.len()), None, None, None);
    } else {
        t.refuse(
            "distinct_count",
            "the entity inventory did not compose, so the image's type set is not known",
        );
    }
    let pending = "not derived here: the per-type schema walk that prices it runs in codegen \
                   and does not reach this writer (phase-454 W6.c)";
    t.refuse("max_fields", pending);
    t.refuse("max_kinds", pending);
    t.refuse("max_nested_depth", pending);
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
) -> eyre::Result<std::path::PathBuf> {
    let leaf = img.leaf.as_path();
    let (inventory, inv_error) = match crate::leaf_entity_env::inventory_for_leaf(leaf) {
        Ok((inv, _unprobeable)) if !inv.is_empty() => (Some(inv), None),
        Ok(_) => (None, None),
        Err(e) => (None, Some(e.to_string())),
    };
    if let Some(e) = &inv_error {
        eprintln!(
            "{who}: {}: sizing descriptor has no endpoint rows ({e}); every consumer keeps its \
             own defaults",
            leaf.display()
        );
    }
    let (bounds, bounds_error) = match crate::leaf_payload_classes::leaf_bound_inventory(leaf) {
        Ok(b) => (b, None),
        Err(e) => (Vec::new(), Some(e)),
    };

    let rmw = img.decl.rmw.clone();
    let inputs = DescriptorInputs {
        entry: img.image_id.clone(),
        inventory: inventory.as_ref(),
        bounds,
        bounds_error,
        target_triple: img.target.clone(),
        // A board that pins no rustc triple builds for the host, and that is
        // the ONE case where this process's own width is the target's.
        host_build: img.target.is_none(),
        heap_budget_bytes: heap_budget(path_env, &img.board),
        // A cargo leaf is a Rust entry by construction: this road is cargo, and
        // the C/C++ images go through cmake.
        language: Some(EntryLanguage::Rust),
        backend_schema: rmw.as_deref().and_then(backend_schema),
        rmw,
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
    Ok(path)
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
    use nros_orchestration_ir::qos_override::QoSHistoryPolicy;

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

    fn bounded(ty: &str, rx: usize) -> (String, BoundState) {
        (ty.into(), BoundState::Bounded { tx: rx - 4, rx })
    }

    fn base<'a>(inv: &'a EntityInventory) -> DescriptorInputs<'a> {
        DescriptorInputs {
            entry: "talker".into(),
            inventory: Some(inv),
            bounds: vec![bounded("std_msgs/msg/String", 1170)],
            bounds_error: None,
            target_triple: Some("thumbv7em-none-eabihf".into()),
            host_build: false,
            heap_budget_bytes: Some(65536),
            language: Some(EntryLanguage::Rust),
            backend_schema: Some(BackendSchema::Schemaless),
            rmw: Some("zenoh".into()),
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
        assert_eq!(
            ep.registration_path().stated(),
            Some(&RegistrationPath::RustTypedSchemaless)
        );
        assert_eq!(d.meta.status, Status::Derived);
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
    }
}

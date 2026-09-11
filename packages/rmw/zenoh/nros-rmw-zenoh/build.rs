use nros_sizing_descriptor::{EndpointKind, Fact, SizingDescriptor};

fn main() {
    // phase-454 W6.a (RFC-0100 D5) — what this IMAGE declares, read by path.
    //
    // NOT watched as a variable: `load_for_build_script` puts the rebuild edge
    // on the file's CONTENT, which is the whole reason D4 chose a file. A
    // `rerun-if-env-changed` on a path name is issue 0491, and this crate is
    // one of the two that paid for it.
    let sizing = sizing_descriptor();

    // issue 0682 — the peer-mode build input (`just test-zpico-peer`).
    println!("cargo:rerun-if-env-changed=ZPICO_MULTICAST_TRANSPORT");
    println!("cargo:rerun-if-env-changed=NROS_SUBSCRIBER_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SERVICE_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=NROS_SERVICE_TIMEOUT_MS");
    println!("cargo:rerun-if-env-changed=NROS_KEYEXPR_STRING_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_RING_DEPTH");
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_LARGE_SIZE");
    println!("cargo:rerun-if-env-changed=ZPICO_SUBSCRIBER_SIZE_THRESHOLD");
    println!("cargo:rerun-if-env-changed=ZPICO_MAX_LARGE_SUBSCRIBERS");
    // issue 1122 — WATCH what we READ. Consuming a declared fact without
    // declaring it is what let the queryable tables go stale (the note on
    // `resolve_queryable_default` one crate over): an image that gains or
    // loses a subscription keeps its previously-sized pool until something
    // else forces a rebuild, and the sizing then reads as applied while being
    // stale.
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_LARGE_SUBSCRIBERS");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_EXECUTOR_MAX_NODES");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_SUBSCRIBER_LARGE_SIZE");
    println!("cargo:rerun-if-env-changed=NROS_EXECUTOR_MAX_NODES");
    println!("cargo:rerun-if-env-changed=ZPICO_PUBLISHER_TX_BUFFER_SIZE");

    // Phase 214.C.3 — default coordinated with
    // `packages/core/nros-node/build.rs::NROS_SUBSCRIPTION_BUFFER_SIZE`
    // (also 1024). If you change one, change the other — they share the
    // wire-format expectation. Both can be overridden independently via
    // their respective env vars.
    // issue 1199 — the derived SMALL class, on the same DECLARED road as the
    // large count below. The derivation publishes it only when something
    // received actually fits under the ceiling, so an absent variable is "no
    // answer" and the crate default stands.
    let sub_size: usize = env_usize_rung(
        "NROS_SUBSCRIBER_BUFFER_SIZE",
        declared_usize("NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE"),
        1024,
    );
    // phase-454 W6.a — the `SLOT_BYTES` factor of `SERVICE_BUFFERS`
    // (RFC-0100 D2's `pool = Σ COUNT × SLOTS × SLOT_BYTES + fixed`, where COUNT
    // is `ZPICO_MAX_SESSIONS × ZPICO_MAX_QUERYABLES` and SLOTS is
    // `SERVICE_REQUEST_RING_DEPTH`). The declared service surface supplies the
    // DEFAULT; a stated knob, and the Kconfig rung on Zephyr, still win.
    let svc_size: usize = env_usize(
        "ZPICO_SERVICE_BUFFER_SIZE",
        declared_service_request_bytes(sizing.as_ref()).unwrap_or(SERVICE_BUFFER_SIZE_DEFAULT),
    );
    // Phase 160.C.2 — bumped 10_000 → 30_000. The original 10 s default
    // was too short for slow zenoh-pico flushes on Zephyr/NSOS where
    // each publish/query can take ~2.5 s under Z_FEATURE_INTEREST=1. An
    // action `get_result` query sent while the server is still running a
    // feedback loop (11 publishes × ~2.5 s each = ~28 s before
    // `complete_goal` fires) expires the internal query timer well
    // before the server reaches its `try_handle_get_result` handler.
    // Bumping to 30 s covers the common slow-Zephyr action window; fast
    // services on POSIX still return in milliseconds so the wider cap
    // only matters when something is genuinely slow.
    // phase-400 W6 — the `[knobs.zenoh.limits]` rungs. `None` when no lane named
    // a platform, and the builtins below then stand.
    let limits = nros_platform_config::platform_config::BuildRungs::from_build_env()
        .map(|r| r.zenoh_limit_rungs())
        .unwrap_or_default();
    // NOT a ladder knob, deliberately: this value is read HERE and in
    // `nros-build-helpers`'s C emitter, because two artifacts embed it (a Rust
    // const and a C define). `check-knob-single-reader` allows a migrated knob
    // exactly one reader, and that invariant is what stops the pair drifting —
    // so migrating this one needs the two readers to share a single resolution
    // point first, which is a change to where the value is EMITTED, not to the
    // ladder.
    let service_timeout_ms: usize = env_usize("NROS_SERVICE_TIMEOUT_MS", 30_000);
    let keyexpr_string_size: usize =
        env_usize_rung("NROS_KEYEXPR_STRING_SIZE", limits.keyexpr_string_size, 256);
    // Phase 124.D.3.c — SPSC ring depth per subscriber. Default 4
    // keeps the static-RAM bump small (4 × SUBSCRIBER_BUFFER_SIZE
    // per subscriber); raise for burst-heavy topics. Must be ≥ 1.
    //
    // phase-454 W6.a — the `SLOTS` factor of both payload pools, and the one
    // factor RFC-0100 D2 names as coming from the QoS history depth:
    //
    //   zenoh `SMALL_PAYLOADS` | subscription count | QoS depth | small bound
    //
    // It was AUTHORED until this wave. What the image declares now supplies the
    // DEFAULT, below both stated rungs: a consumer naming the env var wins, and
    // so does a board that states `[knobs.zenoh.limits] subscriber_ring_depth`,
    // because a board rung is a STATEMENT (RFC-0049) while a derived fact is
    // only ever a default (RFC-0100 D1).
    let ring_depth: usize = env_usize_min(
        "ZPICO_SUBSCRIBER_RING_DEPTH",
        limits
            .subscriber_ring_depth
            .or(declared_ring_depth(sizing.as_ref()))
            .unwrap_or(SUBSCRIBER_RING_DEPTH_DEFAULT),
        1,
    );
    // Phase 231 (RFC-0038) — size-class receive buffers. `SUBSCRIBER_BUFFER_SIZE`
    // above is the `small` class slot size; the `large` class is for big
    // messages (images, point clouds). A subscription routes to `large` when its
    // `rx_buffer_hint` exceeds the threshold. `large` is capped at a small count
    // so the big slots don't multiply across every subscriber.
    // issue 1199 — the derived LARGE class size. Published only when the large
    // COUNT is non-zero: a size for a class with no blocks would be inventing a
    // number, which is the rule `_nros_bounds_publish_payload_classes` states
    // and `DERIVED_PAYLOAD_ENV_KEYS` repeats for the leaf road.
    let large_size: usize = env_usize_rung(
        "ZPICO_SUBSCRIBER_LARGE_SIZE",
        declared_usize("NROS_DECLARED_SUBSCRIBER_LARGE_SIZE"),
        16384,
    );
    let size_threshold: usize = env_usize("ZPICO_SUBSCRIBER_SIZE_THRESHOLD", 2048);
    // Phase 403 W4 — the count of LARGE-class blocks, and 0 is legal.
    //
    // This carried issue 0827's floor of 1 until W4, on the stated ground that
    // "the lookup path indexes the pool unconditionally". That is true of the
    // other two floored knobs and it is NOT true here: `alloc_payload_block`
    // tests `idx >= MAX_LARGE_SUBSCRIBERS` BEFORE it indexes `LARGE_PAYLOADS`,
    // so a zero-length pool returns `None` and is never subscripted. The floor
    // was therefore charging every image `RING_DEPTH * LARGE_SIZE` bytes
    // (65,536 at the defaults) for a class it may never route a single
    // subscription into -- which is the shape of the waste 0827 exists to
    // remove, kept alive by 0827's own guard.
    //
    // Zero is a claim, not a shrug: it says this image's types all fit the
    // small class. `alloc_payload_block` refuses a hint that no class can hold,
    // so getting it wrong fails at `create_subscription` rather than dropping
    // every sample at the transport.
    //
    // issue 1122 — the DEFAULT comes from the declaration when cmake made one.
    // `nros_derive_message_bound_knobs()` derives this correctly on every lane
    // and, before 1122, only the Zephyr lane could deliver it; the number was
    // computed, written to disk and discarded everywhere else. It now travels
    // as `NROS_DECLARED_LARGE_SUBSCRIBERS`, on the same lane-independent
    // carrier `NROS_DECLARED_SERVICE_SERVERS` already uses.
    //
    // It is the DEFAULT and not the value, so rung 1 of the ladder survives: a
    // consumer who names `ZPICO_MAX_LARGE_SUBSCRIBERS` still wins. Setting the
    // knob itself in the child environment would have made the named override
    // unreachable, silently.
    let max_large: usize = env_usize_rung(
        "ZPICO_MAX_LARGE_SUBSCRIBERS",
        declared_usize("NROS_DECLARED_LARGE_SUBSCRIBERS"),
        2,
    );
    // Phase 268 — per-session per-node NN liveliness token cap. One zenoh
    // session hosts at most the executor's node cap of graph nodes, so this
    // tracks `nros-node`'s `NROS_EXECUTOR_MAX_NODES` (default 4); keep them in
    // sync — set the same env var for both. `.max(1)` so a session always has
    // room for its own primary node.
    // issue 1233 — the SECOND reader of this knob, and the delivery asymmetry
    // it carried. `nros-node` resolves it through a four-rung ladder that
    // consults `$DOTCONFIG`; this crate's `env_usize` has no Kconfig row for
    // the name, so on a Zephyr Rust image `CONFIG_NROS_EXECUTOR_MAX_NODES`
    // reached one crate and not the other — and the comment below already says
    // "keep them in sync", which nothing enforced.
    //
    // Taking the declared rung here closes half of that: both crates now see
    // the number cmake derived. The floor stays, because the lookup path here
    // indexes the pool unconditionally (issue 0827) while `nros-node`'s does
    // not — one derivation, two consumers, two legal minima.
    let max_nodes: usize = env_usize_min(
        "NROS_EXECUTOR_MAX_NODES",
        declared_usize("NROS_DECLARED_EXECUTOR_MAX_NODES").unwrap_or(4),
        1,
    );
    // Issue 0813 — per-publisher TX arena capacity for the zero-copy loan path
    // (`SlotLending`). This was a bare `const` in `shim/publisher.rs`, so its
    // 1 KiB ceiling was neither raisable by a consumer nor visible to
    // `scripts/gen-pool-inventory.py`. It is the publisher-side twin of
    // `NROS_SUBSCRIBER_BUFFER_SIZE` and shares its default. The arena is
    // per-publisher, so the cost is `ZPICO_MAX_PUBLISHERS` × this — priced in
    // the inventory via the `nros-pool:` annotation beside `LendArena`.
    let publisher_tx_size: usize = env_usize("ZPICO_PUBLISHER_TX_BUFFER_SIZE", 1024);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("buffer_config.rs");
    std::fs::write(
        &path,
        format!(
            "/// Subscriber buffer size (set via NROS_SUBSCRIBER_BUFFER_SIZE, default 1024).\n\
             pub const SUBSCRIBER_BUFFER_SIZE: usize = {sub_size};\n\
             /// Service request buffer size (set via ZPICO_SERVICE_BUFFER_SIZE; default is\n\
             /// the largest declared service/action bound, floored at 1024 — phase-454 W6.a).\n\
             pub const SERVICE_BUFFER_SIZE: usize = {svc_size};\n\
             /// Default service client RPC timeout in milliseconds\n\
             /// (set via NROS_SERVICE_TIMEOUT_MS, default 30000).\n\
             pub const SERVICE_DEFAULT_TIMEOUT_MS: u32 = {service_timeout_ms};\n\
             /// Maximum key expression string size for topic/service names\n\
             /// (set via NROS_KEYEXPR_STRING_SIZE, default 256).\n\
             pub const KEYEXPR_STRING_SIZE: usize = {keyexpr_string_size};\n\
             /// Key expression buffer size (KEYEXPR_STRING_SIZE + 1 for null terminator).\n\
             pub const KEYEXPR_BUFFER_SIZE: usize = {keyexpr_buf_size};\n\
             /// Phase 124.D.3.c — per-subscriber SPSC ring depth (set via\n\
             /// ZPICO_SUBSCRIBER_RING_DEPTH or the board's `subscriber_ring_depth` rung;\n\
             /// default is the largest declared subscription depth, else 4 — phase-454 W6.a).\n\
             pub const SUBSCRIBER_RING_DEPTH: usize = {ring_depth};\n\
             /// Phase 231 (RFC-0038) — `large` size-class slot size\n\
             /// (set via ZPICO_SUBSCRIBER_LARGE_SIZE, default 16384).\n\
             pub const SUBSCRIBER_LARGE_SIZE: usize = {large_size};\n\
             /// Phase 231 — rx_buffer_hint above this routes to the `large` class\n\
             /// (set via ZPICO_SUBSCRIBER_SIZE_THRESHOLD, default 2048).\n\
             pub const SUBSCRIBER_SIZE_THRESHOLD: usize = {size_threshold};\n\
             /// Phase 231 — max concurrent `large`-class subscribers\n\
             /// (set via ZPICO_MAX_LARGE_SUBSCRIBERS, default 2).\n\
             pub const MAX_LARGE_SUBSCRIBERS: usize = {max_large};\n\
             /// Phase 268 — per-session per-node NN liveliness token cap, tracking\n\
             /// `nros-node`'s NROS_EXECUTOR_MAX_NODES (default 4): one session hosts\n\
             /// at most that many graph nodes.\n\
             pub const MAX_PER_NODE_LIVELINESS: usize = {max_nodes};\n\
             /// Issue 0813 — per-publisher TX arena capacity for the zero-copy\n\
             /// loan path (set via ZPICO_PUBLISHER_TX_BUFFER_SIZE, default 1024).\n\
             pub const PUBLISHER_TX_BUFFER_SIZE: usize = {publisher_tx_size};\n",
            keyexpr_buf_size = keyexpr_string_size + 1,
        ),
    )
    .unwrap();
}

/// The per-subscriber SPSC ring depth when nothing states or derives one.
///
/// Phase 124.D.3.c's number, unchanged. It is a POLICY default — a burst
/// absorber, not a QoS depth — which is why a refused derivation falls back
/// HERE rather than to `rmw_qos_profile_default`'s KEEP_LAST(10): the
/// consequence of a short ring is the reported, graph-advertised downgrade in
/// `shim/qos.rs` (`nros-qos-honours: DEPTH`), not a `BufferTooSmall`. Falling
/// back to 10 would multiply every partially-declared image's payload pools by
/// 2.5 for a policy nobody wrote.
const SUBSCRIBER_RING_DEPTH_DEFAULT: usize = 4;

/// The service-request slot size when nothing states or derives one.
///
/// Also the FLOOR of the derivation below. See
/// [`declared_service_request_bytes`] for why the app's own declarations may
/// raise this number and may not lower it.
const SERVICE_BUFFER_SIZE_DEFAULT: usize = 1024;

/// The descriptor this build was pointed at, or `None`.
///
/// Mirrors `nros-node/build.rs::sizing_descriptor`, deliberately: three
/// outcomes, and the middle one is D6 working.
///
/// * **no descriptor** — nobody ran `nros sync` for this image, or nothing
///   pointed this build at one. Every knob below keeps its builtin and the
///   build is byte-identical to every build before this wave;
/// * **a descriptor that refuses a field** — the builtin, plus a
///   `cargo::warning` naming the refusal. *"Worst case when refused, always the
///   safe direction and always loud"*;
/// * **a descriptor that does not parse** — a hard build error naming the file.
///   A descriptor EXISTS, so defaulting would size from numbers a user believes
///   they supplied.
fn sizing_descriptor() -> Option<SizingDescriptor> {
    match nros_sizing_descriptor::from_build_env() {
        Ok(d) => d,
        // A path was named and nothing is there, or the file is corrupt. Both
        // are loud: somebody pointed this build at a descriptor.
        Err(e) => panic!("{e}"),
    }
}

/// D6's second half: *"Worst case when refused, always the safe direction and
/// always LOUD — the build prints what declaring would save."*
///
/// No crate prefix: cargo already stamps `nros-rmw-zenoh@<ver>:` on a build
/// script's warning, and adding one prints the name twice.
fn warn(msg: &str) {
    println!("cargo::warning={msg}");
}

/// The ring depth this image's subscriptions declare — RFC-0100 D2's `SLOTS`.
///
/// One knob serves every subscriber (the ring is a build-time constant baked
/// into `SmallPayloadBlock` / `LargePayloadBlock`), so the derived demand is the
/// MAXIMUM over the subscriptions: a ring shorter than a declared depth is the
/// clamp `shim/qos.rs` reports and advertises, and clamping an endpoint the
/// image asked for is exactly what this wave exists to stop.
///
/// `None` — keep the builtin — in three cases, and each is a different fact:
///
/// * **no subscription rows.** Nothing declared a depth, so there is no demand
///   to read. NOT zero: absence is not zero (D6), and a derived 0 here would
///   reach `env_usize_min`'s floor as a build panic naming a knob the user
///   never set;
/// * **any subscription's `depth` is REFUSED.** The `keep_all` trigger, and the
///   one the RFC names first: *"a KEEP_ALL queue has no static bound"*. The
///   refusal is HONOURED — no number is substituted for it, here or anywhere —
///   and the build prints what the endpoint said;
/// * **any subscription's `depth` is ABSENT.** That endpoint gets
///   `rmw_qos_profile_default`'s KEEP_LAST(10) at runtime, which the maximum
///   over the OTHERS cannot see. Deriving from a subset would under-size the
///   ring for the row that stayed silent, which is the direction that loses
///   samples.
fn declared_ring_depth(desc: Option<&SizingDescriptor>) -> Option<usize> {
    let desc = desc?;
    let subs: Vec<_> = desc
        .endpoints
        .iter()
        .filter(|e| e.kind == EndpointKind::Subscription)
        .collect();
    if subs.is_empty() {
        return None;
    }
    let mut max = 0usize;
    for s in &subs {
        match s.depth() {
            Fact::Stated(d) => max = max.max(d as usize),
            Fact::Refused(reason) => {
                warn(&format!(
                    "sizing descriptor refuses `depth` on subscription {}: {reason}. The \
                     per-subscriber ring keeps {SUBSCRIBER_RING_DEPTH_DEFAULT} \
                     (ZPICO_SUBSCRIBER_RING_DEPTH), and `shim/qos.rs` reports the clamp for \
                     anything that asks for more",
                    s.topic
                ));
                return None;
            }
            Fact::Absent => {
                warn(&format!(
                    "sizing descriptor states no `depth` for subscription {} ({}); the \
                     per-subscriber ring keeps {SUBSCRIBER_RING_DEPTH_DEFAULT}. Declaring every \
                     subscription's depth sizes both payload pools from what this image actually \
                     keeps",
                    s.topic, s.type_name
                ));
                return None;
            }
        }
    }
    if max == 0 {
        // A stated KEEP_LAST(0) keeps nothing, and the ring's slot index is
        // `counter % depth`. The DESCRIPTOR is right to carry it unfloored
        // (D7 — zero is a legitimate demand and the floor lives at the
        // consumer); this build script IS that consumer, and it refuses rather
        // than silently substituting 1, which is issue 0827's rule for a value
        // somebody stated.
        warn(
            "sizing descriptor states KEEP_LAST(0) on a subscription; a zero-slot receive ring \
             cannot be indexed, so the per-subscriber ring keeps its default of \
             ZPICO_SUBSCRIBER_RING_DEPTH",
        );
        return None;
    }
    Some(max)
}

/// The service-request slot size this image's declarations ask for.
///
/// RFC-0100 D2 gives `SERVICE_BUFFERS` as `sessions × queryables` slots of one
/// request each, so `SLOT_BYTES` is the largest request or response this image
/// can receive. `None` keeps [`SERVICE_BUFFER_SIZE_DEFAULT`].
///
/// # The derivation may RAISE this number and may not lower it
///
/// That is not timidity, it is what the descriptor can and cannot see. A zenoh
/// service server IS a queryable, and **eleven of them exist before the app
/// declares anything** — `[param_services]` (6) and `[lifecycle]` (5), issue
/// 0460's measurement. Those servers receive `rcl_interfaces/srv/*` and
/// `lifecycle_msgs/srv/*` requests through this very pool, and NOTHING in the
/// contract declares them, so they appear in no `[[endpoint]]` row. Sizing the
/// slot down to what the app declared would under-size a surface the
/// declaration structurally cannot mention — the failure would land as
/// `ServiceRequestSlot::overflow` on a parameter set, at runtime, on an image
/// whose every knob gate read green.
///
/// So the app's declarations are an over-ride upward and the builtin is the
/// floor, with the floor at the CONSUMER rather than in the descriptor (D7).
/// Lowering it is a separate question that needs the built-in service surface
/// to become a declared one.
///
/// # Why this refuses on every in-tree image today
///
/// The join is `(kind, type, topic)` and a service row's type is
/// `pkg/srv/Name`. `BoundInventory::record_message` is called for `.msg` files
/// and for nothing else, so `pkg/srv/Name_Request` and `pkg/action/Name_Result`
/// have no bound entry — the entity inventory's own header says so, and the
/// producer therefore writes `wire_bound_bytes` REFUSED on every service and
/// action row. The refusal is printed rather than swallowed: it names the type,
/// which is where the next wave has to start.
fn declared_service_request_bytes(desc: Option<&SizingDescriptor>) -> Option<usize> {
    let desc = desc?;
    let svc: Vec<_> = desc
        .endpoints
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EndpointKind::ServiceServer
                    | EndpointKind::ServiceClient
                    | EndpointKind::ActionServer
                    | EndpointKind::ActionClient
            )
        })
        .collect();
    if svc.is_empty() {
        return None;
    }
    let mut max = 0usize;
    for e in &svc {
        match e.wire_bound_bytes() {
            Fact::Stated(b) => max = max.max(b),
            // ONE unpriced row refuses the whole derivation: the slot is shared
            // by every queryable, so a maximum over the rows that answered is
            // not a bound on the rows that did not.
            f => {
                warn(&format!(
                    "sizing descriptor states no `wire_bound_bytes` for {} {} ({}): {}. The \
                     service request slot keeps {SERVICE_BUFFER_SIZE_DEFAULT} bytes \
                     (ZPICO_SERVICE_BUFFER_SIZE)",
                    e.kind.tag(),
                    e.topic,
                    e.type_name,
                    f.refusal().unwrap_or("nothing derived it"),
                ));
                return None;
            }
        }
    }
    Some(max.max(SERVICE_BUFFER_SIZE_DEFAULT))
}

/// The Kconfig option each knob is resolved from on Zephyr. Only the two the
/// cmake side forwards (`_nros_resolve_knob` in `nros_cargo_build.cmake`) have
/// a row; the rest are env-or-default as before. See issue 0460 and the twin
/// table in `nros-zpico-build`'s runner — a Zephyr RUST image never inherits
/// the cmake `set(ENV{...})` exports, so without this a Kconfig'd buffer size
/// reached the C lane and silently did not reach this crate.
const KCONFIG_KNOBS: &[(&str, &str)] = &[
    (
        "NROS_SUBSCRIBER_BUFFER_SIZE",
        "CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE",
    ),
    (
        "ZPICO_SERVICE_BUFFER_SIZE",
        "CONFIG_NROS_SERVICE_BUFFER_SIZE",
    ),
];

/// issue 0827 — a floored knob must REFUSE a value below its floor, never
/// round it up.
///
/// These pools cannot be zero-length: the lookup paths index them
/// unconditionally, which is what the `.max(1)` this replaces was protecting.
/// But `.max(1)` protected it by SILENTLY substituting 1, so a knob of 0 built
/// a pool and reported nothing.
///
/// Phase 403 W4 — `ZPICO_MAX_LARGE_SUBSCRIBERS` was the third member and is no
/// longer floored. It never met the premise: `alloc_payload_block` bounds-checks
/// the class index BEFORE subscripting `LARGE_PAYLOADS`, so a zero-length large
/// pool returns `None` rather than indexing out of range. Its floor was
/// reserving 65,536 bytes at the defaults for a class an image whose types all
/// fit the small class never routes into. Read that as the rule this doc
/// already states, applied to itself: a floor is only honest where the lookup
/// really cannot refuse the entity kind first, and here it can.
///
/// That matters now because 0827's fix derives these knobs from the resolved
/// model: an image with no subscriptions would ask for 0, get 1, and reserve
/// 64 KiB while every config file and inventory line read as satisfied. A knob
/// that cannot honour a value has to say so at BUILD time, where the person
/// who set it is standing — the alternative is a saving that looks applied and
/// is not, which is the defect class this campaign keeps finding.
///
/// Unset stays silent: the defaults are all above the floor.
fn env_usize_min(name: &str, default: usize, min: usize) -> usize {
    let v = env_usize(name, default);
    if v < min {
        panic!(
            "{name}={v} is below this pool's floor of {min}.\n  \
             The lookup path indexes the pool unconditionally, so a shorter one \
             is not representable — raise the value, or change the lookup to \
             refuse the entity kind first (issue 0827).\n  \
             This used to be silently rounded up to {min}, which reserved the \
             memory anyway while reading as though the knob had been honoured."
        );
    }
    v
}

fn env_usize(name: &str, default: usize) -> usize {
    match KCONFIG_KNOBS.iter().find(|(env, _)| *env == name) {
        Some((_, kconfig)) => nros_zephyr_build::knob_usize(name, kconfig, default),
        None => std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default),
    }
}

/// phase-400 W6 — env, then the platform/board rung, then the builtin.
///
/// A thin sibling of `env_usize` rather than a change to it: the other callers
/// of that helper are knobs with no tenant, and giving them a rung parameter
/// they always pass `None` for would say the ladder reaches further than it
/// does.
fn env_usize_rung(name: &str, rung: Option<usize>, default: usize) -> usize {
    env_usize(name, rung.unwrap_or(default))
}

/// issue 1122 / 1199 — a `NROS_DECLARED_*` fact cmake derived for THIS image.
///
/// `None` when cmake made no claim, which is every build that is not driven by
/// our CMake lanes (a bare `cargo build`, a Rust leaf) and every configure
/// whose message-bound join REFUSED or answered on the `closure` basis. The
/// carrier is only written under `derived` + `subscribed`
/// (`_nros_payload_facts_env` in `cmake/NanoRosEntityFacts.cmake`), so an
/// absent variable is "no answer" and never "zero".
///
/// That distinction is the whole point: `0` is a legal and meaningful value
/// here -- it says this image's types all fit the small class -- so it cannot
/// share a spelling with "nobody told me".
fn declared_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

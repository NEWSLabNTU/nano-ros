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
    // phase-461 W1 - the per-family service inboxes (the four `KCONFIG_KNOBS`
    // rows also print this line; stating it here keeps the watch list in one
    // place with its siblings).
    println!("cargo:rerun-if-env-changed=NROS_SERVICE_INBOX_BYTES");
    println!("cargo:rerun-if-env-changed=NROS_SERVICE_INBOX_DEPTH");
    println!("cargo:rerun-if-env-changed=NROS_ACTION_INBOX_BYTES");
    println!("cargo:rerun-if-env-changed=NROS_ACTION_INBOX_DEPTH");
    // phase-461 W3 -- the declared road's carriers for the two families' slot
    // sizes. WATCHED and not merely read: `check-declared-fact-carriers` rule 3
    // exists because `resolve_queryable_default` read two of these without
    // watching either, and an entry that gained a service server kept its
    // previously-sized tables until something else forced a rebuild.
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_SERVICE_INBOX_BYTES");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_ACTION_INBOX_BYTES");
    // phase-461 W2b - the BUILTIN family (the ROS parameter services and the
    // REP-2002 lifecycle services). The pair is spelled as phase-461 W2
    // forwards it, so one Kconfig symbol feeds both readers rather than the
    // two families acquiring two names for one geometry.
    println!("cargo:rerun-if-env-changed=NROS_PARAM_SERVICE_INBOX_BYTES");
    println!("cargo:rerun-if-env-changed=NROS_PARAM_SERVICE_INBOX_DEPTH");
    // WATCH what we READ -- the two carriers the builtin partition is drawn
    // from. An entry that gains a service server changes how many of its
    // queryables are the runtime's, and a fact nothing watches reads as
    // applied while being stale (issue 1122).
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_SERVICE_SERVERS");
    // issue 1485 -- the Zephyr resolver road's answer to the same question.
    println!("cargo:rerun-if-env-changed=NROS_ENTITY_APP_QUERYABLES");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_PARAM_SERVICE_SHAPE");
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
    // phase-455 W5 / issue 1341 — the TRANSIENT_LOCAL retention pool.
    println!("cargo:rerun-if-env-changed=ZPICO_MAX_TL_PUBLISHERS");
    // The retention pool's DECLARED demand, for a road with no descriptor
    // (`transient_local_publisher_demand`).
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_TL_PUBLISHERS");
    println!("cargo:rerun-if-env-changed=ZPICO_TL_RETAIN_BYTES");

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
    // phase-454 W6.a - the `SLOT_BYTES` factor of the service inbox
    // (RFC-0100 D2's `pool = Σ COUNT × SLOTS × SLOT_BYTES + fixed`, where COUNT
    // is `ZPICO_MAX_SESSIONS x ZPICO_MAX_QUERYABLES` and SLOTS is the ring
    // depth). The declared service surface supplies the DEFAULT; a stated
    // knob, and the Kconfig rung on Zephyr, still win.
    //
    // phase-461 W1 - `ZPICO_SERVICE_BUFFER_SIZE` / `CONFIG_NROS_SERVICE_BUFFER_SIZE`
    // is the one-release ALIAS of `NROS_SERVICE_INBOX_BYTES`: it resolves
    // exactly as before, and the new name outranks it when both are stated.
    //
    // phase-461 W3 - the floor is GONE. W6.a kept `.max(1024)` because the
    // builtin `[param_services]` (6) and `[lifecycle]` (5) queryables received
    // through this very pool and appear in no `[[endpoint]]` row, so sizing
    // down to the app's declarations would under-size a surface the
    // declaration structurally cannot mention. W2 moves those two families off
    // this pool and onto rings of their own, so the argument no longer holds
    // and D7's unfloored demand is restored: what is published here is the
    // DEMAND, and a consumer that needs a floor applies its own.
    let declared_request_bytes = declared_service_request_bytes(sizing.as_ref(), SERVICE_FAMILY)
        .unwrap_or(SERVICE_BUFFER_SIZE_DEFAULT);
    let svc_size: usize = env_usize("ZPICO_SERVICE_BUFFER_SIZE", declared_request_bytes);
    // phase-461 W1 - one inbox per FAMILY (issue 1352). The user-service and
    // action families each get a slot size and a ring depth; both default to
    // what the single table was (the derived-or-1024 slot at depth 4), so an
    // image that states nothing is byte-identical to the one table it had.
    // W3 prices the two families apart (`_Request` bounds per family); the
    // parameter and lifecycle families do not appear here at all -- they bring
    // their own ring (W2), sized by the crate that can price their requests.
    //
    // phase-461 W3 - the two families are priced APART, each from the request
    // types ITS OWN declared endpoints carry, on both roads that reach this
    // crate: the sizing descriptor (a cargo leaf) and the `NROS_DECLARED_*`
    // carrier the entity inventory writes (every cmake / Zephyr west image,
    // which is the road the island is on -- phase-454 W11 measured that the
    // descriptor is inert there). A stated knob still outranks both.
    let service_inbox_bytes: usize = env_usize_rung(
        "NROS_SERVICE_INBOX_BYTES",
        declared_usize("NROS_DECLARED_SERVICE_INBOX_BYTES")
            .or_else(|| declared_service_request_bytes(sizing.as_ref(), SERVICE_FAMILY)),
        svc_size,
    );
    let service_inbox_depth: usize =
        env_usize_min("NROS_SERVICE_INBOX_DEPTH", SERVICE_INBOX_DEPTH_DEFAULT, 1);
    let action_inbox_bytes: usize = env_usize_rung(
        "NROS_ACTION_INBOX_BYTES",
        declared_usize("NROS_DECLARED_ACTION_INBOX_BYTES")
            .or_else(|| declared_service_request_bytes(sizing.as_ref(), ACTION_FAMILY)),
        svc_size,
    );
    let action_inbox_depth: usize =
        env_usize_min("NROS_ACTION_INBOX_DEPTH", SERVICE_INBOX_DEPTH_DEFAULT, 1);
    let action_inbox_queryables: usize = declared_action_queryables(sizing.as_ref());
    // phase-461 W2b / issue 1352 - the BUILTIN family: the six ROS parameter
    // services of every node and the five REP-2002 lifecycle services. ONE
    // family and not two because their geometry is equal -- both carry
    // `rcl_interfaces` requests bounded by the contract's declared
    // parameters, and every lifecycle request is smaller than every parameter
    // one -- so splitting them would be two tables always holding the same
    // number.
    //
    // The SLOT is the ladder its siblings use: a stated
    // `NROS_PARAM_SERVICE_INBOX_BYTES` wins, otherwise the contract's declared
    // parameters bound it (`declared_param_request_max`), otherwise the
    // user-service slot stands, which is today's behaviour.
    //
    // The DEPTH is 1 and not the transport's 4: one parameter client sends one
    // request and waits for its reply (`ros2 param`, rclcpp's
    // `SyncParametersClient`), and a node's services are polled serially in
    // one spin, so a slot is drained within one spin period. A default on a
    // knob, not a ceiling -- an image serving a parameter dashboard states 2.
    //
    // issue 1485 -- "not stated" is a value no rung can produce, exactly as
    // `nros-node` probes the same knob, and a stated 0 is REFUSED with the same
    // words there. It used to be read with the derivation as the DEFAULT, so
    // the literal 0 the Kconfig row forwarded (its old derive sentinel) won
    // over the derivation and became the slot size.
    let builtin_inbox_bytes: usize = match env_usize("NROS_PARAM_SERVICE_INBOX_BYTES", usize::MAX) {
        usize::MAX => declared_param_request_max(sizing.as_ref()).unwrap_or(svc_size),
        0 => panic!("{PARAM_INBOX_ZERO_REFUSAL}"),
        n => n,
    };
    let builtin_inbox_depth: usize = env_usize_min("NROS_PARAM_SERVICE_INBOX_DEPTH", 1, 1);
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
    // phase-455 W5 / issue 1341 — the TRANSIENT_LOCAL retention pool, one slot
    // per TL publisher. See `transient_local_publisher_demand` for what the
    // descriptor can and cannot answer, and why an image that declares nothing
    // keeps a builtin rather than deriving zero.
    let tl_retain_bytes: usize = env_usize("ZPICO_TL_RETAIN_BYTES", TL_RETAIN_BYTES_DEFAULT);
    let tl_demand = transient_local_publisher_demand(sizing.as_ref());
    let max_tl_publishers: usize = resolve_max_tl_publishers(tl_demand);
    // phase-461 W2b - resolved HERE and not beside its two siblings above,
    // because a transient-local publisher IS a queryable and this partition
    // has to subtract it: the transient-local demand is the one input that
    // does not arrive as an env string, and asking for it twice would warn
    // twice on a refusal.
    // issue 1485 -- emitted as an `Option`, never as a sentinel. It used to be
    // `usize::MAX` for "nobody said", written as a token, and that is how the
    // absence of a DELIVERY read as the absence of a DECLARATION: the Zephyr
    // resolver road carried no application count at all, so an image whose
    // contract declares the parameter family built its builtin table empty
    // and every parameter service fell through to a user-service ring.
    let declared_app_queryables = match declared_app_queryables(tl_demand) {
        Some(n) => format!("Some({n})"),
        None => "None".to_string(),
    };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("buffer_config.rs");
    std::fs::write(
        &path,
        format!(
            "/// Subscriber buffer size (set via NROS_SUBSCRIBER_BUFFER_SIZE, default 1024).\n\
             pub const SUBSCRIBER_BUFFER_SIZE: usize = {sub_size};\n\
             /// phase-461 W1 - the one-release alias of `SERVICE_INBOX_BYTES` (set via\n\
             /// ZPICO_SERVICE_BUFFER_SIZE; default is the largest declared service/action\n\
             /// bound, floored at 1024 - phase-454 W6.a). `NROS_SERVICE_INBOX_BYTES` wins.\n\
             pub const SERVICE_BUFFER_SIZE: usize = {service_inbox_bytes};\n\
             /// phase-461 W1 - the user-service family's slot size (set via\n\
             /// NROS_SERVICE_INBOX_BYTES; default is what ZPICO_SERVICE_BUFFER_SIZE resolved).\n\
             pub const SERVICE_INBOX_BYTES: usize = {service_inbox_bytes};\n\
             /// phase-461 W1 - the user-service family's ring depth (set via\n\
             /// NROS_SERVICE_INBOX_DEPTH, default {SERVICE_INBOX_DEPTH_DEFAULT}).\n\
             pub const SERVICE_INBOX_DEPTH: usize = {service_inbox_depth};\n\
             /// phase-461 W1 - the action family's slot size (set via NROS_ACTION_INBOX_BYTES;\n\
             /// default is the user-service family's, until W3 prices them apart).\n\
             pub const ACTION_INBOX_BYTES: usize = {action_inbox_bytes};\n\
             /// phase-461 W1 - the action family's ring depth (set via NROS_ACTION_INBOX_DEPTH,\n\
             /// default {SERVICE_INBOX_DEPTH_DEFAULT}: the twin of ZPICO_MAX_PENDING_REPLIES).\n\
             pub const ACTION_INBOX_DEPTH: usize = {action_inbox_depth};\n\
             /// phase-461 W1 - queryables the action family declares per session: three per\n\
             /// declared action server, 0 when this image declares none (its action\n\
             /// queryables then draw user-service rings, which is the single-table behaviour).\n\
             pub const ACTION_INBOX_QUERYABLES: usize = {action_inbox_queryables};\n\
             /// phase-461 W2b - the builtin family's slot size: the ROS parameter services\n\
             /// and the REP-2002 lifecycle services (set via NROS_PARAM_SERVICE_INBOX_BYTES;\n\
             /// default is the largest request the contract's declared parameters can\n\
             /// produce, else the user-service slot).\n\
             pub const BUILTIN_INBOX_BYTES: usize = {builtin_inbox_bytes};\n\
             /// phase-461 W2b - the builtin family's ring depth (set via\n\
             /// NROS_PARAM_SERVICE_INBOX_DEPTH, default 1: these clients are sequential).\n\
             pub const BUILTIN_INBOX_DEPTH: usize = {builtin_inbox_depth};\n\
             /// phase-461 W2b - queryables this image's own DECLARATION attributes to the\n\
             /// application, per session. `None` when nothing declared them, which\n\
             /// leaves the builtin table empty and every queryable on the user-service\n\
             /// geometry it has today (issue 1485: never a sentinel).\n\
             pub const DECLARED_APP_QUERYABLES: Option<usize> = {declared_app_queryables};\n\
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
             pub const PUBLISHER_TX_BUFFER_SIZE: usize = {publisher_tx_size};\n\
             /// phase-455 W5 — how many TRANSIENT_LOCAL publishers this image can\n\
             /// serve at once (set via ZPICO_MAX_TL_PUBLISHERS; default is the\n\
             /// declared demand, else {TL_PUBLISHERS_DEFAULT}). Each costs one\n\
             /// retention slot here AND one slot in the C shim's queryable table.\n\
             pub const MAX_TL_PUBLISHERS: usize = {max_tl_publishers};\n\
             /// phase-455 W5 — bytes one retained TRANSIENT_LOCAL sample may hold\n\
             /// (set via ZPICO_TL_RETAIN_BYTES, default {TL_RETAIN_BYTES_DEFAULT}).\n\
             pub const TL_RETAIN_BYTES: usize = {tl_retain_bytes};\n\
             /// phase-455 W5 — how many samples a TRANSIENT_LOCAL publisher\n\
             /// retains. ONE: `rcl_action_qos_profile_status_default` is\n\
             /// KEEP_LAST(1), and `shim/qos.rs` grants this depth and advertises\n\
             /// it, so a deeper request is reported rather than pocketed.\n\
             pub const TL_RETAIN_DEPTH: u32 = 1;\n",
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
/// NOT a floor any more. phase-454 W6.a made it one because the builtin
/// parameter and lifecycle queryables shared this pool; phase-461 W2 gives
/// those two families rings of their own, so the derivation below publishes
/// the app's DEMAND unfloored (RFC-0100 D7) and this constant is what an image
/// that derives NOTHING keeps.
const SERVICE_BUFFER_SIZE_DEFAULT: usize = 1024;

/// The per-queryable request ring depth when nothing states one -- for both
/// shim families. Phase 237 follow-up's 4, chosen for the action path ("a
/// burst of queries delivered in one read-task batch") and equal to the C
/// shim's `ZPICO_MAX_PENDING_REPLIES` on purpose; phase-461 W1 makes it a knob
/// per family so the parameter family can stop paying it (issue 1352).
const SERVICE_INBOX_DEPTH_DEFAULT: usize = 4;

/// phase-461 W1 -- how many queryables this image's declared action servers
/// will register per session: three each (`send_goal`, `cancel_goal`,
/// `get_result`; the `/status` cache queryable is a transient-local publisher's
/// and takes no inbox). ZERO with no descriptor or no `action_server` row,
/// and zero is the single-table answer: the action table is then empty and an
/// action queryable draws a user-service ring in `shim/service.rs`.
///
/// Counted from the same descriptor `declared_service_request_bytes` reads,
/// so the count is exact on a leaf that states its endpoints and absent on
/// one that does not -- never a guess.
fn declared_action_queryables(desc: Option<&SizingDescriptor>) -> usize {
    desc.map_or(0, |d| {
        d.endpoints
            .iter()
            .filter(|e| matches!(e.kind, EndpointKind::ActionServer))
            .count()
            * 3
    })
}

/// phase-461 W2b - queryables this image's own DECLARATION attributes to the
/// APPLICATION, per session, or `None` for "nobody said".
///
/// The builtin families have no count here, and deliberately so. A service
/// server IS a queryable, so `ZPICO_MAX_QUERYABLES` is already
/// `app + infra + transient-local` on a declared image
/// (`nros-zpico-build::queryable_default_from`), and the arithmetic this crate
/// can do is the SUBTRACTION: the slots the declaration did not attribute to
/// the application are the runtime's. Restating the infrastructure counts here
/// is what issue 0827 forbids and `check-infra-queryable-counts` refuses --
/// this crate does not depend on `nros-node` and can see neither the constants
/// nor whether their features are compiled in, so a number stated here is a
/// number that drifts.
///
/// `None` rather than 0 for the undeclared case, because the two
/// directions are opposite. For the TABLE's size an absent declaration means
/// "assume the infrastructure is present" -- over-reserving costs RAM and
/// under-reserving fails at boot. For the ring GEOMETRY it must mean the other
/// thing: an absent declaration must not shrink anybody's ring, so it leaves
/// the builtin table EMPTY and every queryable on the geometry it has today,
/// byte for byte.
///
/// **The same two inputs the TABLE was sized from, and no others.** Both terms
/// are read exactly as `nros-zpico-build` reads them --
/// `NROS_DECLARED_SERVICE_SERVERS` for the application (an action server's
/// three channels are three service servers on that road, which is why
/// neither side adds an action term of its own), and the transient-local
/// publishers descriptor-first with their carrier behind it. A subtraction is
/// only sound against the number it subtracts from: counting the DESCRIPTOR's
/// `service_server` rows here while the table had been sized from the env
/// carrier would let the two disagree, and the leftover would not be the
/// runtime's share.
///
/// **The Zephyr resolver road (issue 1485).** A Zephyr west entry receives
/// neither carrier above: its queryable table is `NROS_MAX_QUERYABLES`, which
/// `nros_resolve_knobs()` resolves from the entity inventory's
/// `NROS_DERIVED_MAX_QUERYABLES`, and that road never ran the CMake road's
/// `nros_entity_facts_env`. So the inventory publishes the application's share
/// of that SAME derivation, `NROS_ENTITY_APP_QUERYABLES` (servers, three per
/// action server, and the transient-local cache queryables -- everything in
/// the table that is not the runtime's), and the resolver forwards it beside
/// the per-kind counts. Same rule as above: the subtraction is taken against
/// the number the table was sized from. Read SECOND, because an image that
/// has the CMake road's carriers sized its table from them.
fn declared_app_queryables(tl: Option<usize>) -> Option<usize> {
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_TL_PUBLISHERS");
    match declared_usize("NROS_DECLARED_SERVICE_SERVERS") {
        Some(app) => Some(
            app + tl
                .or_else(|| declared_usize("NROS_DECLARED_TL_PUBLISHERS"))
                .unwrap_or(0),
        ),
        None => declared_usize("NROS_ENTITY_APP_QUERYABLES"),
    }
}

/// phase-461 W2b - the largest REQUEST the contract's declared parameters can
/// produce, in bytes, rounded up to a multiple of 4 so the ring's slots stay
/// word-aligned on every target the tree builds for.
///
/// `None` when this crate cannot answer, which is both an absent declaration
/// and a declaration it is not entitled to price -- see below. The caller then
/// keeps the user-service slot, which is today's size.
///
/// # The authority is `nros-node`, and this is the half that needs no board
///
/// `nros_node::parameter_services::node_bound` is where the parameter family's
/// worst messages are priced, beside the serializers it bounds and held by the
/// test that serializes them (`the_worst_messages_fit_the_derived_bound`).
/// phase-461 W2 adds `ParamServiceBound::request_max()` over it: the largest of
/// the THREE request fields, because the other five are replies that travel the
/// other way through the executor-side buffer pair and never touch an inbox.
///
/// Three of that function's terms need the parameter STORE's capacities --
/// how long a string, an array or a byte array may be -- and those are board
/// facts `nros-params`' build script resolves. This crate has no business
/// reading them: a second resolution of one number is issue 1025 exactly.
///
/// So this ABSTAINS on any declaration whose worst request depends on them,
/// which is precisely a shape declaring a string, byte-array, bool-array,
/// word-array or string-array parameter (fields 4..9 of the nine-count token).
/// With those five zero the three request bounds are the shape alone, and the
/// arithmetic below is `node_bound`'s, term for term:
///
/// ```text
/// head          = CDR_HEADER + CDR_SEQ                      (4 + 7)
/// names         = name_bytes + params * CDR_STR             (CDR_STR = 3 + 4 + 1)
/// prefixes      = prefix_bytes + prefixes * CDR_STR
/// values        = params * CDR_VALUE_BASE                   (53, no data to add)
/// names_request = head + names
/// list_request  = head + prefixes + CDR_WORD                (CDR_WORD = 7 + 8)
/// set_request   = head + names + values
/// ```
///
/// Each request addresses ONE node's services, so the worst node decides it and
/// the maximum is taken over the rows rather than summed. On the Autoware
/// Safety Island's worst node (`8:170:1:17:0:0:0:0:0`) that is a 669 B
/// `set_parameters`, 672 B rounded -- the same number W2 derives in `nros-node`
/// from the same token, which is the check that keeps the two halves honest.
///
/// A MALFORMED token is not this crate's to refuse: `nros-node`'s build script
/// owns the grammar and panics naming it. Here it reads as "cannot answer".
fn declared_param_request_max(desc: Option<&SizingDescriptor>) -> Option<usize> {
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_PARAM_SERVICE_SHAPE");
    let raw = desc
        .and_then(|d| d.params.service_shape().into_stated())
        .or_else(|| declared_fact("NROS_DECLARED_PARAM_SERVICE_SHAPE"))?;
    param_request_max_from(raw.trim())
}

/// The rule, with the environment lifted out of it: the caller reads the two
/// roads, this takes the token. A build script cannot carry a `#[cfg(test)]`
/// module that anything runs -- cargo compiles build.rs as a host binary and
/// `cargo test` never touches it -- so the evidence for this one is the
/// negative control in the commit message, run over the emitted constant.
fn param_request_max_from(raw: &str) -> Option<usize> {
    /// The 4-byte encapsulation header both halves begin with.
    const CDR_HEADER: usize = 4;
    /// A sequence length: up to 3 bytes of padding to 4, then a `u32`.
    const CDR_SEQ: usize = 3 + 4;
    /// A string beyond its bytes: padding, the `u32` length (which counts the
    /// NUL), and the NUL.
    const CDR_STR: usize = 3 + 4 + 1;
    /// An 8-byte field after anything: up to 7 bytes of padding, then 8.
    const CDR_WORD: usize = 7 + 8;
    /// One `ParameterValue` with no data in it. The worst over all eight start
    /// alignments; `nros-node` states why it is 53 and not the 68 the per-field
    /// worsts sum to.
    const CDR_VALUE_BASE: usize = 53;

    if raw.is_empty() {
        return None;
    }
    let mut worst = 0usize;
    for node in raw.split(',') {
        let f: Option<Vec<usize>> = node.split(':').map(|v| v.trim().parse().ok()).collect();
        let f = f.filter(|f| f.len() == 9)?;
        // Fields 4..9 are the counts whose bound needs the store's
        // capacities. One of them and this crate abstains, for the whole
        // image: a bound that is right for three nodes and guessed for the
        // fourth is not a bound.
        if f[4..9].iter().any(|&n| n != 0) {
            return None;
        }
        let head = CDR_HEADER + CDR_SEQ;
        let names = f[1] + f[0] * CDR_STR;
        let prefixes = f[3] + f[2] * CDR_STR;
        let values = f[0] * CDR_VALUE_BASE;
        worst = worst
            .max(head + names)
            .max(head + prefixes + CDR_WORD)
            .max(head + names + values);
    }
    Some(worst.next_multiple_of(4))
}

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

/// The TRANSIENT_LOCAL retention slots this image's declarations ask for.
///
/// phase-455 W5 / issue 1341. The COUNT is not derived here — it is
/// `nros_sizing_descriptor::transient_local_publishers`, shared with
/// `nros-zpico-build`, which adds the same number to the queryable table
/// because a transient-local publisher costs one slot in each. Two derivations
/// of one number is issue 1025, and here the two pools would disagree by
/// exactly the rows one of them forgot.
///
/// This wrapper only turns the descriptor's three answers into the two a
/// consumer needs — a demand to floor, or nothing — and prints the refusal,
/// because a refusal that reaches no log is a default nobody chose (D6).
///
/// **No descriptor: the DECLARED carrier.** A cmake / Zephyr west / NuttX entry
/// names no descriptor to cargo (issue 1393), and before this the pool then
/// kept its builtin of [`TL_PUBLISHERS_DEFAULT`] while the queryable table --
/// sized from the same rule on the same road -- counted every transient-local
/// publisher. Measured on the Autoware Safety Island (Zephyr, west): table 31,
/// pool 2, five latched publishers, and the third `create_publisher` failed at
/// boot. `NROS_DECLARED_TL_PUBLISHERS` is that road's carrier (the CMake road
/// composes it in `NanoRosEntityFacts.cmake`, the Zephyr resolver forwards the
/// entity inventory's `NROS_DERIVED_TL_PUBLISHERS` under the same name), and
/// `nros-zpico-build` already reads it for the table. The descriptor still
/// wins when both are present, as it does there.
fn transient_local_publisher_demand(desc: Option<&SizingDescriptor>) -> Option<usize> {
    let Some(desc) = desc else {
        return declared_transient_local_publishers(declared_fact("NROS_DECLARED_TL_PUBLISHERS"));
    };
    match nros_sizing_descriptor::transient_local_publishers(desc) {
        Fact::Stated(n) => Some(n),
        Fact::Absent => None,
        Fact::Refused(reason) => {
            warn(&format!(
                "{reason}. The transient-local retention pool keeps \
                 {TL_PUBLISHERS_DEFAULT} slot(s) (ZPICO_MAX_TL_PUBLISHERS)"
            ));
            None
        }
    }
}

/// The DECLARED road's transient-local count, or `None` for "nobody said".
///
/// The same three spellings `nros-zpico-build`'s reader of this carrier
/// accepts: absent (or empty, issue 1429) is undeclared; the word `refused` is
/// a composer that looked and could not answer, which keeps the builtin and
/// says so; a count is the demand. A malformed value panics there and here,
/// for the reason given there -- a value that reads as applied and is not is
/// worse than no value.
fn declared_transient_local_publishers(v: Option<String>) -> Option<usize> {
    let v = v?;
    let v = v.trim();
    if v == "refused" {
        warn(&format!(
            "NROS_DECLARED_TL_PUBLISHERS=refused: the entry declares a publisher whose \
             `durability` nothing states, so no count of transient-local publishers is a \
             bound. The transient-local retention pool keeps {TL_PUBLISHERS_DEFAULT} \
             slot(s) (ZPICO_MAX_TL_PUBLISHERS)"
        ));
        return None;
    }
    match v.parse::<usize>() {
        Ok(n) => Some(n),
        Err(_) => panic!(
            "NROS_DECLARED_TL_PUBLISHERS={v:?} is neither a count nor `refused`. It is how \
             many TRANSIENT_LOCAL publishers the entry declares, which sizes the retention \
             pool (ZPICO_MAX_TL_PUBLISHERS)."
        ),
    }
}

/// `ZPICO_MAX_TL_PUBLISHERS` as a CHECKED override, shaped after
/// `nros-zpico-build`'s `resolve_queryable_default`.
///
/// A stated value BELOW the declared demand is refused at BUILD time naming the
/// knob, because the alternative is `create_publisher` returning
/// `IncompatibleQos` at boot on an image whose every gate read green — which is
/// issue 1341's symptom, and the reason phase-455 W5 asked for this to be a
/// compile-stage answer rather than a runtime `-80`.
fn resolve_max_tl_publishers(demand: Option<usize>) -> usize {
    let default = demand.unwrap_or(TL_PUBLISHERS_DEFAULT);
    let requested = env_usize("ZPICO_MAX_TL_PUBLISHERS", default);
    if let Some(demand) = demand {
        if requested < demand {
            panic!(
                "ZPICO_MAX_TL_PUBLISHERS={requested} cannot serve this image's {demand} \
                 declared TRANSIENT_LOCAL publisher(s).\n  \
                 A transient-local publisher retains its last sample and answers a \
                 late-joining subscriber's history query, so it costs one retention \
                 slot here AND one slot in the zenoh queryable table \
                 (ZPICO_MAX_QUERYABLES) — an action server's \
                 `<action>/_action/status` is one of these, and every action_server \
                 row counts one whether or not it states a durability.\n  \
                 Raise ZPICO_MAX_TL_PUBLISHERS, or stop declaring the endpoint. \
                 It has no Kconfig row yet, so a Zephyr image sets the env var \
                 like every other unmapped knob in KCONFIG_KNOBS.\n  \
                 Each slot costs ZPICO_TL_RETAIN_BYTES plus the keyexpr and \
                 attachment it replies with."
            );
        }
    }
    requested
}

/// The transient-local retention slots an image that declares nothing keeps.
///
/// TWO, and the number is a policy rather than a measurement: one action server
/// is the motivating case (issue 1341) and two lets a second one — or one
/// action server beside one latched topic — build without a knob. Every image
/// that DOES declare its endpoints derives its own count, zero included, so
/// this is only ever the undeclared road's answer.
const TL_PUBLISHERS_DEFAULT: usize = 2;

/// Bytes one retained transient-local sample may hold.
///
/// Shares `NROS_SUBSCRIBER_BUFFER_SIZE`'s 1024 for the same reason the
/// publisher TX arena does (issue 0813): it is the other side of the same wire
/// expectation. A sample that does not fit is NOT retained and says so once,
/// naming `ZPICO_TL_RETAIN_BYTES` — silently retaining a truncated sample would
/// serve a late joiner garbage under a profile that promises it the last value.
const TL_RETAIN_BYTES_DEFAULT: usize = 1024;

/// Which inbox family [`declared_service_request_bytes`] is pricing.
///
/// The two are not one number any more (phase-461 W1 gave them separate rings
/// and separate knobs), and they are not one POPULATION either: a user service
/// server receives `pkg/srv/Name_Request`, an action server's three queryables
/// receive the SendGoal envelope. Pricing them together would give each family
/// the other's worst case -- which is the flat table this phase exists to
/// remove, one level finer.
type InboxFamily = &'static [EndpointKind];

/// The user-service family: a service server and the client that answers to it.
const SERVICE_FAMILY: InboxFamily = &[EndpointKind::ServiceServer, EndpointKind::ServiceClient];

/// The action family, whose queryables are the twin of `ZPICO_MAX_PENDING_REPLIES`.
const ACTION_FAMILY: InboxFamily = &[EndpointKind::ActionServer, EndpointKind::ActionClient];

/// The request-slot size one inbox FAMILY's declarations ask for.
///
/// RFC-0100 D2 gives the pool as `COUNT x SLOTS x SLOT_BYTES`, so `SLOT_BYTES`
/// is the largest request a queryable of this family can receive. `None` leaves
/// the caller's default standing.
///
/// # The floor is gone, and phase-461 W2 is why
///
/// phase-454 W6.a wrote this function with a `.max(SERVICE_BUFFER_SIZE_DEFAULT)`
/// and stated the reason: a zenoh service server IS a queryable, and **eleven
/// of them exist before the app declares anything** -- `[param_services]` (6)
/// and `[lifecycle]` (5), issue 0460's measurement. Those servers received
/// `rcl_interfaces/srv/*` and `lifecycle_msgs/srv/*` requests through this very
/// pool and appear in no `[[endpoint]]` row, so sizing down to what the app
/// declared would under-size a surface the declaration structurally cannot
/// mention.
///
/// W2 moves both families onto rings of their OWN, sized by nros-node -- the
/// crate that can price a parameter request, because it holds the store's
/// capacities. Nothing shares this pool with the app any more, so the floor's
/// premise is gone and D7's rule stands again: publish the DEMAND, and let a
/// consumer that needs a floor apply its own. On the island that is the
/// difference between 1,024 B and 24 B per user-service slot.
///
/// # The join, and why it used to refuse on every in-tree image
///
/// A service row's `type` is `pkg/srv/Name`, and no such type crosses a wire.
/// The bound belongs to `pkg/srv/Name_Request`, which
/// `BoundInventory::record_message` never saw -- it was called for `.msg` files
/// and for nothing else -- so `wire_bound_bytes` was REFUSED on every service
/// and action row and this function returned `None` every time. phase-461 W3
/// prices those types (`BoundInventory::record_service` / `record_action`) and
/// `sizing_descriptor::wire_type_of` joins on them, so the refusal below now
/// means what it says: something in the closure genuinely has no bound.
fn declared_service_request_bytes(
    desc: Option<&SizingDescriptor>,
    family: InboxFamily,
) -> Option<usize> {
    let desc = desc?;
    let svc: Vec<_> = desc
        .endpoints
        .iter()
        .filter(|e| family.contains(&e.kind))
        .collect();
    if svc.is_empty() {
        return None;
    }
    let mut max = 0usize;
    for e in &svc {
        match e.wire_bound_bytes() {
            Fact::Stated(b) => max = max.max(b),
            // ONE unpriced row refuses the whole derivation: the slot is shared
            // by every queryable of the family, so a maximum over the rows that
            // answered is not a bound on the rows that did not.
            f => {
                warn(&format!(
                    "sizing descriptor states no `wire_bound_bytes` for {} {} ({}): {}. The \
                     {} inbox slot keeps its stated or default size",
                    e.kind.tag(),
                    e.topic,
                    e.type_name,
                    f.refusal().unwrap_or("nothing derived it"),
                    if family == ACTION_FAMILY {
                        "action"
                    } else {
                        "service"
                    },
                ));
                return None;
            }
        }
    }
    Some(max)
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
    // phase-461 W1 -- the per-family inbox pairs, RMW-agnostic names on the
    // Kconfig side (phase-403's rule) and resolved under those names by
    // `nros_resolve_knobs()`, so W3's `NROS_DERIVED_*` twin has its
    // `NROS_RESOLVED_NROS_*` counterpart waiting.
    (
        "NROS_SERVICE_INBOX_BYTES",
        "CONFIG_NROS_SERVICE_INBOX_BYTES",
    ),
    (
        "NROS_SERVICE_INBOX_DEPTH",
        "CONFIG_NROS_SERVICE_INBOX_DEPTH",
    ),
    ("NROS_ACTION_INBOX_BYTES", "CONFIG_NROS_ACTION_INBOX_BYTES"),
    ("NROS_ACTION_INBOX_DEPTH", "CONFIG_NROS_ACTION_INBOX_DEPTH"),
    // issue 1490 -- three knobs this table forwarded to nothing.
    //
    // MEASURED, not inferred: `CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7` in
    // `examples/zephyr/rust/talker/prj.conf` reached the build's `.config` and
    // the Rust half still compiled `SUBSCRIBER_RING_DEPTH: usize = 4`. The
    // baseline could not have shown it -- unset, the Kconfig default and the
    // crate default are both 4, so "delivered" and "fell back to the same
    // number" are one observation. Issue 0460, in the crate that resolves
    // through this table.
    //
    // They were invisible to `check-kconfig-knob-forwarding` because its
    // per-knob arm asked whether a reader MENTIONS the name, and all three are
    // mentioned here -- in the `rerun-if-env-changed` list above, and at the
    // call site. That is issue 0751's finding one arm over; the gate now asks
    // a tabulating reader for a ROW.
    (
        "ZPICO_SUBSCRIBER_RING_DEPTH",
        "CONFIG_NROS_SUBSCRIBER_RING_DEPTH",
    ),
    // The param-service inbox pair is issue 1233's shape rather than plain
    // 0460: `nros-node` reads the same two knobs through the DERIVED spelling
    // and so takes the Kconfig value, while this crate took the default -- two
    // crates sizing ONE geometry from two numbers.
    (
        "NROS_PARAM_SERVICE_INBOX_BYTES",
        "CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES",
    ),
    (
        "NROS_PARAM_SERVICE_INBOX_DEPTH",
        "CONFIG_NROS_PARAM_SERVICE_INBOX_DEPTH",
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

/// issue 1485 -- the refusal for a STATED `NROS_PARAM_SERVICE_INBOX_BYTES=0`,
/// word for word the one `nros-node/build.rs` raises for the same knob (RFC-0065
/// D2: refuse, and name the remedy), so a board that states 0 is told one thing
/// whichever build script runs first.
const PARAM_INBOX_ZERO_REFUSAL: &str = "NROS_PARAM_SERVICE_INBOX_BYTES=0: 0 is not a size; -1 derives.\n  \
     It is the bytes ONE request slot of the parameter/lifecycle inbox holds, \
     and a 0-byte slot drops every request. 0 was this knob's derive sentinel \
     until issue 1485 and the readers took it literally.\n  \
     Delete the line (on Zephyr, CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES, whose \
     default -1 derives the size from the contract's declared parameters), \
     or state the size you mean.";

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

/// issue 1122 / 0460 - the string twin of [`declared_usize`], for a carrier
/// that is a TOKEN rather than a count. Named as the readers of the same
/// road in `nros-node` and `nros-zpico-build` are named, and for the same
/// reason: one idiom for the DECLARED carriers, which is what keeps them in
/// the config census (issue 1199).
///
/// The name is an ARGUMENT and not a literal at the `env::var` call, which is
/// what `check-kconfig-knob-forwarding` requires of every forwarded knob: a
/// literal `env::var("<forwarded knob>")` in a build script yields the crate
/// default on a Zephyr Rust image whatever Kconfig says, which is issue 0460's
/// shape. This fact has no `CONFIG_` symbol to miss -- cmake forwards it
/// through the environment and nowhere else -- so the environment is the right
/// and only rung, exactly as `nros-node/build.rs` reads the same carrier.
fn declared_fact(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

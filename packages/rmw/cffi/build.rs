//! Build script. Three responsibilities:
//!
//! 1. Phase 104.B.1 — read `NROS_RMW_MAX_BACKENDS` (env var, default
//!    8) and re-emit it as a `cargo:rustc-env` so the crate's source
//!    can read it via `env!("NROS_RMW_MAX_BACKENDS")`. Mirrors the
//!    `NROS_EXECUTOR_MAX_CBS` / `NROS_LET_BUFFER_SIZE` pattern in
//!    `nros-node`. Cortex-M0+ users can drop to 2; bridge users on
//!    companion-class hardware can bump to 16. Range [1, 64];
//!    values outside the range fail the build.
//!
//! 2. phase-454 W6.e (RFC-0100 D5) — take the DEFAULT for three of those
//!    pools from the image's sizing descriptor instead of from a literal.
//!    RFC-0100 D5's cffi row is three counts and this is where they land:
//!
//!    ```text
//!    NROS_RMW_MAX_BACKENDS      [image] backend_count
//!    NROS_RMW_MAX_NODES         [image] node_count
//!    NROS_RMW_SUBSCRIBER_SLOTS  [image] subscriber_count
//!    ```
//!
//!    `NROS_RMW_MESSAGE_INFO_SLOTS` is NOT among them and must not be:
//!    `MessageInfoSlot`'s width is cfg-dependent (`alloc` + `safety-e2e` add
//!    three fields), which is why it carries a documented deliberate
//!    non-annotation in `lib.rs` rather than a formula. Neither is
//!    `SLOT_SIZE` — a hard 1024 where a size overflow and a full pool return
//!    the same code (issue 1322), and D5 rules it out by name: no user fact
//!    answers "how big is a backend's private state struct".
//!
//! 3. Phase 115.G.4 — compile `tests/c_stubs/c_stub_transport.c`
//!    into a small static lib for the second-language smoke test.
//!    Gated behind the `c-stub-test` Cargo feature so consumers of
//!    `nros-rmw-cffi` that vendor it without a C toolchain on the
//!    build host aren't forced through the cc invocation.

use nros_board_common::platform_config::{BuildRungs, RmwKnobs};
use nros_sizing_descriptor::{Fact, SizingDescriptor};

fn main() {
    watch_declared_facts();
    // phase-400 W6 — the platform/board rungs for `[knobs.rmw]`, resolved once.
    // `None` when no lane named a platform, which is every out-of-tree consumer
    // and every plain `cargo build` here; the builtins then stand.
    let rungs = BuildRungs::from_build_env()
        .map(|r| r.rmw_rungs())
        .unwrap_or_default();
    // phase-454 W6.e — what this IMAGE declares. `None` for a build nobody ran
    // `nros sync` for, which is every plain `cargo build` here and every
    // out-of-tree consumer; all three knobs below then size exactly as they did
    // before this wave.
    let sizing = sizing_descriptor();

    emit_max_backends(&rungs, sizing.as_ref());
    emit_subscriber_slots(sizing.as_ref());
    emit_message_info_slots(&rungs);
    maybe_build_c_stub();
}

/// The descriptor this build was pointed at, or `None`.
///
/// A parse or schema failure PANICS, which is a build error naming the file.
/// That is the negative control for the whole transport: a descriptor EXISTS, so
/// keeping our own literals here would size three static pools from numbers a
/// user believes they supplied. Same three outcomes as `nros-node`'s reader —
/// absent is silent, refused is loud, broken is fatal.
fn sizing_descriptor() -> Option<SizingDescriptor> {
    match nros_sizing_descriptor::from_build_env() {
        // `Ok(None)` is the variable being UNSET, which is the ordinary case.
        // `Err(Missing)` is a path that was NAMED and is not there, and that is
        // an error for the same reason as the rest: somebody pointed this build
        // at a descriptor.
        Ok(d) => d,
        Err(e) => panic!("{e}"),
    }
}

/// A descriptor count, as the rung under an operator's stated value.
///
/// RFC-0049's ladder is unchanged and RFC-0100 D6 says where a derived number
/// sits in it: *"a policy fact is stated, and a derived fact may supply its
/// default"*. So an env var, a `CONFIG_*` symbol and a `[knobs.rmw]` rung all
/// still win; this replaces the crate BUILTIN, which is a literal nobody chose
/// for this image.
///
/// LOUD on a refusal, D6's second half — *"always the safe direction and always
/// loud"*. The safe direction for all three of these pools is the larger
/// builtin, so a refusal costs memory rather than correctness, and the warning
/// says what declaring would have bought.
fn derived_rung(fact: &Fact<usize>, what: &str, builtin: usize) -> Option<usize> {
    match fact {
        Fact::Stated(v) => Some(*v),
        Fact::Refused(why) => {
            println!(
                "cargo::warning=nros-rmw-cffi: sizing descriptor refused `{what}`: {why}; the \
                 pool keeps its builtin {builtin}, which over-states this image rather than \
                 under-sizing it"
            );
            None
        }
        // ABSENT is not an event. A descriptor that states no count for a
        // section nobody derived has nothing to report, and a warning on every
        // such build is the guard that fires on every call.
        Fact::Absent => None,
    }
}

/// Resolve a sizing knob the way every Zephyr-aware build script here does.
///
/// A plain `env::var` reads the crate DEFAULT on a Zephyr RUST image whatever
/// Kconfig says: that lane inherits none of cmake's `set(ENV{...})` exports,
/// while the C lane re-bakes them into its own command — so the two halves
/// disagree about a compile-time constant, silently (issues 0460, 0135).
/// `knob_usize` reads `$DOTCONFIG` for `CONFIG_<name>` and lets explicit env
/// win, which is what `nros-node` and `nros-params` already do.
///
/// Note this drops the previous "not a valid usize" panic: `knob_usize` treats
/// an unparseable value as absent and falls through to the default. The RANGE
/// checks below are kept, since they are the ones that catch a plausible-looking
/// wrong number.
/// phase-400 W6 — `rung` is the platform/board answer from `[knobs.rmw]`, and
/// it sits between the Kconfig rung and the crate builtin. Passing it as
/// `knob_usize`'s default is what places it there: env and `$DOTCONFIG` still
/// win above it, and the builtin only applies when no descriptor said anything.
// issue 1199 — see the note in `nros-zpico-build`'s `watch_declared_facts`:
// spelled literally so the wire is greppable, alongside the dynamic emission.
fn watch_declared_facts() {
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_RMW_SUBSCRIBER_SLOTS");
}

fn knob(name: &str, rung: Option<usize>, default: usize) -> usize {
    nros_zephyr_build::knob_usize(name, &format!("CONFIG_{name}"), rung.unwrap_or(default))
}

/// issue 1199 — a `NROS_DECLARED_*` count cmake derived for THIS image.
///
/// `None` when cmake made no claim: the carrier is written only when the entity
/// inventory's status is `derived`, so absent means "no answer" and never
/// "zero". Zero is a legal answer for this knob, which is exactly why the two
/// cannot share a spelling.
fn declared_usize(name: &str) -> Option<usize> {
    println!("cargo:rerun-if-env-changed={name}");
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// phase-454 W6.e — `MAX_BACKENDS` and `MAX_NODES`, both derived.
///
/// **`MAX_BACKENDS`** is the count of distinct RMW backends the image links.
/// Every leaf that gets a descriptor names exactly one (`LeafSystem::rmw` is a
/// single value, and `nros build` generates one `register()` call for it); a
/// bridge that binds several carries no `system.toml`, so no descriptor is
/// written for one and it keeps the builtin 8. Under-sizing this registry is a
/// REGISTRATION FAILURE and not a truncation — `nros_rmw_cffi_register` returns
/// non-OK and `register()` returns `Err` — so the number is refused rather than
/// guessed wherever the producer cannot name the backend.
///
/// **`MAX_NODES`** is `components().len()`, which the entity inventory has
/// derived since phase-412 and handed only to `NROS_EXECUTOR_MAX_NODES`. The
/// comment below has said "the default mirrors the executor's `MAX_NODES`" for
/// four phases, and that was true of the two DEFAULTS and of nothing else: a
/// declared image sized the executor's node table per image and left this one at
/// 4. Now both read one count.
fn emit_max_backends(rungs: &RmwKnobs, sizing: Option<&SizingDescriptor>) {
    let backends = rungs.max_backends.or_else(|| {
        sizing.and_then(|d| derived_rung(&d.image.backend_count(), "backend_count", 8))
    });
    let parsed = knob("NROS_RMW_MAX_BACKENDS", backends, 8);

    if !(1..=64).contains(&parsed) {
        panic!(
            "NROS_RMW_MAX_BACKENDS={parsed} out of range [1, 64]. Bump \
             the build script's upper bound if a larger value is truly \
             needed."
        );
    }

    println!("cargo:rustc-env=NROS_RMW_MAX_BACKENDS={parsed}");

    // Phase 376 W5/B1 — distinct `(name, namespace)` pairs one session hosts.
    // Default 4, matching `nros-node`'s `NROS_EXECUTOR_MAX_NODES`: the shim
    // cannot see more nodes than the executor will register. phase-454 W6.e —
    // and now they read ONE count rather than two equal literals.
    let node_rung = rungs
        .max_nodes
        .or_else(|| sizing.and_then(|d| derived_rung(&d.image.node_count(), "node_count", 4)));
    let nodes = knob("NROS_RMW_MAX_NODES", node_rung, 4);
    if !(1..=64).contains(&nodes) {
        panic!(
            "NROS_RMW_MAX_NODES={nodes} out of range [1, 64]. Bump \
             NROS_EXECUTOR_MAX_NODES to match if you raise it."
        );
    }
    println!("cargo:rerun-if-env-changed=NROS_RMW_MAX_NODES");
    println!("cargo:rustc-env=NROS_RMW_MAX_NODES={nodes}");
}

/// Issue 0269 — size the embedded (`target_os = "none"`, no-std)
/// subscription-handle pool in `rust_adapter::static_subscriber_storage`.
/// The old hardcoded 4 silently capped a session at four subscriptions:
/// the fifth `create_subscription` returned BAD_ALLOC, which the
/// executor then surfaced as an opaque `SubscriberCreationFailed`.
fn emit_subscriber_slots(sizing: Option<&SizingDescriptor>) {
    // NO ladder rung, deliberately: phase-412 W1 derives this from the
    // entity inventory (`COUNT_SUBSCRIPTION`). Two campaigns resolving one
    // knob is the drift issue 0938 cost, so this one keeps env -> Kconfig ->
    // builtin and takes its platform answer from the derivation instead.
    // issue 1199 — the rung is the DECLARED count cmake derived for this image.
    // It was `None` because phase-412 W1 delivered this knob through the Zephyr
    // resolver only; the DECLARED road carries the same number on the lanes
    // that have no Kconfig. NOT floored: zero is legal here and measured --
    // `[T; 0]` is ordinary Rust, the only access is `for index in
    // 0..SLOT_COUNT`, and issue 1033 retracted the clamp that rounded it to 1.
    //
    // phase-454 W6.e — the DESCRIPTOR is now the first of those two roads, and
    // the `NROS_DECLARED_*` carrier is the fallback rather than the answer.
    // They cannot disagree today: both are `DerivedEntityKnobs::max_subscribers`
    // from one `EntityInventory::derive`, which is the whole point of RFC-0100
    // D4 — *"one artifact, one road"* — and the reason `[image] subscriber_count`
    // carries the TOTAL rather than leaving a build script to count
    // `kind = "action_client"` rows and multiply. That multiplier is one an
    // action's creation calls own (`check-infra-queryable-counts` holds it
    // there), and a third mirror of it, in a build script no gate scans, is how
    // an image with an action gets sized short.
    //
    // The carrier stays because it is the road the C/C++ CMake lane takes and
    // that lane has no descriptor yet; removing the read would take those images
    // from a derived count back to the builtin 8. W9 is where it retires.
    let declared = sizing
        .and_then(|d| derived_rung(&d.image.subscriber_count(), "subscriber_count", 8))
        .or_else(|| declared_usize("NROS_DECLARED_RMW_SUBSCRIBER_SLOTS"));
    let parsed = knob("NROS_RMW_SUBSCRIBER_SLOTS", declared, 8);

    // issue 1033 — ZERO is in range. A pub-only image derives 0 subscriptions
    // from the entity inventory, and 0 slots is the honest answer: `[T; 0]` is
    // ordinary Rust (no zero-length-array question as in C), and the only
    // access is `for index in 0..SLOT_COUNT`, which at 0 does not run.
    //
    // The floor was 1 while the derivation could never produce 0 — every image
    // refused for want of a declaration, so the knob fell to its default of 8
    // and the bound was never exercised. Making the derivation reachable turned
    // that latent disagreement into a build failure on the zephyr cpp talker,
    // which is the first image ever to derive 0 here. Refusing it would clamp,
    // and clamping is what issue 0827 forbids: it "reserved the memory anyway
    // while reading as though the knob had been honoured".
    if !(0..=128).contains(&parsed) {
        panic!(
            "NROS_RMW_SUBSCRIBER_SLOTS={parsed} out of range [0, 128]. Each \
             slot is a 1 KiB static; bump the upper bound only with the \
             memory budget in hand."
        );
    }

    println!("cargo:rustc-env=NROS_RMW_SUBSCRIBER_SLOTS={parsed}");
}

/// Issue 0271 — size the `MESSAGE_INFO_TABLE` pool.
///
/// Was hardcoded at 64 while every comparable pool in the tree is env-tunable,
/// so a 256 KB-class image carried 3.5 KB of `.bss` for message metadata it
/// never used: measured 3,136 bytes of live BTCM on the Orin SPE at 4
/// subscribers. The table is keyed by backend handle, so the useful bound is
/// the session's subscription count, not a fixed 64.
fn emit_message_info_slots(rungs: &RmwKnobs) {
    let parsed = knob("NROS_RMW_MESSAGE_INFO_SLOTS", rungs.message_info_slots, 64);

    if !(1..=256).contains(&parsed) {
        panic!(
            "NROS_RMW_MESSAGE_INFO_SLOTS={parsed} out of range [1, 256]. The \
             table is keyed by backend subscriber handle, so sizing it below \
             the session's subscription count costs metadata, not correctness \
             — `MessageInfo` simply reads back `None`."
        );
    }

    println!("cargo:rustc-env=NROS_RMW_MESSAGE_INFO_SLOTS={parsed}");
}

fn maybe_build_c_stub() {
    println!("cargo:rerun-if-changed=tests/c_stubs/c_stub_transport.c");
    println!("cargo:rerun-if-changed=tests/c_stubs/c_stub_transport.h");
    println!("cargo:rerun-if-changed=tests/c_stubs/abi_layout_check.c");
    // issue 0490 — `../../core/nros-rmw-abi`, not `../nros-rmw-abi`. This crate
    // was `packages/core/nros-rmw-cffi` when the line was written; phase-321
    // W2.e (`12c365774`) moved it to `packages/rmw/cffi` and the relative path
    // came along unchanged, naming a directory that does not exist. Cargo treats
    // a MISSING `rerun-if-changed` input as permanently dirty
    // (`StaleItem(MissingFile{..})`), so this build script — and every crate
    // above it, which for a fixture image is all of them — recompiled on every
    // single invocation. Silent: the build always succeeded, it was just never
    // fresh. Keep it pointing at a real path.
    println!("cargo:rerun-if-changed=../../core/nros-rmw-abi/include/nros");

    if std::env::var_os("CARGO_FEATURE_C_STUB_TEST").is_none() {
        return;
    }

    // issue 0383 — implicit-function-declaration / int-conversion as errors.
    nros_cc_flags::strict_decls(&mut cc::Build::new())
        .file("tests/c_stubs/c_stub_transport.c")
        .include("tests/c_stubs")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_c_stub_transport");

    // ABI-layout single-source-of-truth (issue #238 / #239): a header
    // TU of `_Static_assert`s that pin the C-side widths of the RMW
    // mirror. Its Rust counterpart is the `abi_layout` const-assert
    // block in `src/lib.rs`. If either side's layout drifts, exactly
    // one guard fails the build. Compiled against the public headers.
    // issue 0779 — `../../core/nros-rmw-abi`, the same correction the comment 30
    // lines above already made for the `rerun-if-changed` spelling. 0490 fixed
    // that site and left this one, and the two failed DIFFERENTLY, which is why
    // only one was noticed: a missing `rerun-if-changed` input is silently
    // always-dirty (the build still succeeds), while a missing `-I` is a hard
    // `fatal error: nros/rmw_entity.h: No such file or directory`.
    //
    // The hard one stayed hidden because everything below is behind
    // `CARGO_FEATURE_C_STUB_TEST`, which no recipe enabled — so the layout guard
    // this file exists to run had not compiled since phase-321 W2.e moved the
    // crate. A guard that cannot build is not a guard.
    nros_cc_flags::strict_decls(&mut cc::Build::new())
        .file("tests/c_stubs/abi_layout_check.c")
        .include("../../core/nros-rmw-abi/include")
        .warnings(true)
        .extra_warnings(true)
        .compile("nros_abi_layout_check");
}

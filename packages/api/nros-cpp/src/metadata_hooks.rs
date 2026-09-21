//! phase-308 — the two-function gap the recording RMW backend cannot close.
//!
//! Publishers, subscriptions, services and clients reach the RMW session, so
//! `nros-rmw-metadata` records them with no help from this crate. **Timers and
//! guard conditions never touch the RMW** — they register directly on the
//! executor — so a backend cannot observe them at all. That matters more here
//! than anywhere: a timer is precisely the entity the SystemModel also cannot
//! see, and missing them would reproduce the bug the sidecars exist to fix
//! (issue 0257).
//!
//! Plus one more, found while reading the seam: the RMW's `create_publisher`
//! carries no node — by that layer the owning node is already resolved away.
//! So a backend alone yields a sidecar whose entities belong to no node. The
//! `node_create` hook opens each node and makes it current; `configure()`
//! declares one node's entities at a time, so a cursor is exact, not a guess.
//!
//! Four hooks total (phase-463 W1 added the parameter one). Every one is a
//! no-op unless `metadata-mode` is on, so the call sites in the shipping paths
//! are unconditional and cost nothing.
//!
//! phase-463 W1 -- the hooks tell the whole truth: a timer carries its KIND
//! (which of the four `nros_cpp_timer_create*` entries it came through), a
//! guard condition is recorded under its own kind instead of as a timer, and
//! `on_param_declare` sits on the `nros_cpp_node_declare_param_*` family.
//! C++ has no call that declares a parameter without crossing that ABI, so
//! the set of parameters a sidecar carries is complete by construction.
//! `check-census-hooks-complete` holds every entry point to its hook, and the
//! fixture test at the bottom of this file is the census of a component that
//! creates one of everything.
//!
//! This module records; it does not serialize. No JSON, no schema struct, no
//! slot arithmetic — those live once in `nros::node_metadata` (phase-308's
//! layer constraint).

/// A node was created — make it current so subsequent entities attribute to it.
#[inline]
pub(crate) fn on_node_create(_name: &str, _namespace: &str, _domain_id: u32) {
    #[cfg(feature = "metadata-mode")]
    {
        // A refused begin means the recorder is full; every entity after it
        // would be silently dropped, so say so rather than produce a sidecar
        // that under-counts.
        if !nros::metadata_mode::begin_node(_name, _namespace, _domain_id) {
            panic!(
                "nros metadata mode: recorder rejected node `{_name}` — raise the \
                 MetadataRecorder capacity"
            );
        }
    }
}

/// A timer was registered on the executor.
///
/// Timers carry no name at this ABI (they are bound by function identity —
/// `bind_timer<T, &T::method>`), so the recorded id is synthetic. That is fine:
/// the count is what the executor sizing reads, and a C++ timer has no
/// user-visible name to preserve.
///
/// phase-463 W1 -- `kind` names the entry point (wall / clock / oneshot /
/// in-group) so the census can tell a repeating timer from a one-shot delay;
/// the period is recorded as the code passed it, which for the generated
/// native entry is the LAUNCHED value (the entry seeds parameters before the
/// constructor runs).
#[inline]
pub(crate) fn on_timer_create(_kind: nros::node_metadata::TimerKind, _period_ms: u64) {
    #[cfg(feature = "metadata-mode")]
    {
        record(
            nros::node_metadata::EntityKind::Timer,
            _kind,
            "timer",
            Some(_period_ms),
        );
    }
}

/// A guard condition was registered on the executor. One callback slot, same as
/// a timer.
///
/// phase-463 W1 -- recorded as `TimerKind::GuardCondition`: still a `timers[]`
/// row (one slot each, which is what the sizing consumers count) but with
/// `kind: "guard_condition"`, so the count is unchanged and the census no
/// longer reads a guard as a timer of period 0.
#[inline]
pub(crate) fn on_guard_condition_create() {
    #[cfg(feature = "metadata-mode")]
    {
        record(
            nros::node_metadata::EntityKind::Timer,
            nros::node_metadata::TimerKind::GuardCondition,
            "guard",
            None,
        );
    }
}

/// phase-463 W1 -- a node declared a parameter, with the type and default the
/// code passed.
///
/// Called from every `nros_cpp_node_declare_param_*` entry point, BEFORE the
/// store answers: an adopted launch seed (`NROS_CPP_RET_ALREADY_EXISTS`) is
/// still a declaration the code makes, and a full store is a boot failure the
/// census should still describe. Attributed to the current node through the
/// phase-308 cursor, like every other entity.
#[inline]
pub(crate) fn on_param_declare(_name: &str, _value: &nros::ParameterValue) {
    #[cfg(feature = "metadata-mode")]
    {
        if !nros::metadata_mode::record_parameter(_name, _value) {
            panic!(
                "nros metadata mode: recorder rejected parameter `{_name}` -- a census \
                 built from this sidecar would say the node declares fewer than it does"
            );
        }
    }
}

#[cfg(feature = "metadata-mode")]
fn record(
    kind: nros::node_metadata::EntityKind,
    timer_kind: nros::node_metadata::TimerKind,
    prefix: &str,
    period_ms: Option<u64>,
) {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let id = alloc::format!("{prefix}{n}");
    let rec = nros::metadata_mode::EntityRecord {
        callback_id: Some(&id),
        period_ms,
        timer_kind,
        ..nros::metadata_mode::EntityRecord::new(kind, &id, "")
    };
    if !nros::metadata_mode::record(rec) {
        panic!(
            "nros metadata mode: recorder rejected `{id}` — an executor sized from \
             this sidecar would be too small"
        );
    }
}

/// phase-308 — write the recorded sidecar. The probe's last call.
///
/// Exported from `nros-cpp` rather than from the backend crate so the probe TU
/// links exactly one Rust staticlib (the `nros-cpp` umbrella, phase-241 D3-rev)
/// and needs no extra link line.
///
/// Serialization is `nros::metadata_mode::to_json` — the SAME emitter the Rust
/// producer uses. Nothing here formats anything.
///
/// Returns 0 on success, -1 on a bad argument, -2 if nothing was recorded, -3
/// on a serialize/write failure. Recording NOTHING is an error, not an empty
/// sidecar: a component that declared no entities either failed to run its
/// declaration path or has none, and both are bugs the driver must surface
/// rather than bake a zero into an executor size.
///
/// # Safety
/// Every pointer must be a valid NUL-terminated string.
#[cfg(feature = "metadata-mode")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_metadata_dump(
    package: *const core::ffi::c_char,
    component: *const core::ffi::c_char,
    executable: *const core::ffi::c_char,
    language: *const core::ffi::c_char,
    out_path: *const core::ffi::c_char,
) -> i32 {
    let read = |p: *const core::ffi::c_char| -> Option<&str> {
        if p.is_null() {
            return None;
        }
        unsafe { core::ffi::CStr::from_ptr(p) }.to_str().ok()
    };
    let (Some(package), Some(component), Some(out_path)) =
        (read(package), read(component), read(out_path))
    else {
        return -1;
    };
    if nros::metadata_mode::entity_count() == 0 {
        return -2;
    }
    let mut export = nros::node_metadata::SourceMetadataExport::new(package, component)
        .language(read(language).unwrap_or("cpp"));
    if let Some(exe) = read(executable) {
        export = export.executable(exe);
    }
    let Ok(json) = nros::metadata_mode::to_json(&export) else {
        return -3;
    };
    // phase-359 W10 — this `std` is the CAPABILITY, not a spelling to unwind.
    // `metadata-mode` exists to write this file; a filesystem is what it needs
    // and `std` is where one lives. The guard in `lib.rs` names it, which is the
    // part that was missing. (`nros`'s half of the same feature requires only
    // `alloc`: it records into a heap-allocated global and hands back a
    // `String` — the write is here, and only here.)
    if std::fs::write(out_path, json).is_err() {
        return -3;
    }
    0
}

/// phase-463 W1 -- the census of a fixture component that creates one of
/// everything, driven through the SAME ABI entry points a C++ constructor
/// reaches (`NROS_SUBSCRIBE`, `create_publisher`, `create_wall_timer`,
/// `create_guard_condition`, `declare_parameter<T>`): one subscription at
/// `QoS(1)`, one publisher at the default profile, one wall timer, one guard
/// condition and two parameters. The sidecar must carry exactly those facts.
///
/// This is the positive half of `check-census-hooks-complete`. The negative
/// control -- remove any one hook call and the census no longer matches -- is
/// `scripts/check-census-hooks-complete.py`, which mutates the sources in
/// memory rather than rebuilding this crate once per hook.
///
/// A `#[cfg(test)]` module in this file rather than an integration test: the
/// entry points are this crate's own `extern "C"` functions, and a test target
/// would need a `required-features` line in the manifest to compile only when
/// `metadata-mode` is on.
#[cfg(all(
    test,
    feature = "metadata-mode",
    feature = "param-services",
    feature = "rmw-cffi"
))]
mod census_fixture_tests {
    use core::{ffi::c_void, mem::MaybeUninit};

    use crate::{
        NROS_CPP_RET_OK,
        guard_condition::nros_cpp_guard_condition_create,
        nros_cpp_fini, nros_cpp_init_rmw, nros_cpp_node_create_ex, nros_cpp_node_options_t,
        nros_cpp_node_t, nros_cpp_qos_t,
        params_shim::{nros_cpp_node_declare_param_bool, nros_cpp_node_declare_param_double},
        publisher::nros_cpp_publisher_create,
        subscription::nros_cpp_subscription_create,
        timer::nros_cpp_timer_create,
    };

    /// The executor's caller-owned storage, aligned as the C++ side aligns it.
    #[repr(C, align(16))]
    struct ExecutorStorage([u64; crate::CPP_EXECUTOR_OPAQUE_U64S]);

    unsafe extern "C" fn noop(_context: *mut c_void) {}

    /// The app-level registration hook `nros_cpp_init*` calls. On a real link
    /// path it is the GENERATED strong C stub (phase-249 P2b) that invokes the
    /// selected backend's `nros_rmw_<x>_register`; a lib test has no generated
    /// stub, so this test is the app and supplies the same body for the one
    /// backend it selects (idempotent; the hosted `.init_array` ctor has
    /// usually registered it already).
    /// cbindgen:ignore
    // cbindgen reads every `#[no_mangle] extern "C"` in the crate, `cfg(test)`
    // or not, and would write a test-only symbol into `nros_cpp_ffi.h`.
    #[unsafe(no_mangle)]
    extern "C" fn nros_app_register_backends() {
        let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    }

    /// The C++ `::nros::QoS(depth)` spelling, as `to_qos_settings` reads it:
    /// keep-last, reliable, volatile, no liveliness -- the profile a
    /// `NROS_SUBSCRIBE(..., ::nros::QoS(1))` call site sends across.
    fn qos_depth(depth: i32) -> nros_cpp_qos_t {
        nros_cpp_qos_t {
            reliability: crate::nros_cpp_qos_reliability_t::NROS_CPP_QOS_RELIABLE,
            durability: crate::nros_cpp_qos_durability_t::NROS_CPP_QOS_VOLATILE,
            history: crate::nros_cpp_qos_history_t::NROS_CPP_QOS_KEEP_LAST,
            liveliness_kind: crate::nros_cpp_qos_liveliness_t::NROS_CPP_QOS_LIVELINESS_NONE,
            depth,
            deadline_ms: 0,
            lifespan_ms: 0,
            liveliness_lease_ms: 0,
            avoid_ros_namespace_conventions: 0,
            tx_express: 0,
        }
    }

    fn array_between<'a>(json: &'a str, key: &str) -> &'a str {
        let start = json
            .find(key)
            .unwrap_or_else(|| panic!("{key} missing in {json}"));
        let rows = &json[start..];
        &rows[..rows.find(']').expect("array end")]
    }

    #[test]
    fn fixture_component_census_carries_every_fact() {
        nros::metadata_mode::reset();

        let mut storage = MaybeUninit::<ExecutorStorage>::uninit();
        let exec = storage.as_mut_ptr().cast::<c_void>();
        let rc = unsafe {
            nros_cpp_init_rmw(
                c"metadata".as_ptr(),
                core::ptr::null(),
                0,
                c"census_fixture".as_ptr(),
                c"/".as_ptr(),
                exec,
            )
        };
        assert_eq!(rc, NROS_CPP_RET_OK, "the metadata backend must open");

        let opts = nros_cpp_node_options_t::default();
        let mut node = MaybeUninit::<nros_cpp_node_t>::uninit();
        let rc = unsafe {
            nros_cpp_node_create_ex(exec, c"census_fixture".as_ptr(), &opts, node.as_mut_ptr())
        };
        assert_eq!(rc, NROS_CPP_RET_OK);
        let node = node.as_mut_ptr().cast_const();

        // One subscription at QoS(1).
        let mut sub = MaybeUninit::<nros::internals::RmwSubscriber>::uninit();
        let rc = unsafe {
            nros_cpp_subscription_create(
                node,
                c"/control/command/control_cmd".as_ptr(),
                c"autoware_control_msgs::msg::dds_::Control_".as_ptr(),
                c"".as_ptr(),
                qos_depth(1),
                sub.as_mut_ptr().cast::<c_void>(),
            )
        };
        assert_eq!(rc, NROS_CPP_RET_OK);

        // One publisher at the default profile (depth 10).
        let mut publisher = MaybeUninit::<nros::internals::RmwPublisher>::uninit();
        let rc = unsafe {
            nros_cpp_publisher_create(
                node,
                c"/system/emergency/control_cmd".as_ptr(),
                c"autoware_control_msgs::msg::dds_::Control_".as_ptr(),
                c"".as_ptr(),
                qos_depth(10),
                publisher.as_mut_ptr().cast::<c_void>(),
            )
        };
        assert_eq!(rc, NROS_CPP_RET_OK);

        // One wall timer.
        let mut handle_id = 0usize;
        let rc = unsafe {
            nros_cpp_timer_create(exec, 33, Some(noop), core::ptr::null_mut(), &mut handle_id)
        };
        assert_eq!(rc, NROS_CPP_RET_OK);

        // One guard condition.
        let mut guard = MaybeUninit::<nros_node::GuardCondition>::uninit();
        let rc = unsafe {
            nros_cpp_guard_condition_create(
                exec,
                Some(noop),
                core::ptr::null_mut(),
                guard.as_mut_ptr().cast::<c_void>(),
            )
        };
        assert_eq!(rc, NROS_CPP_RET_OK);

        // Two parameters.
        let rc = unsafe { nros_cpp_node_declare_param_double(node, c"rate".as_ptr(), 30.0) };
        assert_eq!(rc, NROS_CPP_RET_OK);
        let rc =
            unsafe { nros_cpp_node_declare_param_bool(node, c"use_pull_over".as_ptr(), false) };
        assert_eq!(rc, NROS_CPP_RET_OK);

        let export =
            nros::node_metadata::SourceMetadataExport::new("fixture_pkg", "census_fixture")
                .executable("census_fixture")
                .language("cpp");
        let json = nros::metadata_mode::to_json(&export).expect("serialize");
        let rc = unsafe { nros_cpp_fini(exec) };
        assert_eq!(rc, NROS_CPP_RET_OK);

        assert!(json.contains("\"version\":2"), "schema v2: {json}");

        let subs = array_between(&json, "\"subscribers\":");
        assert_eq!(
            subs.matches("\"id\":").count(),
            1,
            "one subscription: {subs}"
        );
        assert!(subs.contains("/control/command/control_cmd"), "{subs}");
        assert!(
            subs.contains("\"depth\":1,"),
            "the QoS the code passed, not a default: {subs}"
        );

        let pubs = array_between(&json, "\"publishers\":");
        assert_eq!(pubs.matches("\"id\":").count(), 1, "one publisher: {pubs}");
        assert!(
            pubs.contains("\"depth\":10,"),
            "the default profile: {pubs}"
        );

        let timers = array_between(&json, "\"timers\":");
        assert!(
            timers.contains("\"kind\":\"wall\",\"period_ms\":33,"),
            "one wall timer at the code's period: {timers}"
        );
        assert!(
            timers.contains("\"kind\":\"guard_condition\""),
            "one guard condition, under its own kind: {timers}"
        );
        assert_eq!(
            timers.matches("\"kind\":").count(),
            2,
            "two slot rows: {timers}"
        );

        let params = array_between(&json, "\"parameters\":");
        assert!(
            params.contains("\"name\":\"rate\",\"type\":\"double\",\"default\":30.0,"),
            "the declared type and code default: {params}"
        );
        assert!(
            params.contains("\"name\":\"use_pull_over\",\"type\":\"bool\",\"default\":false,"),
            "{params}"
        );
        assert_eq!(
            params.matches("\"name\":").count(),
            2,
            "two parameters: {params}"
        );

        // Nothing else: a hook that records twice is as wrong as one that
        // records nothing.
        assert_eq!(nros::metadata_mode::entity_count(), 6);
        nros::metadata_mode::reset();
    }
}

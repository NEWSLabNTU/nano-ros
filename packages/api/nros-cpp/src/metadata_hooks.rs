//! phase-308 — the C++ ABI's half of the host metadata probe and census.
//!
//! The four executor-side HOOKS (node / timer / guard condition / parameter)
//! used to live here. They moved to `nros::census_hooks` (issue 1419, issue
//! 1556 item 1, RFC-0100 Amendment 1): a census instrument whose hooks sit on
//! one language's ABI cannot attribute a Rust node or a C node that opens its
//! own node through `nros-c`, and `nros` is the crate all three node APIs sit
//! on. The `nros_cpp_*` entry points call them there — unconditionally, with
//! bodies only under `metadata-mode`, exactly as before the move.
//!
//! What stays is what is C++-ABI-specific: the dump entry point the probe TU
//! and the hosted census funnel call, and the fixture census of a component
//! that creates one of everything through this crate's own `extern "C"` entry
//! points. `check-census-hooks-complete` holds every entry point to its hook.
//!
//! This module records nothing and serializes nothing itself. No JSON, no
//! schema struct, no slot arithmetic — those live once in
//! `nros::node_metadata` (phase-308's layer constraint).

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
    use alloc::format;
    use core::{ffi::c_void, mem::MaybeUninit};

    use crate::{
        NROS_CPP_RET_OK,
        guard_condition::nros_cpp_guard_condition_create,
        nros_cpp_fini, nros_cpp_init_rmw, nros_cpp_node_create_ex, nros_cpp_node_options_t,
        nros_cpp_node_t, nros_cpp_qos_t,
        params_shim::{nros_cpp_node_declare_param_bool, nros_cpp_node_declare_param_double},
        publisher::nros_cpp_publisher_create,
        subscription::{
            nros_cpp_subscription_create, nros_cpp_subscription_options_t,
            nros_cpp_subscription_register, nros_cpp_subscription_register_with_info,
        },
        timer::nros_cpp_timer_create,
    };

    /// The executor's caller-owned storage, aligned as the C++ side aligns it.
    #[repr(C, align(16))]
    struct ExecutorStorage([u64; crate::CPP_EXECUTOR_OPAQUE_U64S]);

    unsafe extern "C" fn noop(_context: *mut c_void) {}

    /// phase-457 W3 — a borrowed-bytes callback, which is the shape
    /// `process_raw_in_place` can serve.
    unsafe extern "C" fn raw_noop(_data: *const u8, _len: usize, _context: *mut c_void) {}

    /// And one that also wants the sample's wire ATTACHMENT, which an in-place
    /// dispatch does not carry — so this shape buffers however capable the
    /// backend is. The pair is the whole reason the registration fact cannot be
    /// an image-level answer.
    unsafe extern "C" fn raw_info_noop(
        _data: *const u8,
        _len: usize,
        _attachment: *const u8,
        _attachment_len: usize,
        _context: *mut c_void,
    ) {
    }

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

        // phase-457 W3 — the two REGISTRATION shapes, so the sidecar carries what
        // each call site answered about in-place dispatch.
        //
        // Through the arena-registering ABI (`nros_cpp_subscription_register*`),
        // not the poll-style create above, and that is the point: the fact is
        // reported from `Executor::open_subscription`, which the create path never
        // reaches. Same backend, same image, one capable shape and one not.
        let sub_opts = nros_cpp_subscription_options_t::default();
        let mut raw_handle = 0usize;
        let rc = unsafe {
            nros_cpp_subscription_register(
                node,
                c"/in_place_capable".as_ptr(),
                c"std_msgs::msg::dds_::String_".as_ptr(),
                c"".as_ptr(),
                qos_depth(1),
                raw_noop,
                core::ptr::null_mut(),
                &mut raw_handle,
                &sub_opts,
            )
        };
        assert_eq!(rc, NROS_CPP_RET_OK);
        let mut info_handle = 0usize;
        let rc = unsafe {
            nros_cpp_subscription_register_with_info(
                node,
                c"/buffers_anyway".as_ptr(),
                c"std_msgs::msg::dds_::String_".as_ptr(),
                c"".as_ptr(),
                qos_depth(1),
                raw_info_noop,
                core::ptr::null_mut(),
                &mut info_handle,
                &sub_opts,
            )
        };
        assert_eq!(rc, NROS_CPP_RET_OK);

        let export =
            nros::node_metadata::SourceMetadataExport::new("fixture_pkg", "census_fixture")
                .executable("census_fixture")
                .language("cpp");
        let json = nros::metadata_mode::to_json(&export).expect("serialize");
        let rc = unsafe { nros_cpp_fini(exec) };
        assert_eq!(rc, NROS_CPP_RET_OK);

        // READ, not restated. The schema is ADDITIVE, so what this asserts is the
        // CURRENT version, not the number whichever wave last touched the file
        // happened to write — phase-457 W3 moved it to 3 and a literal here is
        // what noticed.
        assert!(
            json.contains(&format!(
                "\"version\":{}",
                nros::node_metadata::SOURCE_METADATA_SCHEMA_VERSION
            )),
            "the current schema version: {json}"
        );

        let subs = array_between(&json, "\"subscribers\":");
        assert_eq!(
            subs.matches("\"id\":").count(),
            3,
            "one poll-style subscription plus the two REGISTERED shapes: {subs}"
        );
        assert!(subs.contains("/control/command/control_cmd"), "{subs}");
        assert!(
            subs.contains("\"depth\":1,"),
            "the QoS the code passed, not a default: {subs}"
        );
        // phase-457 W3 — each registration's own answer reached its OWN row.
        //
        // The end-to-end: the C++ ABI registers, the executor's one consulting
        // site reports what the CALL SITE answered, the recorder joins it onto the
        // row the backend created at `create_subscription`, and the emitter writes
        // it. Rows are split on the array's own row separator so a needle cannot
        // wander into a neighbour.
        let row = |topic: &str| {
            subs.split(",{\"id\":")
                .find(|r| r.contains(topic))
                .unwrap_or_else(|| panic!("no row for {topic} in {subs}"))
        };
        assert!(
            row("/in_place_capable").contains("\"in_place\":true"),
            "a borrowed-bytes registration reports that it CAN dispatch in \
             place: {}",
            row("/in_place_capable")
        );
        assert!(
            row("/buffers_anyway").contains("\"in_place\":false"),
            "and one that wants the wire attachment reports that it cannot -- \
             SAME backend, same image, different row, which is why issue 1340's \
             saving cannot be taken from an image-level fact: {}",
            row("/buffers_anyway")
        );
        assert!(
            !row("/control/command/control_cmd").contains("\"in_place\""),
            "a subscription nothing REGISTERED carries no registration fact at \
             all -- the third state, which every consumer reads as a refusal and \
             never as a `false`: {}",
            row("/control/command/control_cmd")
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
        //
        // phase-457 W3 — 6 -> 8: the two REGISTERED subscriptions are entities in
        // their own right (each creates a backend subscriber, which is what the
        // recording RMW records), and the registration fact rides those rows
        // rather than adding any. A count that moved by more than two would mean
        // the observation had started recording as well as annotating.
        assert_eq!(nros::metadata_mode::entity_count(), 8);
        nros::metadata_mode::reset();
    }
}

/// Issue 1419 / issue 1556 item 1 -- the census hooks moved to `nros`, so they
/// reach the two node APIs that are NOT this crate's ABI.
///
/// Before the move, a C node that opens its own node through `nros-c`
/// (`nros_executor_node_init`, `rclc_node_init_default`) reached the recording
/// backend with no node attribution and its timers, guard conditions and
/// parameters not at all; a Rust node's `register()` (the `nros::main!` install
/// path, `ExecutorSink`) reached none of the hooks. Each test drives the REAL
/// entry points of its API against the recording backend and asserts the facts
/// land on the node that declared them.
///
/// Lives here rather than in `nros-c` / `nros` because this is the one lane
/// that builds `metadata-mode` with the recording backend linked
/// (`just check census-hooks-complete`), and both APIs are reachable from it:
/// `nros-cpp` bundles `nros-c`, and `nros` is a direct dependency.
#[cfg(all(
    test,
    feature = "metadata-mode",
    feature = "param-services",
    feature = "rmw-cffi"
))]
mod census_hooks_reach_every_api {
    use core::ffi::c_void;

    /// The JSON of ONE node: from its `source_default_name` to the next node's
    /// (or the end). Entities are nested inside their node, so a fact found
    /// here is a fact attributed to that node.
    fn node_section<'a>(json: &'a str, name: &str) -> &'a str {
        let needle = alloc::format!("\"source_default_name\":\"{name}\"");
        let start = json
            .find(&needle)
            .unwrap_or_else(|| panic!("no node `{name}` in {json}"));
        let rest = &json[start + needle.len()..];
        match rest.find("\"source_default_name\":") {
            Some(end) => &json[start..start + needle.len() + end],
            None => &json[start..],
        }
    }

    unsafe extern "C" fn timer_noop(_t: *mut nros_c::nros_timer_t, _c: *mut c_void) {}
    unsafe extern "C" fn guard_noop(_c: *mut c_void) {}

    /// A C node opened through `nros-c`'s own node entry point: its timer,
    /// guard condition and parameter are census rows ON THAT NODE.
    ///
    /// Red before the move (measured): the node is absent from `nodes[]`, so
    /// `node_section` panics -- the recorder never heard of it, and every
    /// executor-side entity after it was dropped for want of a current node.
    #[test]
    fn a_c_node_opened_through_nros_c_is_attributed() {
        use nros_c::*;
        nros::metadata_mode::reset();
        let _ = nros_rmw_metadata::nros_rmw_metadata_register();

        let mut support = alloc::boxed::Box::new(nros_support_get_zero_initialized());
        let rc = unsafe {
            nros_support_init_rmw(
                &mut *support,
                core::ptr::null(),
                0,
                c"census_c".as_ptr(),
                c"metadata".as_ptr(),
            )
        };
        assert_eq!(rc, NROS_RET_OK, "the metadata backend must open");

        let mut executor = alloc::boxed::Box::new(rclc_executor_get_zero_initialized_executor());
        let rc = unsafe { nros_executor_init(&mut *executor, &*support, 8) };
        assert_eq!(rc, NROS_RET_OK);

        let mut node = alloc::boxed::Box::new(rcl_get_zero_initialized_node());
        let rc = unsafe {
            nros_executor_node_init(
                &mut *executor,
                &mut *node,
                c"c_census_node".as_ptr(),
                core::ptr::null(),
            )
        };
        assert_eq!(rc, NROS_RET_OK);

        let mut timer = alloc::boxed::Box::new(rcl_get_zero_initialized_timer());
        let rc = unsafe {
            nros_timer_init(
                &mut *timer,
                &*support,
                50_000_000,
                Some(timer_noop),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(rc, NROS_RET_OK);
        let rc = unsafe { rclc_executor_add_timer(&mut *executor, &mut *timer) };
        assert_eq!(rc, NROS_RET_OK);

        let mut guard = alloc::boxed::Box::new(rcl_get_zero_initialized_guard_condition());
        let rc = unsafe {
            nros_node_create_guard_condition(
                &mut *node,
                &mut *guard,
                Some(guard_noop),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(rc, NROS_RET_OK);

        let rc =
            unsafe { nros_executor_declare_param_double(&mut *executor, c"gain".as_ptr(), 1.5) };
        assert_eq!(rc, NROS_RET_OK);

        let export =
            nros::node_metadata::SourceMetadataExport::new("c_pkg", "c_census_node").language("c");
        let json = nros::metadata_mode::to_json(&export).expect("serialize");
        let section = node_section(&json, "c_census_node");
        assert!(
            section.contains("\"kind\":\"wall\",\"period_ms\":50,"),
            "the C wall timer, at the code's period, on the C node: {section}"
        );
        assert!(
            section.contains("\"kind\":\"guard_condition\""),
            "the C guard condition, under its own kind: {section}"
        );
        assert!(
            section.contains("\"name\":\"gain\",\"type\":\"double\",\"default\":1.5,"),
            "the C parameter, as declared: {section}"
        );
        nros::metadata_mode::reset();
    }

    /// The legacy rclc-style node (`rclc_node_init_default` ->
    /// `nros_node_init_ex`), which is what every `nros_app_main` application
    /// (the twelve NuttX C leaves of issue 1556) opens.
    #[test]
    fn an_rclc_node_opens_the_census_cursor() {
        use nros_c::*;
        nros::metadata_mode::reset();
        let _ = nros_rmw_metadata::nros_rmw_metadata_register();

        let mut support = alloc::boxed::Box::new(nros_support_get_zero_initialized());
        let rc = unsafe {
            nros_support_init_rmw(
                &mut *support,
                core::ptr::null(),
                7,
                c"census_rclc".as_ptr(),
                c"metadata".as_ptr(),
            )
        };
        assert_eq!(rc, NROS_RET_OK);
        let mut node = alloc::boxed::Box::new(rcl_get_zero_initialized_node());
        let rc = unsafe {
            rclc_node_init_default(
                &mut *node,
                c"rclc_node".as_ptr(),
                c"/robot".as_ptr(),
                &*support,
            )
        };
        assert_eq!(rc, NROS_RET_OK);

        let export = nros::node_metadata::SourceMetadataExport::new("c_pkg", "rclc_node");
        let json = nros::metadata_mode::to_json(&export).expect("serialize");
        let section = node_section(&json, "rclc_node");
        assert!(
            section.contains("\"namespace\":\"/robot\""),
            "the namespace the code asked for: {section}"
        );
        nros::metadata_mode::reset();
    }

    /// A C timer registered before ANY node -- legal (`nros_timer_init` needs
    /// only a support context; `nros-c`'s `timer_clock_source.c` does it). The
    /// slot is counted under the executor scope instead of panicking the
    /// process, which is what a bare `record` with no current node would do in
    /// every native image that links the recorder.
    #[test]
    fn a_node_less_c_timer_is_counted_under_the_executor_scope() {
        use nros_c::*;
        nros::metadata_mode::reset();
        let _ = nros_rmw_metadata::nros_rmw_metadata_register();

        let mut support = alloc::boxed::Box::new(nros_support_get_zero_initialized());
        let rc = unsafe {
            nros_support_init_rmw(
                &mut *support,
                core::ptr::null(),
                0,
                c"census_scope".as_ptr(),
                c"metadata".as_ptr(),
            )
        };
        assert_eq!(rc, NROS_RET_OK);
        let mut executor = alloc::boxed::Box::new(rclc_executor_get_zero_initialized_executor());
        assert_eq!(
            unsafe { nros_executor_init(&mut *executor, &*support, 4) },
            NROS_RET_OK
        );
        let mut timer = alloc::boxed::Box::new(rcl_get_zero_initialized_timer());
        let rc = unsafe {
            nros_timer_init(
                &mut *timer,
                &*support,
                10_000_000,
                Some(timer_noop),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(rc, NROS_RET_OK);
        assert_eq!(
            unsafe { rclc_executor_add_timer(&mut *executor, &mut *timer) },
            NROS_RET_OK
        );

        let json = nros::metadata_mode::to_json(&nros::node_metadata::SourceMetadataExport::new(
            "c_pkg", "scope",
        ))
        .expect("serialize");
        let section = node_section(&json, nros::census_hooks::EXECUTOR_SCOPE);
        assert!(
            section.contains("\"period_ms\":10,"),
            "the node-less timer, counted: {section}"
        );
        nros::metadata_mode::reset();
    }

    /// A Rust component, through the SAME install path a `nros::main!` entry
    /// takes (`ExecutorNodeRuntime::register_node` -> `ExecutorSink`): its node,
    /// its timer and its parameter are census rows on its node.
    ///
    /// Red before the move (measured): `nodes[]` has no `rust_census_node` --
    /// `ExecutorSink` called no hook, so the recorder had no node and the timer
    /// and parameter were never recorded.
    #[test]
    fn a_rust_component_registered_through_the_runtime_is_attributed() {
        struct CensusComp;
        impl nros::Node for CensusComp {
            const NAME: &'static str = "rust_census_node";
            fn register(ctx: &mut nros::NodeContext<'_>) -> nros::NodeResult<()> {
                let mut node = ctx.create_node(nros::NodeOptions::new("rust_census_node"))?;
                let _t = node.create_timer_for_callback_name(
                    "on_tick",
                    nros::TimerDuration::from_millis(25),
                )?;
                let _p = node.declare_parameter_for_name_with_default(
                    "limit",
                    nros::node_metadata::ParameterDefault::Integer(7),
                )?;
                Ok(())
            }
        }
        impl nros::ExecutableNode for CensusComp {
            type State = ();
            fn init() -> Self::State {}
            fn on_callback(
                _state: &mut Self::State,
                _callback: nros::Callback<'_>,
                _ctx: &mut nros::CallbackCtx<'_>,
            ) {
            }
        }

        nros::metadata_mode::reset();
        let _ = nros_rmw_metadata::nros_rmw_metadata_register();
        let config = nros::ExecutorConfig::new("").rmw("metadata");
        let executor = nros::Executor::open(&config).expect("the metadata backend must open");
        let mut runtime = nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
        runtime
            .register_node::<CensusComp>()
            .expect("register through the runtime");

        let export = nros::node_metadata::SourceMetadataExport::new("rust_pkg", "rust_census_node")
            .language("rust");
        let json = nros::metadata_mode::to_json(&export).expect("serialize");
        let section = node_section(&json, "rust_census_node");
        assert!(
            section.contains("\"kind\":\"wall\",\"period_ms\":25,"),
            "the Rust timer, at the code's period, on the Rust node: {section}"
        );
        assert!(
            section.contains("\"name\":\"limit\",\"type\":\"integer\",\"default\":7,"),
            "the Rust parameter, as declared: {section}"
        );
        nros::metadata_mode::reset();
    }
}

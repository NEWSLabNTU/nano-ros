//! A Rust component installed through its C-ABI install symbol reads the
//! launch `<param>` values the foreign entry hands it.
//!
//! Issue 2568: a C++ entry installs a Rust component by calling
//! `__nros_component_<pkg>_install`, the symbol `nros::node!` emits. That seam
//! used to carry no params, so `ctx.param(name)` in the component's `register`
//! answered `None` for every launch `<param>` while a `nros::main!` image saw
//! them. The seam now takes a flat `[name0, value0, ...]` C-string array, and
//! this test calls the macro-emitted symbol exactly as the generated C++ entry
//! does, on an executor opened over the transport-free `metadata` backend.
//!
//! `nros::node!` emits one install symbol per crate, so this file carries the
//! one component.

use std::sync::Mutex;

use nros::{
    Callback, CallbackCtx, Component, ExecutableNode, NodeContext, NodeOptions, NodeResult, TickCtx,
};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

/// What `register` read, per parameter it asked for.
static SEEN: Mutex<Vec<(&'static str, Option<String>)>> = Mutex::new(Vec::new());

struct ParamReader;

impl Component for ParamReader {
    const NAME: &'static str = "param_reader";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut seen = SEEN.lock().unwrap();
        for name in ["speed", "frame_id", "absent"] {
            seen.push((name, ctx.param(name).map(str::to_owned)));
        }
        drop(seen);
        let _node = ctx.create_node(NodeOptions::new("param_reader"))?;
        Ok(())
    }
}

impl ExecutableNode for ParamReader {
    type State = ();
    fn init() -> Self::State {}
    fn on_callback(_state: &mut (), _cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}
    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
    const HAS_TICK: bool = false;
}

nros::node!(ParamReader);

#[test]
fn c_abi_install_delivers_launch_params_to_ctx_param() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let mut executor = nros::Executor::open(&config).expect("the metadata backend must open");
    let handle = core::ptr::from_mut(&mut executor).cast::<core::ffi::c_void>();

    // The shape the generated C++ entry writes for a Rust node with params.
    let kv: [*const core::ffi::c_char; 4] = [
        c"speed".as_ptr(),
        c"2.5".as_ptr(),
        c"frame_id".as_ptr(),
        c"base_link".as_ptr(),
    ];
    let rc = __nros_component_nros_rmw_metadata_install(
        core::ptr::null(),
        handle,
        core::ptr::null_mut(),
        kv.as_ptr(),
        2,
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
        0,
    );
    assert_eq!(rc, 0, "the install must succeed");

    let seen = SEEN.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            ("speed", Some("2.5".to_owned())),
            ("frame_id", Some("base_link".to_owned())),
            ("absent", None),
        ],
        "`ctx.param` must read the launch values the install carried"
    );

    // A malformed array is refused before the component registers.
    let bad: [*const core::ffi::c_char; 2] = [c"speed".as_ptr(), core::ptr::null()];
    let rc = __nros_component_nros_rmw_metadata_install(
        core::ptr::null(),
        handle,
        core::ptr::null_mut(),
        bad.as_ptr(),
        1,
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
        0,
    );
    assert_eq!(rc, -5, "a null value must be refused");
    assert_eq!(
        SEEN.lock().unwrap().len(),
        3,
        "a refused install must not register"
    );
}

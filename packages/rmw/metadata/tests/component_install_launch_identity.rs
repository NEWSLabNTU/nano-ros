//! A Rust component installed through its C-ABI install symbol takes the
//! launch `<node name= namespace=>` and `<remap>` rules the foreign entry
//! hands it.
//!
//! Issue 2590: a C++ entry installs a Rust component by calling
//! `__nros_component_<pkg>_install`, the symbol `nros::node!` emits. That seam
//! carried the launch params (issue 2568) but no identity and no remaps, so
//! the node took its `NodeOptions` name and every launch remap was dropped,
//! while a `nros::main!` image applied both. This test calls the macro-emitted
//! symbol exactly as the generated C++ entry does, on an executor opened over
//! the transport-free `metadata` backend, and reads back what the image
//! declared.
//!
//! `nros::node!` emits one install symbol per crate, so this file carries the
//! one component.

use nros::{
    Callback, CallbackCtx, CdrReader, CdrWriter, Component, DeclarativeNode, DeserError,
    ExecutableNode, NodeContext, NodeOptions, NodeResult, RosMessage, SerError, TickCtx,
};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

#[derive(Debug, Clone, Copy, Default)]
struct Ping;

impl nros::Serialize for Ping {
    fn serialize(&self, _w: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl nros::Deserialize for Ping {
    fn deserialize(_r: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self)
    }
}

impl RosMessage for Ping {
    const TYPE_NAME: &'static str = "test_msgs::msg::dds_::Ping_";
    const TYPE_HASH: &'static str = "ping_hash";
}

// `MessageForRmw` also requires a field schema; an empty message has none.
impl nros_serdes::schema::Message for Ping {
    const TYPE_NAME: &'static str = "test_msgs/msg/Ping";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[];
}

struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        // The code's own name and namespace: what the launch overrides.
        let mut node = ctx.create_node(NodeOptions::new("code_name").namespace("/code_ns"))?;
        // A relative name, so the namespace the node lands in shows in it too.
        let _ = node.create_publisher_for_topic::<Ping>("chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = ();
    fn init() -> Self::State {}
    fn on_callback(_state: &mut (), _cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}
    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
    const HAS_TICK: bool = false;
}

nros::node!(Talker);

fn sidecar() -> String {
    nros::metadata_mode::to_json(&nros::node_metadata::SourceMetadataExport {
        package: "nros_rmw_metadata",
        component: "talker",
        executable: None,
        exported_symbol: None,
        package_manifest: "",
        source_artifacts: &[],
        language: "rust",
    })
    .expect("the sidecar serialises")
}

#[test]
fn c_abi_install_names_and_remaps_the_node_as_the_launch_says() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    nros::metadata_mode::reset();
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let mut executor = nros::Executor::open(&config).expect("the metadata backend must open");
    let handle = core::ptr::from_mut(&mut executor).cast::<core::ffi::c_void>();

    // The shape the generated C++ entry writes for a Rust node the launch
    // names `driver` in `/robot`, with one remap.
    let remaps: [*const core::ffi::c_char; 2] = [c"chatter".as_ptr(), c"/robot/out".as_ptr()];
    let rc = __nros_component_nros_rmw_metadata_install(
        core::ptr::null(),
        handle,
        core::ptr::null_mut(),
        core::ptr::null(),
        0,
        c"driver".as_ptr(),
        c"/robot".as_ptr(),
        remaps.as_ptr(),
        1,
    );
    assert_eq!(rc, 0, "the install must succeed");

    let node = executor
        .nodes()
        .iter()
        .find(|n| n.name.as_str() == "driver")
        .unwrap_or_else(|| {
            let names: Vec<_> = executor.nodes().iter().map(|n| n.name.as_str()).collect();
            panic!("no node named by the launch; the image's nodes: {names:?}")
        });
    assert_eq!(
        node.namespace.as_str(),
        "/robot",
        "the launch namespace, not the code's"
    );
    assert!(
        !executor
            .nodes()
            .iter()
            .any(|n| n.name.as_str() == "code_name"),
        "the code's NodeOptions name must not stand when the launch names the node"
    );

    let json = sidecar();
    assert!(
        json.contains("\"/robot/out\""),
        "the launch remap must reach the publisher's name; sidecar:\n{json}"
    );
    assert!(
        !json.contains("\"/robot/chatter\"") && !json.contains("\"/code_ns/chatter\""),
        "the unremapped name must not be declared; sidecar:\n{json}"
    );

    // A malformed remap array is refused before the component registers.
    let bad: [*const core::ffi::c_char; 2] = [c"chatter".as_ptr(), core::ptr::null()];
    let rc = __nros_component_nros_rmw_metadata_install(
        core::ptr::null(),
        handle,
        core::ptr::null_mut(),
        core::ptr::null(),
        0,
        core::ptr::null(),
        core::ptr::null(),
        bad.as_ptr(),
        1,
    );
    assert_eq!(rc, -5, "a null remap target must be refused");
}

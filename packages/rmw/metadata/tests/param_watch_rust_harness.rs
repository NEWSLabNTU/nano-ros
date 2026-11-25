//! Issue 2964: the Rust probe harness states whether its `parameters` list is
//! the whole declaration set (`parameter_declarations_watched`), as the C/C++
//! road does through `metadata_mode`.
//!
//! A no-contract Rust image sizes its parameter store from that statement. With
//! nothing stated it keeps the default store, carved from the executor backing,
//! and the Pico listener overflowed RAM by 16,344 B where the C one fit. The
//! harness entry (`record_node_metadata_seeded_watching_prose`, which
//! `render_harness_main` calls) now arms the watch, and breaks it on what its
//! own rows cannot hold: a node that opts into undeclared parameters, a
//! declaration through an imperative route, or a refused row.
//!
//! This crate's dev-dependency compiles `metadata-mode` too, so these tests
//! also cover the branch that reads that module's watch and count. Driven on an
//! executor over the transport-free `metadata` backend, the one the harness
//! opens.

use nros::{
    Component, DeclarativeNode, NodeContext, NodeOptions, NodeResult, ParameterType,
    node_metadata::{MetadataRecorder, SourceMetadataExport},
};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

/// The recorder behind a metadata executor is process-global; one test at a
/// time records.
static CENSUS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Declares nothing: the listener's shape.
struct Quiet;
impl Component for Quiet {
    const NAME: &'static str = "quiet";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("quiet"))?;
        Ok(())
    }
}

/// Declares through the declarative route, whose rows the harness records.
struct Declarative;
impl Component for Declarative {
    const NAME: &'static str = "declarative";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("declarative"))?;
        let _rate = node.declare_parameter_for_name("rate", ParameterType::Integer)?;
        Ok(())
    }
}

/// Opts into parameters it never declares: no list can be whole.
struct Undeclared;
impl Component for Undeclared {
    const NAME: &'static str = "undeclared";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("undeclared"))?;
        node.allow_undeclared_parameters(true);
        Ok(())
    }
}

/// Declares the typed rclrs way, an imperative route below the harness's rows.
struct Imperative;
impl Component for Imperative {
    const NAME: &'static str = "imperative";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("imperative"))?;
        let _rate = node
            .declare_parameter::<i64>("rate")
            .expect("the store opens")
            .default(5)
            .mandatory()
            .expect("rate declares");
        Ok(())
    }
}

/// A registration that fails part way, as a refused row does: the recorder
/// holds a declaration, but no proof the list is whole.
struct Refused;
impl Component for Refused {
    const NAME: &'static str = "refused";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("refused"))?;
        let _rate = node.declare_parameter_for_name("rate", ParameterType::Integer)?;
        Err(nros::NodeDeclError::Runtime)
    }
}

fn probe<C: Component>() -> MetadataRecorder {
    let _census = CENSUS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    nros::metadata_mode::reset();
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let mut executor = nros::Executor::open(&config).expect("the metadata backend must open");
    let mut recorder = MetadataRecorder::default();
    let _ = nros::__macro::record_node_metadata_seeded_watching_prose::<C>(
        &mut recorder,
        &mut executor,
        &[],
    );
    recorder
}

#[test]
fn a_node_that_declares_nothing_states_its_list_is_whole() {
    let recorder = probe::<Quiet>();
    assert_eq!(recorder.parameter_declarations_watched(), Some(true));
    let json = recorder
        .to_source_metadata_json(&SourceMetadataExport::new("pkg", "quiet"))
        .expect("serialize");
    assert!(
        json.contains("\"parameter_declarations_watched\":true"),
        "the sidecar states it: {json}"
    );
}

#[test]
fn a_declarative_declaration_keeps_the_list_whole() {
    let recorder = probe::<Declarative>();
    assert_eq!(recorder.parameter_count(), 1);
    assert_eq!(recorder.parameter_declarations_watched(), Some(true));
}

/// The negative controls: each is what the store could not be sized from.
#[test]
fn undeclared_parameters_break_the_watch() {
    assert_eq!(
        probe::<Undeclared>().parameter_declarations_watched(),
        Some(false)
    );
}

#[test]
fn an_imperative_declaration_breaks_the_watch() {
    assert_eq!(
        probe::<Imperative>().parameter_declarations_watched(),
        Some(false)
    );
}

#[test]
fn a_failed_registration_breaks_the_watch() {
    assert_eq!(
        probe::<Refused>().parameter_declarations_watched(),
        Some(false)
    );
}

//! phase-483 W3 — the declarative component surface, recorded.
//!
//! These lived in `nros/src/node.rs` as unit tests that ran `register()`
//! against a bare `MetadataRecorder`. A component now registers into a REAL
//! executor (its `register` gets the same `nros::Node` a program does), so
//! the tests need one, and this crate has the backend that opens one without
//! a transport: `metadata`. What they assert is unchanged — what each
//! declarative constructor records.

use nros::{
    CallbackId, CdrReader, CdrWriter, Component, DeclarativeNode, DeserError, EntityId,
    NodeContext, NodeDeclError, NodeId, NodeOptions, NodeResult, NodeRuntime, ParameterDefault,
    ParameterType, RosAction, RosMessage, RosService, SerError, SourceNameKind, TimerClockSource,
    TimerDuration,
    node_metadata::{
        CallbackEffectKind, EntityKind, EntityMetadata, MetadataRecorder, MetadataString,
        NodeMetadataError,
    },
};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls;
// naming the crate is what links it.
extern crate nros_platform_cffi as _;

fn metadata_executor() -> nros::Executor<'static> {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    nros::Executor::open(&config).expect("the metadata backend must open")
}

fn metadata_str(value: &str) -> MetadataString {
    let mut s = MetadataString::new();
    s.push_str(value).expect("fits the metadata string");
    s
}

fn record<C: Component>(recorder: &mut dyn NodeRuntime) -> NodeResult<()> {
    let mut executor = metadata_executor();
    nros::record_node_metadata::<C>(recorder, &mut executor)
}

#[derive(Debug, Clone, Copy, Default)]
struct TestMsg;

impl nros::Serialize for TestMsg {
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl nros::Deserialize for TestMsg {
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self)
    }
}

impl RosMessage for TestMsg {
    const TYPE_NAME: &'static str = "test_msgs::msg::dds_::Test_";
    const TYPE_HASH: &'static str = "test_hash";
}

// Phase 380 W4 — `MessageForRmw` now also requires a field schema, because
// that is where a subscription's size bound comes from. `TestMsg` is an
// empty struct, so its schema is the empty slice and its bound is just the
// encapsulation header — which is exactly what the build assertion should
// see for it.
impl nros_serdes::schema::Message for TestMsg {
    const TYPE_NAME: &'static str = "test_msgs/msg/Test";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[];
}

struct TestService;

impl RosService for TestService {
    type Request = TestMsg;
    type Reply = TestMsg;

    const SERVICE_NAME: &'static str = "test_msgs::srv::dds_::Test_";
    const SERVICE_HASH: &'static str = "test_service_hash";
}

struct TestAction;

impl RosAction for TestAction {
    type Goal = TestMsg;
    type Result = TestMsg;
    type Feedback = TestMsg;
    type SendGoalRequest = TestMsg;
    type SendGoalResponse = TestMsg;
    type GetResultRequest = TestMsg;
    type GetResultResponse = TestMsg;
    type FeedbackMessage = TestMsg;

    const ACTION_NAME: &'static str = "test_msgs::action::dds_::Test_";
    const ACTION_HASH: &'static str = "test_action_hash";
}

struct TalkerComponent;

impl Component for TalkerComponent {
    const NAME: &'static str = "talker_component";

    fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node =
            context.create_node_with_id(NodeId::new("node"), NodeOptions::new("talker"))?;
        let _publisher =
            node.declare_publisher::<TestMsg>(EntityId::new("pub_chatter"), "chatter")?;
        let _subscription = node.declare_subscription::<TestMsg>(
            EntityId::new("sub_cmd"),
            CallbackId::new("on_cmd"),
            "~/cmd",
        )?;
        let _timer = node.declare_timer(
            EntityId::new("timer_tick"),
            CallbackId::new("on_tick"),
            TimerDuration::from_millis(10),
        )?;
        let _parameter =
            node.declare_parameter(EntityId::new("param_gain"), "gain", ParameterType::Double)?;
        node.callback(CallbackId::new("on_tick"))
            .publishes(EntityId::new("pub_chatter"))?
            .writes(EntityId::new("param_gain"))?;
        Ok(())
    }
}

#[test]
fn component_records_metadata_without_transport() {
    let mut recorder = MetadataRecorder::<2, 8, 4>::new();
    record::<TalkerComponent>(&mut recorder).unwrap();

    assert_eq!(recorder.nodes().len(), 1);
    assert_eq!(recorder.nodes()[0].name.as_str(), "talker");
    assert_eq!(recorder.entities().len(), 4);
    assert_eq!(recorder.entities()[0].kind, EntityKind::Publisher);
    assert_eq!(recorder.entities()[1].source_name.as_str(), "~/cmd");
    assert_eq!(
        recorder.entities()[1]
            .callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("on_cmd")
    );
    assert_eq!(recorder.callback_effects().len(), 2);
}

// Phase 250 Wave 2b — the declarative `.safety()` opt-in records the
// `EntityMetadata.safety` flag so the runtime registers the integrity-aware
// subscription. A plain subscription stays `safety == false`.
struct SafetyComponent;
impl Component for SafetyComponent {
    const NAME: &'static str = "safety_component";
    fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node =
            context.create_node_with_id(NodeId::new("node"), NodeOptions::new("listener"))?;
        let _plain = node.create_subscription_for_callback_name::<TestMsg>("on_plain", "/a")?;
        let _safe =
            node.create_subscription_for_callback_name_with_safety::<TestMsg>("on_safe", "/b")?;
        Ok(())
    }
}

#[test]
fn safety_opt_in_records_metadata_flag() {
    let mut recorder = MetadataRecorder::<2, 8, 4>::new();
    record::<SafetyComponent>(&mut recorder).unwrap();
    let ents = recorder.entities();
    assert_eq!(ents.len(), 2);
    // Plain subscription on /a — no safety.
    assert_eq!(ents[0].source_name.as_str(), "/a");
    assert!(!ents[0].safety, "plain sub must not be flagged");
    // `.safety()` subscription on /b — flagged.
    assert_eq!(ents[1].source_name.as_str(), "/b");
    assert!(ents[1].safety, "safety sub must be flagged");
}

/// phase-457 W5 (issue 1522) — the Rust producer STATES a subscription's
/// registration path instead of refusing it.
///
/// Nothing observes this road: `record_node_metadata` opens no executor,
/// so `Executor::open_subscription` never runs and
/// `registration_observer` never fires. Before this wave the row stayed
/// `None`, `registration_path` refused, and every consumer fell back to
/// budgeting the whole receive region.
///
/// Asserted against the CLASSIFIER rather than a literal `false` on
/// purpose: the fact under test is that the recorder reports what the
/// registrar will do, not what it answers today. Issue 1340 flips the
/// `BufferedRaw` arm to `true`, and this test must follow it without an
/// edit — an assertion on `Some(false)` would have to be found and
/// changed, which is the drift the shared classifier exists to remove.
/// The literal is asserted once, in
/// `nros_node::executor::declared_shape`'s own tests, where flipping it
/// is the whole event.
#[test]
fn a_declared_subscription_states_the_path_its_registration_will_take() {
    use nros_node::executor::declared_shape::DeclaredSubscriptionShape;

    let mut recorder = MetadataRecorder::<2, 8, 4>::new();
    record::<SafetyComponent>(&mut recorder).unwrap();
    let ents = recorder.entities();

    assert_eq!(
        ents[0].in_place_capable,
        Some(DeclaredSubscriptionShape::BufferedRaw.in_place_capable()),
        "a plain declared subscription lowers to \
         `register_subscription_buffered_raw_on`, so the probe states that \
         entry point's own answer"
    );
    assert_eq!(
        ents[1].in_place_capable,
        Some(ents[1].declared_subscription_shape().in_place_capable()),
        "a `.safety()` declaration must state whatever the shape it \
         classifies to answers — which is the masked shape on a build \
         without the capability"
    );
    // The row is the SUBSCRIPTION's. A kind that reaches no subscription
    // entry point must keep refusing, because nothing decided anything
    // for it (issue 1522's other three populations).
    let mut publishers = MetadataRecorder::<2, 8, 4>::new();
    record::<TalkerComponent>(&mut publishers).unwrap();
    let pubs = publishers.entities();
    assert_eq!(pubs[0].kind, EntityKind::Publisher);
    assert_eq!(
        pubs[0].in_place_capable, None,
        "only a subscription has a registration path to state"
    );
}

struct GroupedComponent;

impl Component for GroupedComponent {
    const NAME: &'static str = "grouped_component";

    fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node =
            context.create_node_with_id(NodeId::new("node"), NodeOptions::new("grouped"))?;
        // Unlabeled entity declared before any group is set.
        let _pub = node.declare_publisher::<TestMsg>(EntityId::new("pub_plain"), "plain")?;
        // Sticky "control" group covers the next two entities.
        node.callback_group("control")?;
        let _sub = node.declare_subscription::<TestMsg>(
            EntityId::new("sub_cmd"),
            CallbackId::new("on_cmd"),
            "~/cmd",
        )?;
        let _timer = node.declare_timer(
            EntityId::new("timer_tick"),
            CallbackId::new("on_tick"),
            TimerDuration::from_millis(10),
        )?;
        // Switch to "telemetry" for the last entity.
        node.callback_group("telemetry")?;
        let _sub2 = node.declare_subscription::<TestMsg>(
            EntityId::new("sub_diag"),
            CallbackId::new("on_diag"),
            "~/diag",
        )?;
        Ok(())
    }
}

#[test]
fn sticky_callback_group_stamps_subsequent_entities() {
    let mut recorder = MetadataRecorder::<2, 8, 4>::new();
    record::<GroupedComponent>(&mut recorder).unwrap();

    let group_of = |idx: usize| {
        recorder.entities()[idx]
            .callback_group
            .as_ref()
            .map(|g| g.as_str())
    };
    // pub_plain — declared before any group → unlabeled.
    assert_eq!(group_of(0), None);
    // sub_cmd + timer_tick — under "control".
    assert_eq!(group_of(1), Some("control"));
    assert_eq!(group_of(2), Some("control"));
    // sub_diag — under "telemetry".
    assert_eq!(group_of(3), Some("telemetry"));
}

#[test]
fn context_can_synthesize_stable_node_id_from_options_name() {
    let mut recorder = MetadataRecorder::<1, 0, 0>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let node = context
            .create_node(NodeOptions::new("talker").namespace("/demo").domain_id(42))
            .unwrap();

        assert_eq!(node.name(), "talker");
    });

    assert_eq!(recorder.nodes().len(), 1);
    assert_eq!(recorder.nodes()[0].id.as_str(), "talker");
    assert_eq!(recorder.nodes()[0].name.as_str(), "talker");
    assert_eq!(recorder.nodes()[0].namespace.as_str(), "/demo");
    assert_eq!(recorder.nodes()[0].domain_id, 42);
}

/// phase-430 W4 — the declarative surface carries the clock through to the
/// runtime, and the clock-less spelling still means WALL.
///
/// The runtime half is one line (`node_runtime.rs` hands
/// `metadata.timer_clock` to `Executor::register_timer_on_clock`), so this
/// asserts the thing that line reads. What the clock then DOES is
/// `nros-node`'s `a_node_level_ros_time_timer_follows_the_simulated_clock`,
/// which needs an executor and a `/clock` override.
#[test]
fn a_declared_timer_records_the_clock_it_asked_for() {
    let mut recorder = MetadataRecorder::<1, 2, 0>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("talker")).unwrap();

        let _wall = node
            .create_timer_for_callback_name("on_tick", TimerDuration::from_millis(10))
            .unwrap();
        let _sim = node
            .create_timer_for_callback_name_on_clock(
                "on_sim_tick",
                TimerDuration::from_millis(10),
                TimerClockSource::Ros,
            )
            .unwrap();
    });

    assert_eq!(recorder.entities().len(), 2);
    assert_eq!(
        recorder.entities()[0].timer_clock,
        TimerClockSource::Steady,
        "a timer declared with no clock is the WALL case, unchanged by W4 -- \
         every timer in the tree predates the axis and must keep its behaviour"
    );
    assert_eq!(
        recorder.entities()[1].timer_clock,
        TimerClockSource::Ros,
        "the clock a component asks for must survive into the metadata the \
         runtime registers from; this is the field `node_runtime` reads"
    );
    // The rest of the declaration is identical between the two spellings --
    // the clock is an added axis, not a different kind of entity.
    assert_eq!(recorder.entities()[1].kind, EntityKind::Timer);
    assert_eq!(recorder.entities()[1].period_us, Some(10_000));
    assert_eq!(
        recorder.entities()[1]
            .callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("on_sim_tick")
    );
}

#[test]
fn synthesized_entity_helpers_record_topic_and_callback_ids() {
    let mut recorder = MetadataRecorder::<1, 3, 2>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("talker")).unwrap();

        let publisher = node
            .create_publisher_for_topic::<TestMsg>("/chatter")
            .unwrap();
        let subscription = node
            .create_subscription_for_callback::<TestMsg>(CallbackId::new("on_message"), "/cmd")
            .unwrap();
        let _timer = node
            .create_timer_for_callback(CallbackId::new("on_tick"), TimerDuration::from_millis(10))
            .unwrap();

        node.callback(CallbackId::new("on_tick"))
            .publishes_entity(&publisher)
            .unwrap();
        node.callback(CallbackId::new("on_message"))
            .reads_entity(&subscription)
            .unwrap();

        assert_eq!(publisher.id(), EntityId::new("/chatter"));
        assert_eq!(subscription.id(), EntityId::new("on_message"));
    });

    assert_eq!(recorder.entities().len(), 3);

    let publisher = &recorder.entities()[0];
    assert_eq!(publisher.id.as_str(), "/chatter");
    assert_eq!(publisher.kind, EntityKind::Publisher);
    assert_eq!(publisher.source_name.as_str(), "/chatter");

    let subscription = &recorder.entities()[1];
    assert_eq!(subscription.id.as_str(), "on_message");
    assert_eq!(subscription.kind, EntityKind::Subscription);
    assert_eq!(subscription.source_name.as_str(), "/cmd");
    assert_eq!(
        subscription.callback_id.as_ref().map(|id| id.as_str()),
        Some("on_message")
    );

    let timer = &recorder.entities()[2];
    assert_eq!(timer.id.as_str(), "on_tick");
    assert_eq!(timer.kind, EntityKind::Timer);
    assert_eq!(
        timer.callback_id.as_ref().map(|id| id.as_str()),
        Some("on_tick")
    );

    assert_eq!(recorder.callback_effects().len(), 2);
    assert_eq!(
        recorder.callback_effects()[0].entity_id.as_str(),
        "/chatter"
    );
    assert_eq!(
        recorder.callback_effects()[1].entity_id.as_str(),
        "on_message"
    );
}

#[test]
fn named_callback_helpers_avoid_manual_callback_ids() {
    let mut recorder = MetadataRecorder::<1, 3, 2>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("listener")).unwrap();

        let publisher = node
            .create_publisher_for_topic::<TestMsg>("/chatter")
            .unwrap();
        let subscription = node
            .create_subscription_for_callback_name::<TestMsg>("on_message", "/chatter")
            .unwrap();
        let timer = node
            .create_timer_for_callback_name("on_tick", TimerDuration::from_millis(10))
            .unwrap();

        node.callback_for_name("on_message")
            .reads_entity(&subscription)
            .unwrap();
        node.callback_for_name("on_tick")
            .publishes_entity(&publisher)
            .unwrap();

        assert_eq!(subscription.id().as_str(), "on_message");
        assert_eq!(timer.id().as_str(), "on_tick");
    });

    assert_eq!(
        recorder.entities()[1]
            .callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("on_message")
    );
    assert_eq!(
        recorder.entities()[2]
            .callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("on_tick")
    );
    assert_eq!(
        recorder.callback_effects()[0].callback_id.as_str(),
        "on_message"
    );
    assert_eq!(
        recorder.callback_effects()[0].entity_id.as_str(),
        "on_message"
    );
    assert_eq!(
        recorder.callback_effects()[1].callback_id.as_str(),
        "on_tick"
    );
    assert_eq!(
        recorder.callback_effects()[1].entity_id.as_str(),
        "/chatter"
    );
}

#[test]
fn synthesized_entity_ids_reject_collisions() {
    let mut recorder = MetadataRecorder::<1, 2, 0>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("talker")).unwrap();

        node.create_publisher_for_topic::<TestMsg>("/chatter")
            .unwrap();
        let result = node.create_publisher_for_topic::<TestMsg>("/chatter");

        assert!(matches!(
            result,
            Err(NodeDeclError::Metadata(NodeMetadataError::DuplicateId))
        ));
    });
}

#[test]
fn component_rejects_effect_for_unknown_entity() {
    let mut recorder = MetadataRecorder::<1, 1, 1>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let result = context
            .callback(CallbackId::new("cb"))
            .reads(EntityId::new("missing"));
        assert!(matches!(
            result,
            Err(NodeDeclError::Metadata(NodeMetadataError::UnknownEntity))
        ));
    });
}

struct RobotComponent;

impl Component for RobotComponent {
    const NAME: &'static str = "robot_component";

    fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
        {
            let mut sensors = context
                .create_node_with_id(NodeId::new("node_sensors"), NodeOptions::new("sensors"))?;
            let _status =
                sensors.declare_publisher::<TestMsg>(EntityId::new("pub_status"), "~/status")?;
        }

        let mut control = context
            .create_node_with_id(NodeId::new("node_control"), NodeOptions::new("control"))?;
        let _cmd = control.declare_subscription::<TestMsg>(
            EntityId::new("sub_cmd"),
            CallbackId::new("cb_cmd"),
            "~/cmd",
        )?;
        let _reset = control.create_service_server::<TestService>(
            EntityId::new("srv_reset"),
            CallbackId::new("cb_reset"),
            "reset",
        )?;
        let _navigate = control.create_action_server_with_callbacks::<TestAction>(
            EntityId::new("act_navigate"),
            CallbackId::new("cb_nav_goal"),
            CallbackId::new("cb_nav_cancel"),
            CallbackId::new("cb_nav_accepted"),
            "~/navigate",
        )?;
        let _gain = control.declare_parameter_with_default(
            EntityId::new("param_gain"),
            "gain",
            ParameterDefault::Double(metadata_str("1.5")),
        )?;

        control
            .callback(CallbackId::new("cb_cmd"))
            .publishes(EntityId::new("pub_status"))?
            .reads(EntityId::new("param_gain"))?;
        control
            .callback(CallbackId::new("cb_nav_accepted"))
            .writes(EntityId::new("param_gain"))?;

        Ok(())
    }
}

/// Verifies the component API records multi-node services, actions, and defaults.
#[test]
fn component_api_records_multi_node_services() {
    let mut recorder = MetadataRecorder::<4, 12, 4>::new();
    record::<RobotComponent>(&mut recorder).unwrap();

    assert_eq!(recorder.nodes().len(), 2);
    assert_eq!(recorder.nodes()[0].id.as_str(), "node_sensors");
    assert_eq!(recorder.nodes()[1].id.as_str(), "node_control");

    let status = recorder
        .entities()
        .iter()
        .find(|entity| entity.id.as_str() == "pub_status")
        .unwrap();
    assert_eq!(status.kind, EntityKind::Publisher);
    assert_eq!(status.source_name.as_str(), "~/status");
    assert_eq!(status.source_name_kind, SourceNameKind::Private);

    let reset = recorder
        .entities()
        .iter()
        .find(|entity| entity.id.as_str() == "srv_reset")
        .unwrap();
    assert_eq!(reset.kind, EntityKind::ServiceServer);
    assert_eq!(
        reset.callback_id.as_ref().map(|id| id.as_str()),
        Some("cb_reset")
    );

    let navigate = recorder
        .entities()
        .iter()
        .find(|entity| entity.id.as_str() == "act_navigate")
        .unwrap();
    assert_eq!(navigate.kind, EntityKind::ActionServer);
    assert_eq!(
        navigate.callback_id.as_ref().map(|id| id.as_str()),
        Some("cb_nav_goal")
    );
    assert_eq!(
        navigate
            .action_cancel_callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("cb_nav_cancel")
    );
    assert_eq!(
        navigate
            .action_accepted_callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("cb_nav_accepted")
    );

    let gain = recorder
        .entities()
        .iter()
        .find(|entity| entity.id.as_str() == "param_gain")
        .unwrap();
    assert_eq!(gain.kind, EntityKind::Parameter);
    assert!(matches!(
        gain.parameter_default.as_ref(),
        Some(ParameterDefault::Double(value)) if value.as_str() == "1.5"
    ));

    assert_eq!(recorder.callback_effects().len(), 3);
    assert!(recorder.callback_effects().iter().any(|effect| {
        effect.callback_id.as_str() == "cb_cmd"
            && effect.kind == CallbackEffectKind::Publishes
            && effect.entity_id.as_str() == "pub_status"
    }));
    assert!(recorder.callback_effects().iter().any(|effect| {
        effect.callback_id.as_str() == "cb_nav_accepted"
            && effect.kind == CallbackEffectKind::Writes
            && effect.entity_id.as_str() == "param_gain"
    }));
}

#[test]
fn component_api_json_contains_planner_callback_links() {
    let mut recorder = MetadataRecorder::<4, 12, 4>::new();
    record::<RobotComponent>(&mut recorder).unwrap();

    let json = recorder
        .to_source_metadata_json(&nros::SourceMetadataExport::new(
            "demo_robot",
            RobotComponent::NAME,
        ))
        .unwrap();

    assert!(json.contains("\"callbacks\":["));
    assert!(json.contains("\"id\":\"cb_cmd\",\"declaration_slot\":0"));
    assert!(json.contains("\"kind\":\"subscription\""));
    assert!(json.contains("\"id\":\"cb_reset\",\"declaration_slot\":1"));
    assert!(json.contains("\"kind\":\"service\""));
    assert!(json.contains("\"id\":\"cb_nav_goal\",\"declaration_slot\":2"));
    assert!(json.contains("\"kind\":\"action_goal\""));
    assert!(json.contains("\"id\":\"cb_nav_cancel\",\"declaration_slot\":3"));
    assert!(json.contains("\"kind\":\"action_cancel\""));
    assert!(json.contains("\"id\":\"cb_nav_accepted\",\"declaration_slot\":4"));
    assert!(json.contains("\"kind\":\"action_accepted\""));
    assert!(json.contains("\"kind\":\"publishes\",\"entity\":\"pub_status\""));
    assert!(json.contains("\"kind\":\"reads_parameter\",\"entity\":\"param_gain\""));
    assert!(json.contains("\"kind\":\"writes_parameter\",\"entity\":\"param_gain\""));
    assert!(json.contains("\"goal_callback\":\"cb_nav_goal\""));
    assert!(json.contains("\"cancel_callback\":\"cb_nav_cancel\""));
    assert!(json.contains("\"accepted_callback\":\"cb_nav_accepted\""));
}

#[test]
fn create_subscription_static_returns_tag_matching_topic() {
    let mut recorder = MetadataRecorder::<1, 1, 1>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("listener")).unwrap();
        let tag = node
            .create_subscription_static::<TestMsg>("/chatter")
            .unwrap();

        assert_eq!(tag.as_str(), "/chatter");
        assert!(tag == CallbackId::new("/chatter"));
    });

    assert_eq!(recorder.entities().len(), 1);
    let entity = &recorder.entities()[0];
    assert_eq!(entity.kind, EntityKind::Subscription);
    assert_eq!(entity.source_name.as_str(), "/chatter");
    assert_eq!(
        entity.callback_id.as_ref().map(|id| id.as_str()),
        Some("/chatter")
    );
}

#[test]
fn create_service_static_returns_tag() {
    let mut recorder = MetadataRecorder::<1, 1, 1>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("server")).unwrap();
        let tag = node
            .create_service_static::<TestService>("/add_two_ints")
            .unwrap();

        assert_eq!(tag.as_str(), "/add_two_ints");
        assert!(tag == CallbackId::new("/add_two_ints"));
    });

    assert_eq!(recorder.entities().len(), 1);
    let entity = &recorder.entities()[0];
    assert_eq!(entity.kind, EntityKind::ServiceServer);
    assert_eq!(entity.source_name.as_str(), "/add_two_ints");
    assert_eq!(
        entity.callback_id.as_ref().map(|id| id.as_str()),
        Some("/add_two_ints")
    );
}

#[test]
fn create_service_helpers_use_name_as_entity_and_callback_id() {
    let mut recorder = MetadataRecorder::<1, 2, 1>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("services")).unwrap();
        let server = node
            .create_service_server_for_name::<TestService>("/add_two_ints")
            .unwrap();
        let client = node
            .create_service_client_for_name::<TestService>("/reset")
            .unwrap();

        assert_eq!(server.id(), EntityId::new("/add_two_ints"));
        assert_eq!(client.id(), EntityId::new("/reset"));
    });

    assert_eq!(recorder.entities().len(), 2);

    let server = &recorder.entities()[0];
    assert_eq!(server.kind, EntityKind::ServiceServer);
    assert_eq!(server.id.as_str(), "/add_two_ints");
    assert_eq!(server.source_name.as_str(), "/add_two_ints");
    assert_eq!(
        server.callback_id.as_ref().map(|id| id.as_str()),
        Some("/add_two_ints")
    );

    let client = &recorder.entities()[1];
    assert_eq!(client.kind, EntityKind::ServiceClient);
    assert_eq!(client.id.as_str(), "/reset");
    assert_eq!(client.source_name.as_str(), "/reset");
    assert!(client.callback_id.is_none());
}

#[test]
fn create_action_static_returns_tag() {
    let mut recorder = MetadataRecorder::<1, 1, 1>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("server")).unwrap();
        let tag = node
            .create_action_static::<TestAction>("/fibonacci")
            .unwrap();

        assert_eq!(tag.as_str(), "/fibonacci");
        assert!(tag == CallbackId::new("/fibonacci"));
    });

    assert_eq!(recorder.entities().len(), 1);
    let entity = &recorder.entities()[0];
    assert_eq!(entity.kind, EntityKind::ActionServer);
    assert_eq!(entity.source_name.as_str(), "/fibonacci");
    assert_eq!(
        entity.callback_id.as_ref().map(|id| id.as_str()),
        Some("/fibonacci")
    );
    assert_eq!(
        entity
            .action_cancel_callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("/fibonacci")
    );
    assert_eq!(
        entity
            .action_accepted_callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("/fibonacci")
    );
}

#[test]
fn create_action_helpers_use_name_as_entity_and_default_callback_id() {
    let mut recorder = MetadataRecorder::<1, 2, 3>::new();
    let mut __exec = metadata_executor();
    nros::__record_with_context("test", &mut recorder, &mut __exec, |context| {
        let mut node = context.create_node(NodeOptions::new("actions")).unwrap();
        let server = node
            .create_action_server_for_name::<TestAction>("/fibonacci")
            .unwrap();
        let client = node
            .create_action_client_for_name::<TestAction>("/navigate")
            .unwrap();

        assert_eq!(server.id(), EntityId::new("/fibonacci"));
        assert_eq!(client.id(), EntityId::new("/navigate"));
    });

    assert_eq!(recorder.entities().len(), 2);

    let server = &recorder.entities()[0];
    assert_eq!(server.kind, EntityKind::ActionServer);
    assert_eq!(server.id.as_str(), "/fibonacci");
    assert_eq!(server.source_name.as_str(), "/fibonacci");
    assert_eq!(
        server.callback_id.as_ref().map(|id| id.as_str()),
        Some("/fibonacci")
    );
    assert_eq!(
        server
            .action_cancel_callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("/fibonacci")
    );
    assert_eq!(
        server
            .action_accepted_callback_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("/fibonacci")
    );

    let client = &recorder.entities()[1];
    assert_eq!(client.kind, EntityKind::ActionClient);
    assert_eq!(client.id.as_str(), "/navigate");
    assert_eq!(client.source_name.as_str(), "/navigate");
    assert!(client.callback_id.is_none());
}

// Phase 268 W1 — unit test: launch node_identity injection overrides NodeOptions
// default (RFC-0046). Uses a `CapturingRuntime` that applies the same
// `match self.node_identity` logic as `ExecutorSink::create_node` and records the
// resolved (name, namespace). No executor needed — tests the design contract.
// Uses `MetadataString` (heapless::String re-export) to stay no_std-compatible.
#[test]
fn node_identity_injected_wins_over_node_options() {
    /// Minimal NodeRuntime that applies the Phase 268 W1 identity override
    /// (same logic as `ExecutorSink::create_node`) and records the resolved values.
    struct CapturingRuntime {
        node_identity: Option<(&'static str, &'static str)>,
        resolved_name: MetadataString,
        resolved_ns: MetadataString,
    }
    impl NodeRuntime for CapturingRuntime {
        fn create_node(&mut self, _id: NodeId<'_>, options: NodeOptions<'_>) -> NodeResult<()> {
            // Mirror ExecutorSink::create_node override logic (RFC-0046).
            let (name, ns) = match self.node_identity {
                Some((n, s)) => (n, s),
                None => (options.name, options.namespace),
            };
            self.resolved_name.clear();
            let _ = self.resolved_name.push_str(name);
            self.resolved_ns.clear();
            let _ = self.resolved_ns.push_str(ns);
            Ok(())
        }
        fn create_entity(&mut self, _m: EntityMetadata) -> NodeResult<()> {
            Ok(())
        }
        fn record_callback_effect(
            &mut self,
            _id: CallbackId<'_>,
            _kind: nros::node_metadata::CallbackEffectKind,
            _entity: EntityId<'_>,
        ) -> NodeResult<()> {
            Ok(())
        }
    }

    // (a) Injected identity wins over NodeOptions default.
    let mut rt_a = CapturingRuntime {
        node_identity: Some(("launched", "/ns")),
        resolved_name: MetadataString::new(),
        resolved_ns: MetadataString::new(),
    };
    {
        let mut __exec = metadata_executor();
        nros::__record_with_context("test_node", &mut rt_a, &mut __exec, |ctx| {
            ctx.create_node(NodeOptions::new("default").namespace("/d"))
                .unwrap();
        });
    }
    assert_eq!(rt_a.resolved_name.as_str(), "launched");
    assert_eq!(rt_a.resolved_ns.as_str(), "/ns");

    // (b) None → NodeOptions default stands (backward-compatible).
    let mut rt_b = CapturingRuntime {
        node_identity: None,
        resolved_name: MetadataString::new(),
        resolved_ns: MetadataString::new(),
    };
    {
        let mut __exec = metadata_executor();
        nros::__record_with_context("test_node", &mut rt_b, &mut __exec, |ctx| {
            ctx.create_node(NodeOptions::new("default").namespace("/d"))
                .unwrap();
        });
    }
    assert_eq!(rt_b.resolved_name.as_str(), "default");
    assert_eq!(rt_b.resolved_ns.as_str(), "/d");
}

// Phase 305 W3 (issue 0255) — unit test: entity source names are resolved
// through the launch remap seam against the node identity `create_node`
// stored. Applies the same `resolve_name` call shape as
// `ExecutorSink::create_entity` (Timer/Parameter exempt) and records the
// resolved wire name. No executor needed — tests the design contract.
#[test]
fn entity_names_resolved_through_launch_remaps() {
    struct CapturingRuntime {
        node_identity: (&'static str, &'static str),
        remaps: &'static [(&'static str, &'static str)],
        resolved: MetadataString,
    }
    impl NodeRuntime for CapturingRuntime {
        fn create_node(&mut self, _id: NodeId<'_>, _o: NodeOptions<'_>) -> NodeResult<()> {
            Ok(())
        }
        fn create_entity(&mut self, m: EntityMetadata) -> NodeResult<()> {
            // Mirror ExecutorSink::create_entity kind gating + resolution.
            let name = match m.kind {
                EntityKind::Timer | EntityKind::Parameter => m.source_name.clone(),
                _ => nros::node_metadata::resolve_name(
                    m.source_name.as_str(),
                    self.node_identity.0,
                    self.node_identity.1,
                    self.remaps.iter().copied(),
                )
                .map_err(|_| NodeDeclError::Runtime)?,
            };
            self.resolved.clear();
            let _ = self.resolved.push_str(name.as_str());
            Ok(())
        }
        fn record_callback_effect(
            &mut self,
            _id: CallbackId<'_>,
            _kind: nros::node_metadata::CallbackEffectKind,
            _entity: EntityId<'_>,
        ) -> NodeResult<()> {
            Ok(())
        }
    }

    let mut rt = CapturingRuntime {
        node_identity: ("filter", "/sensing"),
        remaps: &[("~/input/points", "/points_raw")],
        resolved: MetadataString::new(),
    };
    {
        let mut __exec = metadata_executor();
        nros::__record_with_context("test_node", &mut rt, &mut __exec, |ctx| {
            let mut node = ctx
                .create_node(NodeOptions::new("filter").namespace("/sensing"))
                .unwrap();
            // Remapped private name → the rule's target.
            node.create_subscription_for_callback_name::<TestMsg>("cb", "~/input/points")
                .unwrap();
        });
    }
    assert_eq!(rt.resolved.as_str(), "/points_raw");

    // Un-remapped relative name → plain expansion.
    {
        let mut __exec = metadata_executor();
        nros::__record_with_context("test_node", &mut rt, &mut __exec, |ctx| {
            let mut node = ctx
                .create_node(NodeOptions::new("filter").namespace("/sensing"))
                .unwrap();
            node.create_publisher_for_topic::<TestMsg>("status")
                .unwrap();
        });
    }
    assert_eq!(rt.resolved.as_str(), "/sensing/status");

    // Parameter names bypass the remap seam.
    {
        let mut __exec = metadata_executor();
        nros::__record_with_context("test_node", &mut rt, &mut __exec, |ctx| {
            let mut node = ctx
                .create_node(NodeOptions::new("filter").namespace("/sensing"))
                .unwrap();
            node.declare_parameter_for_name("~/input/points", nros::ParameterType::Bool)
                .unwrap();
        });
    }
    assert_eq!(rt.resolved.as_str(), "~/input/points");
}

/// A component's node names are its stable node ids, so creating the same
/// name twice in one registration is refused, not silently merged.
#[test]
fn synthesized_node_ids_reject_duplicate_names() {
    let mut recorder = MetadataRecorder::<2, 0, 0>::new();
    let mut exec = metadata_executor();
    let second = nros::__record_with_context("test", &mut recorder, &mut exec, |context| {
        context.create_node(NodeOptions::new("talker")).unwrap();
        context.create_node(NodeOptions::new("talker")).map(|_| ())
    });
    assert!(matches!(
        second,
        Err(NodeDeclError::Metadata(NodeMetadataError::DuplicateId))
    ));
}

/// phase-483 W3 — ONE node type. A component's `register` and a standalone
/// program hand the same `nros::Node` to the same function, so node code is
/// written once and shared between the two.
#[test]
fn a_component_and_a_program_share_one_node_type() {
    fn configure(node: &mut nros::Node<'_, 'static>) -> &'static str {
        let _clock = node.get_clock();
        assert!(node.fully_qualified_name().unwrap().ends_with(node.name()));
        if node.name() == "standalone" {
            "program"
        } else {
            "component"
        }
    }

    // The standalone road: the executor creates the node.
    let mut exec = metadata_executor();
    let mut node = exec.create_node("standalone").unwrap();
    assert_eq!(configure(&mut node), "program");

    // The component road: `register`'s context creates it.
    let mut recorder = MetadataRecorder::<1, 0, 0>::new();
    let mut exec = metadata_executor();
    let seen = nros::__record_with_context("shared", &mut recorder, &mut exec, |context| {
        let mut node = context
            .create_node(NodeOptions::new("in_component"))
            .unwrap();
        configure(&mut node)
    });
    assert_eq!(seen, "component");
}

use std::fmt::Debug;

use nros_cli_core::orchestration::{
    ComponentConfig, NrosPlan, SourceMetadata,
    plan::PlanEntity,
    schema::{COMPONENT_CONFIG_VERSION, PLAN_VERSION, SOURCE_METADATA_VERSION},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

fn assert_json_fixture<T>(raw: &str) -> T
where
    T: DeserializeOwned + Serialize + PartialEq + Debug,
{
    let parsed: T = serde_json::from_str(raw).expect("fixture parses");
    let pretty = format!(
        "{}\n",
        serde_json::to_string_pretty(&parsed).expect("fixture formats")
    );
    assert_eq!(pretty, raw, "fixture must match stable pretty JSON");

    let reparsed: T = serde_json::from_str(&pretty).expect("pretty JSON parses");
    assert_eq!(reparsed, parsed);
    parsed
}

fn assert_toml_fixture<T>(raw: &str) -> T
where
    T: DeserializeOwned + Serialize + PartialEq + Debug,
{
    let parsed: T = toml::from_str(raw).expect("fixture parses");
    let formatted = toml::to_string_pretty(&parsed).expect("fixture formats");
    let reparsed: T = toml::from_str(&formatted).expect("pretty TOML parses");
    assert_eq!(reparsed, parsed);
    parsed
}

fn entity_source_artifact(entity: &PlanEntity) -> &str {
    match entity {
        PlanEntity::Publisher { trace, .. }
        | PlanEntity::Subscriber { trace, .. }
        | PlanEntity::Timer { trace, .. }
        | PlanEntity::ServiceServer { trace, .. }
        | PlanEntity::ServiceClient { trace, .. }
        | PlanEntity::ActionServer { trace, .. }
        | PlanEntity::ActionClient { trace, .. } => &trace.source_artifact.artifact,
    }
}

#[test]
fn source_metadata_json_round_trips() {
    let metadata: SourceMetadata = assert_json_fixture(include_str!(
        "fixtures/orchestration/source_metadata_talker.json"
    ));

    assert_eq!(metadata.version, SOURCE_METADATA_VERSION);
    assert_eq!(metadata.package, "demo_nodes_rs");
    assert_eq!(
        metadata.nodes[0].publishers[0].unresolved_topic.value,
        "chatter"
    );
    assert_eq!(metadata.nodes[0].timers[0].callback, "cb_timer");
}

#[test]
fn source_metadata_generated_id_fields_round_trip() {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/orchestration/source_metadata_talker.json"
    ))
    .expect("fixture parses as JSON value");

    let node = &mut value["nodes"][0];
    node["declaration_slot"] = json!(0);
    node["source_default_name"] = json!("talker");
    node["source"] = json!({
        "artifact": "src/talker.rs",
        "line": 12,
        "column": 3
    });
    node["publishers"][0]["declaration_slot"] = json!(0);
    node["timers"][0]["declaration_slot"] = json!(1);
    node["timers"][0]["callback_slot"] = json!(0);
    node["services"][0]["declaration_slot"] = json!(2);
    node["services"][0]["callback_slot"] = json!(1);
    node["actions"][0]["declaration_slot"] = json!(3);
    node["actions"][0]["goal_callback_slot"] = json!(2);
    node["actions"][0]["cancel_callback_slot"] = json!(3);
    node["actions"][0]["accepted_callback_slot"] = json!(4);
    value["callbacks"][0]["declaration_slot"] = json!(0);
    value["callbacks"][0]["effects"][0]["entity_slot"] = json!(0);
    value["parameters"][0]["declaration_slot"] = json!(4);

    let raw = serde_json::to_string_pretty(&value).expect("formats modified source metadata");
    let metadata: SourceMetadata =
        serde_json::from_str(&raw).expect("new generated-ID fields parse");

    let node = &metadata.nodes[0];
    assert_eq!(node.declaration_slot, Some(0));
    assert_eq!(node.source_default_name.as_deref(), Some("talker"));
    assert_eq!(
        node.source.as_ref().map(|source| source.artifact.as_str()),
        Some("src/talker.rs")
    );
    assert_eq!(node.timers[0].callback_slot, Some(0));
    assert_eq!(node.actions[0].accepted_callback_slot, Some(4));
    assert_eq!(metadata.callbacks[0].declaration_slot, Some(0));
    assert_eq!(metadata.callbacks[0].effects[0].entity_slot, Some(0));
    assert_eq!(metadata.parameters[0].declaration_slot, Some(4));

    let reparsed: SourceMetadata = serde_json::from_str(
        &serde_json::to_string_pretty(&metadata).expect("source metadata formats"),
    )
    .expect("formatted source metadata parses");
    assert_eq!(reparsed, metadata);
}

#[test]
fn source_metadata_name_and_effect_edges_round_trip() {
    let metadata: SourceMetadata = assert_json_fixture(include_str!(
        "fixtures/orchestration/source_metadata_names_effects.json"
    ));

    let node = &metadata.nodes[0];
    assert_eq!(node.unresolved_name.value, "/robot/controller");
    assert_eq!(node.publishers[0].unresolved_topic.value, "/diagnostics");
    assert_eq!(node.subscribers[0].unresolved_topic.value, "~/cmd");
    assert_eq!(metadata.callbacks[0].effects.len(), 2);
    assert_eq!(metadata.callbacks[2].effects.len(), 2);
}

#[test]
fn all_fixtures_use_current_schema_versions() {
    let source_fixtures = [
        include_str!("fixtures/orchestration/source_metadata_talker.json"),
        include_str!("fixtures/orchestration/source_metadata_names_effects.json"),
    ];
    for raw in source_fixtures {
        let metadata: SourceMetadata = assert_json_fixture(raw);
        assert_eq!(metadata.version, SOURCE_METADATA_VERSION);
    }

    let plan_fixtures = [
        include_str!("fixtures/orchestration/plan_pub_sub.json"),
        include_str!("fixtures/orchestration/plan_multi_instance.json"),
        include_str!("fixtures/orchestration/plan_service_action.json"),
        include_str!("fixtures/orchestration/plan_edge_instances_names.json"),
    ];
    for raw in plan_fixtures {
        let plan: NrosPlan = assert_json_fixture(raw);
        assert_eq!(plan.version, PLAN_VERSION);
    }

    let component: ComponentConfig =
        assert_toml_fixture(include_str!("fixtures/orchestration/component_nros.toml"));
    assert_eq!(component.version, COMPONENT_CONFIG_VERSION);
}

#[test]
fn component_nros_toml_round_trips() {
    let config: ComponentConfig =
        assert_toml_fixture(include_str!("fixtures/orchestration/component_nros.toml"));

    assert_eq!(config.version, COMPONENT_CONFIG_VERSION);
    assert_eq!(config.package, "demo_nodes_rs");
    assert_eq!(
        config.metadata.source_metadata,
        "target/nros/metadata/talker.json"
    );
}

#[test]
fn pub_sub_plan_json_round_trips() {
    let plan: NrosPlan =
        assert_json_fixture(include_str!("fixtures/orchestration/plan_pub_sub.json"));

    assert_eq!(plan.version, PLAN_VERSION);
    assert_eq!(plan.instances.len(), 2);
    let first_entity = &plan.instances[0].nodes[0].entities[0];
    let PlanEntity::Publisher { resolved_name, .. } = first_entity else {
        panic!("first entity should be publisher: {first_entity:?}");
    };
    assert_eq!(resolved_name, "/chatter");
    assert!(
        matches!(
            plan.instances[0].nodes[0].entities[1],
            PlanEntity::Timer { .. }
        ),
        "timer entity must not require graph name or interface"
    );
}

#[test]
fn multi_instance_plan_json_round_trips() {
    let plan: NrosPlan = assert_json_fixture(include_str!(
        "fixtures/orchestration/plan_multi_instance.json"
    ));

    assert_eq!(plan.version, PLAN_VERSION);
    assert_eq!(plan.instances.len(), 2);
    assert_ne!(plan.instances[0].id, plan.instances[1].id);
    assert_eq!(plan.instances[0].component, plan.instances[1].component);
}

#[test]
fn edge_instance_name_and_sched_variants_round_trip() {
    let plan: NrosPlan = assert_json_fixture(include_str!(
        "fixtures/orchestration/plan_edge_instances_names.json"
    ));

    assert_eq!(plan.version, PLAN_VERSION);
    assert_eq!(plan.instances.len(), 2);
    assert_eq!(plan.instances[0].component, plan.instances[1].component);
    assert_eq!(plan.instances[0].remaps[0].from, "~/cmd");
    assert_eq!(plan.instances[0].remaps[1].from, "/diagnostics");
    assert_eq!(plan.sched_contexts.len(), 4);
    assert_eq!(
        plan.sched_contexts[0].class,
        nros_cli_core::orchestration::schema::SchedClass::BestEffort
    );
    assert_eq!(
        plan.sched_contexts[1].class,
        nros_cli_core::orchestration::schema::SchedClass::RealTime
    );
    assert_eq!(
        plan.sched_contexts[2].class,
        nros_cli_core::orchestration::schema::SchedClass::TimeTriggered
    );
    assert_eq!(
        plan.sched_contexts[3].class,
        nros_cli_core::orchestration::schema::SchedClass::Interrupt
    );
}

#[test]
fn service_action_plan_json_round_trips() {
    let plan: NrosPlan = assert_json_fixture(include_str!(
        "fixtures/orchestration/plan_service_action.json"
    ));

    assert_eq!(plan.version, PLAN_VERSION);
    assert!(matches!(
        plan.instances[0].nodes[0].entities[0],
        PlanEntity::ServiceServer { .. }
    ));
    assert!(matches!(
        plan.instances[0].nodes[0].entities[1],
        PlanEntity::ActionServer { .. }
    ));
    assert!(!entity_source_artifact(&plan.instances[0].nodes[0].entities[0]).is_empty());
    assert!(!entity_source_artifact(&plan.instances[0].nodes[0].entities[1]).is_empty());
}

#[test]
fn plan_fixtures_keep_traceability_context() {
    let plan: NrosPlan =
        assert_json_fixture(include_str!("fixtures/orchestration/plan_pub_sub.json"));

    assert!(!plan.trace.system_config.is_empty());
    assert!(!plan.trace.launch_record.is_empty());
    for component in &plan.components {
        assert!(!component.source_metadata.is_empty());
    }
    for instance in &plan.instances {
        assert!(!instance.trace.launch_record_entity.is_empty());
        assert!(!instance.trace.source_metadata.is_empty());
        for node in &instance.nodes {
            assert!(!node.source_node.is_empty());
            for entity in &node.entities {
                assert!(!entity_source_artifact(entity).is_empty());
            }
        }
        for callback in &instance.callbacks {
            assert!(!callback.source.artifact.is_empty());
        }
        for parameter in &instance.parameters {
            assert!(!parameter.source.artifact.is_empty());
        }
        for binding in &instance.sched_bindings {
            assert!(!binding.source.is_empty());
        }
    }
}

#[test]
fn plan_generated_id_provenance_fields_round_trip() {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/orchestration/plan_pub_sub.json"))
            .expect("fixture parses as JSON value");

    let instance = &mut value["instances"][0];
    instance["launch_instance"] = json!({
        "index": 0,
        "record_entity": "record://nodes/0",
        "package": "demo_nodes_rs",
        "executable": "talker",
        "launch_name": "talker"
    });

    let node = &mut instance["nodes"][0];
    node["source_default_name"] = json!("talker");
    node["declaration_slot"] = json!(0);
    node["source"] = json!({
        "artifact": "src/talker.rs",
        "line": 12,
        "column": 3
    });

    node["entities"][0]["trace"]["declaration_slot"] = json!(0);
    instance["callbacks"][0]["declaration_slot"] = json!(0);

    let raw = serde_json::to_string_pretty(&value).expect("formats modified plan");
    let plan: NrosPlan = serde_json::from_str(&raw).expect("new provenance fields parse");

    let launch = plan.instances[0]
        .launch_instance
        .as_ref()
        .expect("launch provenance present");
    assert_eq!(launch.index, Some(0));
    assert_eq!(launch.package.as_deref(), Some("demo_nodes_rs"));
    assert_eq!(launch.executable.as_deref(), Some("talker"));

    let planned_node = &plan.instances[0].nodes[0];
    assert_eq!(planned_node.source_default_name.as_deref(), Some("talker"));
    assert_eq!(planned_node.declaration_slot, Some(0));
    assert_eq!(
        planned_node
            .source
            .as_ref()
            .map(|source| source.artifact.as_str()),
        Some("src/talker.rs")
    );

    let PlanEntity::Publisher { trace, .. } = &planned_node.entities[0] else {
        panic!("first entity should be publisher");
    };
    assert_eq!(trace.declaration_slot, Some(0));
    assert_eq!(plan.instances[0].callbacks[0].declaration_slot, Some(0));

    let reparsed: NrosPlan = serde_json::from_str(
        &serde_json::to_string_pretty(&plan).expect("provenance plan formats"),
    )
    .expect("formatted provenance plan parses");
    assert_eq!(reparsed, plan);
}

#[test]
fn unknown_fields_are_rejected() {
    let mut raw = include_str!("fixtures/orchestration/source_metadata_talker.json").to_owned();
    raw = raw.replacen(
        "\"version\": 1,",
        "\"version\": 1,\n  \"unexpected\": true,",
        1,
    );

    let error = serde_json::from_str::<SourceMetadata>(&raw).expect_err("unknown field rejected");
    assert!(
        error.to_string().contains("unknown field"),
        "error should mention unknown field: {error}"
    );
}

#[test]
fn unknown_toml_fields_are_rejected() {
    let raw = include_str!("fixtures/orchestration/component_nros.toml").replacen(
        "component = \"talker\"",
        "component = \"talker\"\nunexpected = true",
        1,
    );

    let error = toml::from_str::<ComponentConfig>(&raw).expect_err("unknown field rejected");
    assert!(
        error.to_string().contains("unknown field"),
        "error should mention unknown field: {error}"
    );
}

#[test]
fn nested_unknown_fields_are_rejected() {
    let raw = include_str!("fixtures/orchestration/plan_pub_sub.json").replacen(
        "\"deadline_policy\": \"warn\",",
        "\"deadline_policy\": \"warn\",\n      \"unexpected_sched_field\": true,",
        1,
    );

    let error = serde_json::from_str::<NrosPlan>(&raw).expect_err("nested unknown field rejected");
    assert!(
        error.to_string().contains("unknown field"),
        "error should mention unknown nested field: {error}"
    );
}

#[test]
fn version_fields_are_required() {
    let raw = include_str!("fixtures/orchestration/plan_pub_sub.json").replacen(
        "  \"version\": 2,\n",
        "",
        1,
    );

    let error = serde_json::from_str::<NrosPlan>(&raw).expect_err("missing version rejected");
    assert!(
        error.to_string().contains("missing field `version`"),
        "error should mention missing version: {error}"
    );
}

//! One graph, three component surfaces, one answer (phase-572 W2).
//!
//! The `workspace-graph-query-parity` image launches a C, a C++ and a Rust
//! component. Each asks the ROS graph for its node, topic and service counts
//! and for `/chatter`'s publisher count and type, through its own language's
//! surface: the C
//! component's `rclc_executor_t *`, the C++ component's `rclcpp::Node`, and
//! the Rust component's `TickCtx`. Each prints once its answer has held for
//! five probes. With a talker on `/chatter`, the three answers must agree, and
//! must see it.
//!
//! Compiles nothing: the image and the talker are prebuilt fixture rows.

use std::{collections::BTreeMap, process::Command, time::Duration};

use nros_tests::{
    fixtures::{
        self, ManagedProcess, RequireFixture, Rmw, ZenohRouter, build_native_c_talker_rmw,
        require_zenohd, zenohd_unique,
    },
    lane_scope,
    matrix::PlatformId,
    output,
};
use rstest::rstest;

const LANGS: [&str; 3] = ["c", "cpp", "rust"];

/// `lang -> "nodes=.. topics=.. services=.. chatter_publishers=.. chatter_type=.."`.
fn answers(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let at = l.find(output::GRAPH_QUERY_MARKER)?;
            let rest = &l[at + output::GRAPH_QUERY_MARKER.len()..];
            let (lang, answer) = rest.split_once(' ')?;
            Some((lang.to_string(), answer.trim().to_string()))
        })
        .collect()
}

#[rstest]
fn the_c_cpp_and_rust_component_surfaces_answer_one_graph_alike(
    #[from(zenohd_unique)] router: ZenohRouter,
) {
    if !lane_scope::admits(PlatformId::Linux) {
        nros_tests::lane_skip!(
            "{}",
            lane_scope::skip_note(PlatformId::Linux, "graph-query-parity")
        );
    }
    require_zenohd();
    let talker_bin = build_native_c_talker_rmw(Rmw::Zenoh).require("linux-c-zenoh-talker");
    let probe_bin = fixtures::build_workspace_cmake_generated_entry(
        "workspace-graph-query-parity",
        "linux_entry",
    )
    .require("workspace-graph-query-parity linux_entry");

    let mut talker_cmd = Command::new(talker_bin);
    talker_cmd.env("NROS_LOCATOR", router.locator());
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "talker").expect("spawn talker");
    talker
        .wait_for_output_count(output::TALKER_LOG_PREFIX, 1, Duration::from_secs(15))
        .expect("the talker did not publish");

    let mut probe_cmd = Command::new(probe_bin);
    probe_cmd
        .env("NROS_LOCATOR", router.locator())
        .env("RUST_LOG", "info");
    let mut probe = ManagedProcess::spawn_image(probe_cmd, "graph-query").expect("spawn probe");
    let text = probe
        .wait_for_output_count(
            output::GRAPH_QUERY_MARKER,
            LANGS.len(),
            Duration::from_secs(30),
        )
        .expect("every probe answers once it sees the talker");
    probe.kill();
    talker.kill();

    let got = answers(&text);
    eprintln!("graph-query answers: {got:?}");
    assert_eq!(
        got.keys().map(String::as_str).collect::<Vec<_>>(),
        LANGS,
        "one answer per language:\n{text}"
    );
    let c = &got["c"];
    // Agreement alone would pass three probes that all saw nothing, so the
    // answer is checked against the graph the test built: the talker and the
    // three probes as nodes, the talker once on `/chatter`, with its type.
    let field = |key: &str| -> usize {
        c.split_whitespace()
            .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
            .unwrap_or_else(|| panic!("no `{key}=` count in {c}"))
    };
    assert!(
        field("nodes") >= 4,
        "the talker and the three probes are nodes: {c}"
    );
    assert!(field("topics") >= 1, "`/chatter` is a topic: {c}");
    assert_eq!(
        field("chatter_publishers"),
        1,
        "the talker is seen once: {c}"
    );
    assert!(
        !c.ends_with("chatter_type="),
        "`/chatter`'s type is seen: {c}"
    );
    for lang in ["cpp", "rust"] {
        assert_eq!(&got[lang], c, "{lang} answers what C answers:\n{text}");
    }
}

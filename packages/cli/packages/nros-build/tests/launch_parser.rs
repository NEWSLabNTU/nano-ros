//! Phase 212.N.11 — launch.xml parser regression tests (v1 tag set).

use std::{fs, path::Path};

use nros_build::{
    launch_parser::parse_launch_file,
    pkg_index::{PkgIndex, build_pkg_index},
};

fn write_package_xml(dir: &Path, name: &str) {
    fs::create_dir_all(dir).expect("mkdir pkg");
    let xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.0.1</version>
  <description>test fixture</description>
  <maintainer email="t@e.com">t</maintainer>
  <license>MIT</license>
</package>
"#
    );
    fs::write(dir.join("package.xml"), xml).unwrap();
}

fn workspace_with_pkgs(pkgs: &[&str]) -> (tempfile::TempDir, PkgIndex) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".colcon_workspace"), "").unwrap();
    for p in pkgs {
        write_package_xml(&root.join(p), p);
    }
    let index = build_pkg_index(root).expect("pkg-index");
    (tmp, index)
}

fn write_launch(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn parses_minimal_launch() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let launch = write_launch(
        tmp.path(),
        "demo.launch.xml",
        r#"<launch>
  <node pkg="X" exec="Y"/>
</launch>"#,
    );
    let desc = parse_launch_file(&launch, &index, &[]).expect("parse");
    assert_eq!(desc.nodes.len(), 1);
    assert_eq!(desc.nodes[0].pkg, "X");
    assert_eq!(desc.nodes[0].exec, "Y");
}

#[test]
fn parses_launch_with_args() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let launch = write_launch(
        tmp.path(),
        "demo.launch.xml",
        r#"<launch>
  <arg name="ns" default="/r1"/>
  <node pkg="X" exec="Y" namespace="$(var ns)"/>
</launch>"#,
    );
    let desc = parse_launch_file(&launch, &index, &[]).expect("parse");
    assert_eq!(desc.nodes.len(), 1);
    assert_eq!(desc.nodes[0].namespace.as_deref(), Some("/r1"));
    assert_eq!(desc.args.len(), 1);
    assert_eq!(desc.args[0].default.as_deref(), Some("/r1"));
}

#[test]
fn args_override_wins() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let launch = write_launch(
        tmp.path(),
        "demo.launch.xml",
        r#"<launch>
  <arg name="ns" default="/r1"/>
  <node pkg="X" exec="Y" namespace="$(var ns)"/>
</launch>"#,
    );
    let override_ = vec![("ns".to_string(), "/r9".to_string())];
    let desc = parse_launch_file(&launch, &index, &override_).expect("parse");
    assert_eq!(desc.nodes[0].namespace.as_deref(), Some("/r9"));
}

#[test]
fn parses_param_and_remap() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let launch = write_launch(
        tmp.path(),
        "demo.launch.xml",
        r#"<launch>
  <node pkg="X" exec="Y">
    <param name="rate_hz" value="10"/>
    <remap from="chatter" to="topic/chatter"/>
  </node>
</launch>"#,
    );
    let desc = parse_launch_file(&launch, &index, &[]).expect("parse");
    assert_eq!(desc.nodes.len(), 1);
    let n = &desc.nodes[0];
    assert_eq!(n.params.len(), 1);
    assert_eq!(n.params[0].name, "rate_hz");
    assert_eq!(n.params[0].value, "10");
    assert_eq!(n.remaps.len(), 1);
    assert_eq!(n.remaps[0].from, "chatter");
    assert_eq!(n.remaps[0].to, "topic/chatter");
}

#[test]
fn parses_group_namespace() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let launch = write_launch(
        tmp.path(),
        "demo.launch.xml",
        r#"<launch>
  <group ns="/robot1">
    <node pkg="X" exec="Y"/>
    <node pkg="X" exec="Z" namespace="inner"/>
  </group>
</launch>"#,
    );
    let desc = parse_launch_file(&launch, &index, &[]).expect("parse");
    assert_eq!(desc.groups.len(), 1);
    let g = &desc.groups[0];
    assert_eq!(g.namespace.as_deref(), Some("/robot1"));
    assert_eq!(g.nodes.len(), 2);
    assert_eq!(g.nodes[0].namespace.as_deref(), Some("/robot1"));
    assert_eq!(g.nodes[1].namespace.as_deref(), Some("/robot1/inner"));
}

#[test]
fn parses_include_with_arg_passthrough() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let root = tmp.path();
    // Sub launch declares `mode = "auto"` (default).
    let sub = write_launch(
        root,
        "sub.launch.xml",
        r#"<launch>
  <arg name="mode" default="auto"/>
  <node pkg="X" exec="Y" name="$(var mode)"/>
</launch>"#,
    );
    let parent = write_launch(
        root,
        "parent.launch.xml",
        &format!(
            r#"<launch>
  <include file="{}">
    <arg name="mode" value="manual"/>
  </include>
</launch>"#,
            sub.display()
        ),
    );
    let desc = parse_launch_file(&parent, &index, &[]).expect("parse");
    // Sub's node is flattened up under the parent's `nodes`.
    assert_eq!(desc.nodes.len(), 1);
    assert_eq!(desc.nodes[0].name.as_deref(), Some("manual"));
    assert_eq!(desc.includes.len(), 1);
}

#[test]
fn resolves_find_substitution() {
    let (tmp, index) = workspace_with_pkgs(&["demo_bringup"]);
    // Drop a `sub.xml` inside `demo_bringup/launch/` so `$(find …)`
    // expands into a real path the parser can then read.
    fs::create_dir_all(tmp.path().join("demo_bringup/launch")).unwrap();
    fs::write(
        tmp.path().join("demo_bringup/launch/sub.xml"),
        r#"<launch>
  <node pkg="demo_bringup" exec="sub_node"/>
</launch>"#,
    )
    .unwrap();
    let parent = write_launch(
        tmp.path(),
        "parent.launch.xml",
        r#"<launch>
  <include file="$(find demo_bringup)/launch/sub.xml"/>
</launch>"#,
    );
    let desc = parse_launch_file(&parent, &index, &[]).expect("parse");
    assert_eq!(desc.includes.len(), 1);
    assert!(
        desc.includes[0]
            .file
            .ends_with("demo_bringup/launch/sub.xml")
    );
    assert_eq!(desc.nodes.len(), 1);
    assert_eq!(desc.nodes[0].pkg, "demo_bringup");
    assert_eq!(desc.nodes[0].exec, "sub_node");
}

#[test]
fn errors_on_include_cycle() {
    let (tmp, index) = workspace_with_pkgs(&["X"]);
    let root = tmp.path();
    let a = root.join("a.launch.xml");
    let b = root.join("b.launch.xml");
    fs::write(
        &a,
        format!(r#"<launch><include file="{}"/></launch>"#, b.display()),
    )
    .unwrap();
    fs::write(
        &b,
        format!(r#"<launch><include file="{}"/></launch>"#, a.display()),
    )
    .unwrap();
    let err = parse_launch_file(&a, &index, &[]).expect_err("cycle must error");
    let msg = format!("{err:#}");
    assert!(msg.contains("cycle"), "diagnostic: {msg}");
}

#[test]
fn errors_on_unknown_pkg_in_find() {
    let (tmp, index) = workspace_with_pkgs(&["other_pkg"]);
    let launch = write_launch(
        tmp.path(),
        "demo.launch.xml",
        r#"<launch>
  <node pkg="$(find nope)" exec="Y"/>
</launch>"#,
    );
    let err = parse_launch_file(&launch, &index, &[]).expect_err("unknown pkg");
    let msg = format!("{err:#}");
    assert!(msg.contains("nope"), "diagnostic: {msg}");
}

#[test]
fn accepts_nav2_launch_smoke() {
    // Nav2-style launch.xml fragment: 8 nodes, `$(find …)`, `$(var …)`,
    // `<group>`, `<include>`, `<arg>`, `<remap>`. The parser must not
    // error out on any tag in the v1 set.
    let (tmp, index) = workspace_with_pkgs(&[
        "nav2_bringup",
        "nav2_amcl",
        "nav2_map_server",
        "nav2_planner",
        "nav2_controller",
        "nav2_bt_navigator",
        "nav2_lifecycle_manager",
        "nav2_recoveries",
        "nav2_waypoint_follower",
    ]);
    let root = tmp.path();
    // Stage a `nav2_bringup/launch/sub.launch.xml` that the include resolves to.
    fs::create_dir_all(root.join("nav2_bringup/launch")).unwrap();
    fs::write(
        root.join("nav2_bringup/launch/sub.launch.xml"),
        r#"<launch>
  <arg name="namespace" default=""/>
  <node pkg="nav2_lifecycle_manager" exec="lifecycle_manager_navigation"/>
</launch>
"#,
    )
    .unwrap();
    let parent = write_launch(
        root,
        "bringup.launch.xml",
        r#"<launch>
  <arg name="namespace" default=""/>
  <arg name="use_sim_time" default="true"/>
  <arg name="map" default="map.yaml"/>
  <arg name="autostart" default="true"/>

  <group ns="$(var namespace)">
    <node pkg="nav2_amcl" exec="amcl" name="amcl">
      <param name="use_sim_time" value="$(var use_sim_time)"/>
      <remap from="scan" to="base_scan"/>
    </node>
    <node pkg="nav2_map_server" exec="map_server" name="map_server">
      <param name="yaml_filename" value="$(var map)"/>
    </node>
    <node pkg="nav2_planner" exec="planner_server" name="planner_server"/>
    <node pkg="nav2_controller" exec="controller_server" name="controller_server"/>
    <node pkg="nav2_bt_navigator" exec="bt_navigator" name="bt_navigator"/>
    <node pkg="nav2_recoveries" exec="recoveries_server" name="recoveries_server"/>
    <node pkg="nav2_waypoint_follower" exec="waypoint_follower" name="waypoint_follower"/>
    <include file="$(find nav2_bringup)/launch/sub.launch.xml">
      <arg name="namespace" value="$(var namespace)"/>
    </include>
  </group>
</launch>
"#,
    );
    let desc = parse_launch_file(&parent, &index, &[]).expect("parse nav2 smoke");
    assert_eq!(desc.args.len(), 4);
    assert_eq!(desc.groups.len(), 1);
    let g = &desc.groups[0];
    // 7 nodes in the group + 1 include flattening adds nothing to `g.nodes`
    // (the include is in g.includes; the included file's node bubbles to
    // the top-level after the file-recursion merge).
    assert_eq!(g.nodes.len(), 7);
    assert_eq!(g.includes.len(), 1);
}

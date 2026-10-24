//! A `.launch.py` must resolve through the resolver AS SHIPPED — issue 0935.
//!
//! `nros-launch-resolve` is two artifacts: the binary, and
//! `libplay_launch_parser_pyexec.so` beside it, which the binary `dlopen`s
//! against a discovered interpreter. Every test that links the Python half
//! IN-PROCESS proves the mechanism and not the packaging, and that gap is not
//! theoretical: both halves statically link `play_launch_parser`, so each had
//! its own thread-local launch context, and every `.launch.py` ABORTED as
//! shipped while passing in-process.
//!
//! So this invokes the INSTALLED binary by path and asserts a model comes out.
//! `multihost_partition_bake` is the sibling that covers `$(eval …)` from XML;
//! this covers the Python-file half, which nothing did.

use std::process::Command;

/// The smallest launch file that cannot resolve without a working Python half:
/// `generate_launch_description` has to RUN, and the `Node` it returns has to
/// travel back across the boundary as a capture.
const LAUNCH_PY: &str = r#"
from launch import LaunchDescription
from launch_ros.actions import Node


def generate_launch_description():
    return LaunchDescription([
        Node(package="demo_pkg", executable="demo_exe", name="from_python"),
    ])
"#;

#[test]
fn a_python_launch_file_resolves_through_the_shipped_pair() {
    let Some(resolver) = nros_tests::launch_resolver_bin() else {
        nros_tests::unmet!("nros-launch-resolve not built (run `just setup-launch-resolve`)");
    };
    if !nros_tests::host_python_available() {
        // NOT a pass: a host with no interpreter cannot answer this question,
        // and saying "green" would be the vacuous shape issue 0914 warned about.
        nros_tests::unmet!("no usable python3 on this host");
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let launch = tmp.path().join("shipped.launch.py");
    std::fs::write(&launch, LAUNCH_PY).expect("write launch file");
    let model = tmp.path().join("model.yaml");

    let out = Command::new(&resolver)
        .arg(&launch)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");

    assert!(
        out.status.success(),
        "resolving a .launch.py failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let yaml = std::fs::read_to_string(&model).expect("the resolver wrote no model");

    // The node came from Python, so its presence is the whole assertion: it
    // exists only if `generate_launch_description` ran AND its captures crossed
    // back. Before issue 0935 the process aborted here instead.
    assert!(
        yaml.contains("from_python"),
        "the Python-declared node is missing from the model — captures did not \
         cross the boundary:\n{yaml}"
    );
    assert!(
        yaml.contains("demo_pkg") && yaml.contains("demo_exe"),
        "the node crossed without its package/executable:\n{yaml}"
    );
}

/// `--record` is the raw parse the retired standalone `play_launch_parser`
/// binary printed (issue 1413), so it must come out of the SHIPPED resolver
/// too: `-` prints it and builds no model, a path writes it beside the model.
#[test]
fn the_shipped_resolver_prints_the_launch_record() {
    let Some(resolver) = nros_tests::launch_resolver_bin() else {
        nros_tests::unmet!("nros-launch-resolve not built (run `just setup-launch-resolve`)");
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let launch = tmp.path().join("record.launch.xml");
    std::fs::write(
        &launch,
        r#"<launch><node pkg="demo_pkg" exec="demo_exe" name="from_xml"/></launch>"#,
    )
    .expect("write launch file");
    let model = tmp.path().join("model.yaml");

    let out = Command::new(&resolver)
        .arg(&launch)
        .args(["--record", "-", "-o"])
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        out.status.success(),
        "--record - failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let record: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--record - printed no JSON record");
    let names: Vec<&str> = record["node"]
        .as_array()
        .expect("the record has no `node` array")
        .iter()
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert_eq!(names, ["from_xml"], "unexpected record:\n{record:#}");
    assert!(
        !model.exists(),
        "`--record -` must stop before building a model"
    );

    let record_file = tmp.path().join("record.json");
    let out = Command::new(&resolver)
        .arg(&launch)
        .arg("--record")
        .arg(&record_file)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        out.status.success(),
        "--record <path> failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&record_file).expect("no record file"))
            .expect("record file is not JSON");
    assert_eq!(written, record, "the file and stdout forms disagree");
    assert!(
        model.exists(),
        "`--record <path>` must still build the model"
    );
}

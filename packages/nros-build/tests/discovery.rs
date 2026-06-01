use nros_build::discovery::discover;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("pkg_a")
}

#[test]
fn discovers_msg_files_under_package_xml_dir() {
    let pkg = fixture();
    let d = discover(&pkg.join("package.xml")).unwrap();
    let names: Vec<String> = d
        .interface_files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"Sensor.msg".to_string()), "msgs: {names:?}");
    assert!(names.contains(&"Status.msg".to_string()), "msgs: {names:?}");
    assert!(names.contains(&"Ping.srv".to_string()), "srvs: {names:?}");
    assert!(
        names.contains(&"Run.action".to_string()),
        "actions: {names:?}"
    );
}

#[test]
fn parses_build_depends_from_package_xml() {
    let pkg = fixture();
    let d = discover(&pkg.join("package.xml")).unwrap();
    assert!(d.build_depends.contains(&"std_msgs".to_string()));
    assert!(d.build_depends.contains(&"builtin_interfaces".to_string()));
}

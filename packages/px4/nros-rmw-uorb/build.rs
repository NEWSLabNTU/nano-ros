//! Build script: parse `topics.toml` and emit a compile-time `phf::Map`
//! mapping ROS 2 topic strings to uORB topic descriptors.

use serde::Deserialize;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct TopicsFile {
    topic: Vec<TopicEntry>,
}

#[derive(Debug, Deserialize)]
struct TopicEntry {
    ros: String,
    uorb: String,
    #[serde(default)]
    instance: u8,
}

fn main() {
    println!("cargo:rerun-if-changed=topics.toml");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let topics_path = manifest_dir.join("topics.toml");
    let raw = fs::read_to_string(&topics_path).expect("read topics.toml");
    let parsed: TopicsFile = toml::from_str(&raw).expect("parse topics.toml");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let out_path = out_dir.join("topics_generated.rs");
    let mut f = fs::File::create(&out_path).expect("create topics_generated.rs");

    let mut map = phf_codegen::Map::<&str>::new();
    for t in &parsed.topic {
        let value = format!(
            "TopicEntry {{ uorb_name: \"{}\", instance: {} }}",
            t.uorb, t.instance
        );
        map.entry(&t.ros, &value);
    }

    writeln!(
        f,
        "/// Compile-time map: ROS 2 topic name → uORB descriptor."
    )
    .unwrap();
    writeln!(
        f,
        "pub static TOPIC_MAP: phf::Map<&'static str, TopicEntry> = {};",
        map.build()
    )
    .unwrap();
}

//! phase-304 W4-B1 — container hash ORACLE (a Tier-B1 container peer).
//!
//! Runs a `ros:jazzy-ros-base` container as a live oracle: reads the REAL
//! RIHS01 `type_hash` a Jazzy node would put on the wire (from the distro's
//! `share/<pkg>/<kind>/<Name>.json` type-description) and asserts it still
//! equals the committed fixture. Pairs with `edition_type_hash_offline.rs`
//! (engine == fixture) to close the loop engine == fixture == live Jazzy — the
//! keyexpr-tail match that makes cross-edition wire interop work.
//!
//! Docker / the image being absent is a CLEAN SKIP (optional external infra,
//! per the interop philosophy), never a failure. The container is a capture
//! TOOL, not a runtime dependency of the product.

use std::{collections::HashMap, path::PathBuf, process::Command};

const IMAGE: &str = "ros:jazzy-ros-base";
const DISTRO: &str = "jazzy";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/nros-tests/fixtures/ros-editions/jazzy")
}

fn load_fixture(name: &str) -> HashMap<String, String> {
    let body = std::fs::read_to_string(fixtures_dir().join(name)).unwrap_or_default();
    let mut map = HashMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(ty), Some(h)) = (it.next(), it.next()) {
            if h.starts_with("RIHS01_") {
                map.insert(ty.to_string(), h.to_string());
            }
        }
    }
    map
}

/// docker present AND the image already pulled (never auto-pull ~1 GB in a test).
fn oracle_available() -> bool {
    let docker = Command::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !docker {
        return false;
    }
    Command::new("docker")
        .args(["image", "inspect", IMAGE])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Read `<type_name> <hash>` for every requested type from the container's
/// share JSON in ONE container run.
fn container_hashes(types: &[&str]) -> HashMap<String, String> {
    let script = format!(
        r#"source /opt/ros/{DISTRO}/setup.bash
for t in {types}; do
  pkg="${{t%%/*}}"; rest="${{t#*/}}"; kind="${{rest%%/*}}"; name="${{rest##*/}}"
  j="$(ros2 pkg prefix "$pkg" 2>/dev/null)/share/$pkg/$kind/$name.json"
  if [ -f "$j" ]; then
    h="$(python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(next(x['hash_string'] for x in d['type_hashes'] if x['type_name']==sys.argv[2]))" "$j" "$t" 2>/dev/null || echo MISSING)"
    echo "$t $h"
  fi
done"#,
        types = types.join(" "),
    );
    let out = Command::new("docker")
        .args(["run", "--rm", IMAGE, "bash", "-c", &script])
        .output()
        .expect("docker run failed");
    assert!(
        out.status.success(),
        "container oracle run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut map = HashMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut it = line.split_whitespace();
        if let (Some(ty), Some(h)) = (it.next(), it.next()) {
            if h.starts_with("RIHS01_") {
                map.insert(ty.to_string(), h.to_string());
            }
        }
    }
    map
}

#[test]
fn committed_fixtures_match_live_jazzy_container() {
    if !oracle_available() {
        eprintln!(
            "SKIP committed_fixtures_match_live_jazzy_container: docker or {IMAGE} not present \
             (optional Tier-B1 oracle; pull with `docker pull {IMAGE}`)"
        );
        return;
    }

    let mut fx = load_fixture("hashes.txt");
    fx.extend(load_fixture("srv-hashes.txt"));
    // Only types whose json carries a hash (skip AddTwoInts = MISSING).
    let types: Vec<&str> = fx.keys().map(|s| s.as_str()).collect();
    assert!(!types.is_empty(), "no fixture hashes loaded");

    let live = container_hashes(&types);
    assert!(
        !live.is_empty(),
        "container produced no hashes — image/layout drift?"
    );

    let mut checked = 0;
    for (ty, want) in &fx {
        let Some(got) = live.get(ty) else { continue };
        assert_eq!(
            got, want,
            "Jazzy DRIFT for {ty}: fixture={want} live={got} — re-capture the fixtures"
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "oracle matched zero fixture types — container layout changed?"
    );
    eprintln!("oracle: {checked} committed Jazzy hashes still current");
}

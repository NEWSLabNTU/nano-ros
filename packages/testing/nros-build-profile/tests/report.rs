//! Golden reporter tests — fixed `BuildProfile` values render to exact text
//! (phase-251 P3, W3.4). Deterministic widths/rounding make this stable.

use nros_build_profile::model::{Backend, BuildProfile, Kind, Stage, Unit};
use nros_build_profile::report::{self, Opts};

fn unit(name: &str, kind: Kind, dur_s: f64, is_native: bool) -> Unit {
    Unit {
        name: name.to_string(),
        kind,
        dur_s,
        is_native,
    }
}

fn ninja_profile(captured_deep: bool) -> BuildProfile {
    BuildProfile {
        backend: Backend::Ninja,
        total_s: 21.9,
        stages: vec![
            Stage {
                name: "compile",
                dur_s: 20.3,
                pct: 92.694,
            },
            Stage {
                name: "link",
                dur_s: 1.6,
                pct: 7.306,
            },
        ],
        units: vec![
            unit("zenoh_pico_net.c.o", Kind::Compile, 18.1, true),
            unit("main.c.o", Kind::Compile, 1.1, true),
            unit("talker.c.o", Kind::Compile, 1.1, true),
            unit("libzenoh_pico.a", Kind::Link, 0.8, true),
            unit("zephyr.elf", Kind::Link, 0.8, true),
        ],
        captured_deep,
        notes: vec![],
    }
}

#[test]
fn coarse_table_golden() {
    let p = ninja_profile(true);
    let opts = Opts {
        deep: false,
        top_n: 8,
        hints: false,
    };
    let expected = "\
Backend: ninja            Total: 21.9s

Stage      Duration    %
compile       20.3s  93%
link           1.6s   7%
";
    assert_eq!(report::render(&p, &[], opts), expected);
}

#[test]
fn deep_drilldown_golden() {
    let p = ninja_profile(true);
    let opts = Opts {
        deep: true,
        top_n: 3,
        hints: false,
    };
    let out = report::render(&p, &[], opts);
    assert!(out.contains("slowest units:\n"));
    assert!(out.contains("zenoh_pico_net.c.o   18.1s ########"), "{out}");
    // top_n = 3 → 2 units remain folded into a "<2 more>" line.
    assert!(out.contains("<2 more>"), "{out}");
}

#[test]
fn deep_without_captured_data_emits_note() {
    let p = ninja_profile(false);
    let opts = Opts {
        deep: true,
        top_n: 8,
        hints: false,
    };
    let out = report::render(&p, &[], opts);
    assert!(out.contains("no per-unit timing captured"), "{out}");
    assert!(out.contains("cargo build --timings"), "{out}");
}

#[test]
fn hints_render_when_present() {
    let p = ninja_profile(true);
    let opts = Opts {
        deep: false,
        top_n: 8,
        hints: true,
    };
    let out = report::render(&p, &["do the thing".to_string()], opts);
    assert!(out.ends_with("hints:\n  - do the thing\n"), "{out}");
}

#[test]
fn json_carries_backend_stages_and_hints() {
    let p = ninja_profile(true);
    let json = report::to_json(&p, &["hint one".to_string()]);
    assert!(json.contains("\"backend\": \"ninja\""));
    assert!(json.contains("\"name\": \"compile\""));
    assert!(json.contains("\"hints\""));
    assert!(json.contains("hint one"));
    // round-trips as valid JSON.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["total_s"], 21.9);
}

//! Collector parser tests against checked-in fixture artifacts.
//! No build is run — the fixtures are hand-authored to mirror real
//! `.ninja_log` / cargo-timings output (phase-251 P1, W1.4).

use nros_build_profile::{
    collect::{cargo, ninja},
    model::Kind,
};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn ninja_log_parses_units_and_skips_malformed() {
    let c = ninja::parse(&fixture("sample.ninja_log"));

    // 5 valid rows; 2 malformed (no-tabs row + end<start row) skipped.
    assert_eq!(c.units.len(), 5, "valid row count");
    assert!(c.deep);
    assert!(
        c.notes.iter().any(|n| n.contains("skipped 2")),
        "expected skip note, got {:?}",
        c.notes
    );

    let big = c
        .units
        .iter()
        .find(|u| u.name == "zenoh_pico_net.c.o")
        .expect("big compile unit present");
    assert_eq!(big.kind, Kind::Compile);
    assert!((big.dur_s - 18.1).abs() < 1e-6, "dur {}", big.dur_s);
    assert!(big.is_native);

    let elf = c.units.iter().find(|u| u.name == "zephyr.elf").unwrap();
    assert_eq!(elf.kind, Kind::Link);

    let archive = c
        .units
        .iter()
        .find(|u| u.name == "libzenoh_pico.a")
        .unwrap();
    assert_eq!(archive.kind, Kind::Link);

    // Compile-stage sum: 18.1 + 1.1 + 1.1 = 20.3
    let compile: f64 = c
        .units
        .iter()
        .filter(|u| u.kind == Kind::Compile)
        .map(|u| u.dur_s)
        .sum();
    assert!((compile - 20.3).abs() < 1e-6, "compile sum {compile}");
}

#[test]
fn ninja_collapses_multi_output_edges() {
    // One edge (same start/end/cmdhash) emits 3 outputs — a corrosion cargo
    // build's .a + stamp + generated header. It must count ONCE (20.1s), not 3×,
    // and be attributed to compile (the Rust build), not link.
    let c = ninja::parse(&fixture("multi_output_edge.ninja_log"));
    assert_eq!(c.units.len(), 2, "2 edges, not 4 output lines");

    let rust = c
        .units
        .iter()
        .find(|u| u.kind == Kind::Compile)
        .expect("rust-build edge attributed to compile");
    assert!(
        (rust.dur_s - 20.1).abs() < 1e-6,
        "counted once: {}",
        rust.dur_s
    );
    assert_eq!(rust.name, "nros_c_cargo_build");

    let elf = c.units.iter().find(|u| u.name == "zephyr.elf").unwrap();
    assert_eq!(elf.kind, Kind::Link);
}

#[test]
fn ninja_classifies_untokenized_cargo_output_as_compile() {
    // Issue #73: a cargo edge whose output stamp has no cargo[-_]build token —
    // nuttx's `cargo-target/<triple>/release/nros-nuttx-ffi`. Must land in
    // compile (the Rust FFI build), not "other".
    let text = "# ninja log v6\n\
        0\t31436\t0\tcargo-target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi\tabc\n\
        0\t1000\t0\tfoo/x86_64-unknown-linux-gnu/release/some_bin\tdef\n\
        0\t500\t0\tbuild/release/plain_c_app\tghi\n";
    let c = ninja::parse(text);

    let ffi = c.units.iter().find(|u| u.name == "nros-nuttx-ffi").unwrap();
    assert_eq!(ffi.kind, Kind::Compile, "cargo-target path → compile");
    assert!((ffi.dur_s - 31.436).abs() < 1e-6);

    // triple/release binary also recognised
    let triple = c.units.iter().find(|u| u.name == "some_bin").unwrap();
    assert_eq!(triple.kind, Kind::Compile);

    // but a plain build/release/app (no triple dir) is NOT misread as cargo
    let plain = c.units.iter().find(|u| u.name == "plain_c_app").unwrap();
    assert_eq!(
        plain.kind,
        Kind::Other,
        "no false positive on non-triple release"
    );
}

#[test]
fn cargo_timings_parses_unit_data() {
    let c = cargo::parse(&fixture("cargo-timing.html"));

    // 5 records; one is a build-script (codegen), rest compile.
    assert_eq!(c.units.len(), 5);
    assert!(c.deep);

    let codegen: Vec<_> = c.units.iter().filter(|u| u.kind == Kind::Codegen).collect();
    assert_eq!(codegen.len(), 1, "one run-custom-build → codegen");
    assert_eq!(codegen[0].name, "talker");

    let sys = c.units.iter().find(|u| u.name == "zenoh-pico-sys").unwrap();
    assert_eq!(sys.kind, Kind::Compile);
    assert!(sys.is_native, "-sys crate flagged native");
    assert!((sys.dur_s - 18.1).abs() < 1e-6);

    // The bracket-in-string name must survive the depth scanner intact.
    assert!(
        c.units.iter().any(|u| u.name == "weird[name]-sys"),
        "bracket-in-string name parsed"
    );
}

#[test]
fn cargo_missing_unit_data_is_a_note_not_a_panic() {
    let c = cargo::parse("<html><body>no data here</body></html>");
    assert!(c.units.is_empty());
    assert!(!c.deep);
    assert!(c.notes.iter().any(|n| n.contains("UNIT_DATA")));
}

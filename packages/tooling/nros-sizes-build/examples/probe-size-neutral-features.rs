//! Issue 2840 -- `just check probe-size-neutral-features`: is every feature in
//! `nros_sizes_build::SIZE_NEUTRAL_FEATURES` still size-neutral?
//!
//! The sizes probe drops those features from its key and from its nested
//! build, so two images that differ only by one share a probe directory. That
//! is safe only while the feature changes no `__NROS_SIZE_*` answer. This
//! builds the probed crate (`nros`) the way the probe does
//! (`NROS_SIZES_PROBE_BUILD=1`, `--no-default-features --features ...`), with
//! and without each neutral feature, and compares the markers through the
//! probe's own reader (`extract_sizes`). Any difference fails the gate, and the
//! feature must go back into the key.
//!
//! What it covers:
//! - the seven base feature sets that held a `profile-mode` twin in host 142's
//!   tier-1 probe tree (2026-11-25), on this host's triple;
//! - one embedded set on `thumbv7m-none-eabi`, so a size that depends on
//!   pointer width or alignment cannot hide behind a 64-bit host.
//!
//!   cargo run -p nros-sizes-build --example probe-size-neutral-features -- <target-dir>

use std::{collections::HashMap, path::Path, process::ExitCode};

use nros_sizes_build::{SIZE_NEUTRAL_FEATURES, extract_sizes, nested_cargo};

/// Host base sets: each held a `profile-mode` twin in the measured tree.
const HOST_SETS: &[&str] = &[
    "alloc,env,ffi-size-markers,log,metadata-mode,nros-rmw/buffered-dispatch,nros-rmw/in-place-dispatch,param-services,param-store,rmw-cffi,ros-humble,rosout,sim-time",
    "alloc,env,ffi-size-markers,lifecycle-services,log,metadata-mode,nros-rmw/buffered-dispatch,nros-rmw/in-place-dispatch,param-services,param-store,rmw-cffi,ros-humble,rosout,sim-time",
    "alloc,env,ffi-size-markers,log,metadata-mode,nros-rmw/buffered-dispatch,param-services,param-store,rmw-cffi,ros-humble,rosout,sim-time",
    "alloc,env,ffi-size-markers,log,metadata-mode,nros-rmw/buffered-dispatch,nros-rmw/in-place-dispatch,param-store,rmw-cffi,ros-humble",
    "alloc,env,ffi-size-markers,log,metadata-mode,nros-rmw/buffered-dispatch,nros-rmw/in-place-dispatch,param-services,param-store,rmw-cffi,ros-humble,rosout,safety-e2e,sim-time",
    "alloc,env,ffi-size-markers,lifecycle-services,log,metadata-mode,nros-rmw/buffered-dispatch,param-services,param-store,rmw-cffi,ros-humble,rosout,sim-time",
    "alloc,env,ffi-size-markers,log,metadata-mode,nros-rmw/buffered-dispatch,nros-rmw/in-place-dispatch,param-store,rmw-cffi,ros-humble,rosout",
];

/// A 32-bit embedded set (the rpi-pico-w-zephyr probe's, on thumbv7m).
const EMBEDDED_TARGET: &str = "thumbv7m-none-eabi";
const EMBEDDED_SET: &str = "alloc,ffi-size-markers,nros-rmw/in-place-dispatch,param-services,param-store,rmw-cffi,ros-humble,rosout,sim-time";

fn host_triple() -> String {
    let out = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("rustc -vV");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .expect("rustc -vV names the host")
}

/// Build `nros` for `target` with `features` into `dir`; its size markers.
fn sizes(dir: &Path, target: &str, features: &str) -> Result<HashMap<String, u64>, String> {
    let out = nested_cargo()
        .args([
            "build",
            "-q",
            "--locked",
            "-p",
            "nros",
            "--lib",
            "--no-default-features",
        ])
        .args(["--target", target, "--features", features])
        .arg("--target-dir")
        .arg(dir)
        .env("NROS_SIZES_PROBE_BUILD", "1")
        .output()
        .map_err(|e| format!("cargo: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo build failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let rlib = dir.join(target).join("debug").join("libnros.rlib");
    let got =
        extract_sizes(&rlib, "__NROS_SIZE_").map_err(|e| format!("{}: {e}", rlib.display()))?;
    if got.is_empty() {
        return Err(format!("{}: no __NROS_SIZE_ markers", rlib.display()));
    }
    Ok(got)
}

fn main() -> ExitCode {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: probe-size-neutral-features <target-dir>");
        return ExitCode::from(2);
    };
    let root = Path::new(&root);
    let host = host_triple();
    let mut cases: Vec<(String, &str)> = HOST_SETS.iter().map(|s| (host.clone(), *s)).collect();
    cases.push((EMBEDDED_TARGET.to_string(), EMBEDDED_SET));

    let mut failed = 0;
    for (i, (target, base)) in cases.iter().enumerate() {
        // One target dir per TRIPLE, shared by every case and variant: they
        // differ by features, which cargo keeps as separate units, so the
        // dependencies no feature reaches build once (3.0 GiB as one dir per
        // case, measured on host 118).
        let dir = root.join(target);
        for (feature, _) in SIZE_NEUTRAL_FEATURES {
            // The pair differs by exactly this feature, whether or not the
            // base set already names it.
            let without: Vec<&str> = base.split(',').filter(|f| f != feature).collect();
            let without = without.join(",");
            let with = format!("{without},{feature}");
            let plain = match sizes(&dir, target, &without) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "probe-size-neutral-features: case {i} ({target}) without {feature}: {e}"
                    );
                    failed += 1;
                    continue;
                }
            };
            match sizes(&dir, target, &with) {
                Ok(s) if s == plain => {
                    println!(
                        "probe-size-neutral-features: case {i} {target} +{feature}: {} markers identical",
                        s.len()
                    );
                }
                Ok(s) => {
                    failed += 1;
                    eprintln!(
                        "probe-size-neutral-features: FAILED case {i} {target} +{feature} changes a probed size:"
                    );
                    let mut names: Vec<&String> = plain.keys().chain(s.keys()).collect();
                    names.sort();
                    names.dedup();
                    for n in names {
                        let (a, b) = (plain.get(n), s.get(n));
                        if a != b {
                            eprintln!("    {n}: {a:?} -> {b:?}");
                        }
                    }
                    eprintln!("    base features: {without}");
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("probe-size-neutral-features: case {i} {target} +{feature}: {e}");
                }
            }
        }
    }
    if failed > 0 {
        eprintln!(
            "\n  A feature in SIZE_NEUTRAL_FEATURES (nros-sizes-build) changed a size or could\n  \
             not be measured. Remove it from that list so it keys the probe again\n  \
             (a shared probe would hand one image another image's sizes)."
        );
        return ExitCode::FAILURE;
    }
    println!(
        "probe-size-neutral-features: OK ({} case(s) x {} feature(s))",
        cases.len(),
        SIZE_NEUTRAL_FEATURES.len()
    );
    ExitCode::SUCCESS
}

//! Issue 1284 — a STATED executor backing must MEET the executor's default,
//! measured, in a lane that gates merges.
//!
//! A Zephyr image that lowers `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` states the
//! reservation it pays for, `CONFIG_NROS_EXECUTOR_BACKING_U64S=<words>` (issues
//! 1145/1171). `check-executor-backing-arena-pairing` holds `arena + 8 * words
//! == base` — that the numbers SUM. Whether `words` is ENOUGH is
//! `executor::backing`'s const assertion, a compile error, and no merge-gating
//! lane builds a Zephyr Rust image: twelve leaves stayed exactly paired at
//! 11041 and then 11045 while the default grew to 11065 and then 11069 under
//! them, each drift found by a build days later.
//!
//! This test is the host half of that claim check. The conf side — which confs
//! claim, for which boards, and which of them nothing can vouch for — has ONE
//! home, the pairing gate's `--claims` output, and this reads it rather than
//! re-parsing a conf. For each claim on a board of THIS host's pointer width it
//! compares `words` against [`ExecutorSizing::DEFAULT`] as compiled here, and
//! names the conf and both numbers when it falls short.
//!
//! It REFUSES — fails with a reason, never skips — whatever it cannot vouch for:
//!
//! * a claim the gate itself refused (no fixture row, an unknown board, a
//!   merged fragment that sets an executor-sizing knob);
//! * a claim on a board of another pointer width, unless the lane has already
//!   compiled `nros-node` for that board's own target with the stated value
//!   and says so in `NROS_BACKING_CROSS_VERIFIED`;
//! * a sizing knob or a knob-resolution rung (`DOTCONFIG`, the platform
//!   descriptor) in this process's environment — the one `cargo` handed the
//!   build that computed `DEFAULT` — because then `DEFAULT` is not the image's;
//! * a `zephyr/Kconfig` default for a sizing knob that is neither a derive
//!   sentinel nor the value this build resolved, for the same reason.
//!
//! `#[ignore]`d and run by `just check node-std-tests` (pull_request AND
//! merge_group), which does the cross-width compile first. A bare
//! `cargo test -- --ignored` without that step refuses the cross-width claims,
//! which is the correct answer for a run that did not measure them.
//!
//! Built with `std,rmw-cffi` because the default's layout depends on the RMW
//! seam (`ConcreteSession` is `CffiSession` only under `rmw-cffi`) and on
//! `alloc`; a Zephyr Rust image reaches `nros-node` through `nros`'s
//! `alloc,rmw-cffi`.
#![cfg(all(feature = "std", feature = "rmw-cffi"))]

use std::{path::PathBuf, process::Command};

use nros_node::ExecutorSizing;

const SCRIPT: &str = "scripts/check-executor-backing-arena-pairing.py";
const CROSS_VERIFIED_ENV: &str = "NROS_BACKING_CROSS_VERIFIED";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("nros-node sits three levels below the repository root")
}

/// What this build resolved for a sizing knob whose `zephyr/Kconfig` default is
/// a literal rather than a derive sentinel. `None`: not exposed as a const, so a
/// literal Kconfig default for it cannot be compared and is refused.
fn resolved_here(knob: &str) -> Option<usize> {
    use nros_node::config;
    match knob {
        "NROS_EXECUTOR_MAX_CBS" => Some(config::MAX_CBS),
        "NROS_EXECUTOR_MAX_SC" => Some(config::MAX_SC),
        "NROS_EXECUTOR_MAX_NODES" => Some(config::MAX_NODES),
        "NROS_EXECUTOR_ARENA_SIZE" => Some(config::ARENA_SIZE),
        "NROS_EXECUTOR_ACTION_CLIENTS" => Some(config::ARENA_ACTION_CLIENTS),
        "NROS_SUBSCRIPTION_BUFFER_SIZE" => Some(config::DEFAULT_RX_BUF_SIZE),
        "NROS_PUBSUB_QOS_DEPTH" => Some(config::arena_model::BUDGETED_QOS_DEPTH as usize),
        _ => None,
    }
}

/// A Kconfig default `build.rs` does not take literally: a negative value does
/// not parse as `usize` and falls through to the next rung (`-1 = derive`), and
/// `NROS_EXECUTOR_ARENA_SIZE=0` is that knob's own derive sentinel.
fn is_derive_sentinel(knob: &str, default: i64) -> bool {
    default < 0 || (knob == "NROS_EXECUTOR_ARENA_SIZE" && default == 0)
}

#[test]
#[ignore = "run by `just check node-std-tests`, which compiles the cross-width \
            claims first; without that step they are refused, not skipped"]
fn every_stated_executor_backing_meets_the_measured_default() {
    let root = repo_root();
    let out = Command::new("python3")
        .arg(root.join(SCRIPT))
        .arg("--claims")
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("could not run python3 {SCRIPT} --claims: {e}"));
    let listing = String::from_utf8_lossy(&out.stdout);

    let default = ExecutorSizing::DEFAULT.u64_len();
    let host_width = usize::BITS as usize;
    let cross_verified: Vec<String> = std::env::var(CROSS_VERIFIED_ENV)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let mut failures = Vec::new();
    let mut scanned: Option<usize> = None;
    let (mut met, mut by_compile) = (0usize, 0usize);

    for line in listing.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["scanned", n] => scanned = n.parse().ok(),
            ["refuse", what, why] => failures.push(format!("{what}: {why}")),
            ["knob" | "rung", name] => {
                if let Some(v) = std::env::var_os(name) {
                    failures.push(format!(
                        "`{name}={}` is in this environment, so the nros-node build \
                         that computed DEFAULT = {default} words saw it: that is not \
                         the default of an image that sets nothing. Unset it.",
                        v.to_string_lossy()
                    ));
                }
            }
            ["kconfig", knob, value] => {
                let value: i64 = value.parse().expect("the gate prints integers");
                if is_derive_sentinel(knob, value) {
                    continue;
                }
                match resolved_here(knob) {
                    Some(here) if here as i64 == value => {}
                    Some(here) => failures.push(format!(
                        "zephyr/Kconfig defaults {knob} to {value}, but this build \
                         resolved {here}: every Zephyr image gets {value}, so the \
                         host DEFAULT is not theirs"
                    )),
                    None => failures.push(format!(
                        "zephyr/Kconfig defaults {knob} to the literal {value}, and \
                         this test cannot see what the host build resolved for it, \
                         so it cannot vouch that the host DEFAULT is an image's"
                    )),
                }
            }
            ["claim", conf, words, board, width, cross] => {
                let words: usize = words.parse().expect("the gate prints integers");
                let width: usize = width.parse().expect("the gate prints integers");
                if width == host_width {
                    if words >= default {
                        met += 1;
                    } else {
                        failures.push(format!(
                            "{conf} states CONFIG_NROS_EXECUTOR_BACKING_U64S={words} for \
                             `{board}`, but the executor's default measured on this \
                             {host_width}-bit host is {default} words ({} short). The image \
                             will not compile. Restate it as {default} and re-pair the arena \
                             (`arena = nros-arena-base - 8 * {default}`).",
                            default - words
                        ));
                    }
                } else if *cross != "-" && cross_verified.iter().any(|t| t == cross) {
                    by_compile += 1;
                } else {
                    failures.push(format!(
                        "{conf} states {words} words for `{board}` ({width}-bit), and \
                         this host is {host_width}-bit, so its DEFAULT = {default} is not \
                         that board's. Nothing measured it: `{cross}` is not in \
                         {CROSS_VERIFIED_ENV}. Run `just check node-std-tests`, which \
                         compiles nros-node for that target with the stated value first."
                    ));
                }
            }
            _ => failures.push(format!("unparseable claims line: {line:?}")),
        }
    }

    assert!(
        out.status.success() || !failures.is_empty(),
        "{SCRIPT} --claims failed with no refusal to show:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        scanned.is_some_and(|n| n > 0),
        "{SCRIPT} --claims scanned no conf at all — the listing is empty, which is \
         what a broken listing also prints:\n{listing}"
    );
    assert!(
        failures.is_empty(),
        "issue 1284 — {} stated executor backing claim(s) fail or cannot be vouched \
         for (DEFAULT = {default} words, {host_width}-bit host):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
    println!(
        "executor backing claims: {met} met DEFAULT = {default} words on this \
         {host_width}-bit host; {by_compile} verified by the cross-width compile"
    );
}

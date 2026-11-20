//! phase-569 W4 (RFC-0133 D7) — completion through the real shell glue.
//!
//! bash sources what `COMPLETE=bash nros` prints, exactly as the line
//! `nros completions bash` prints does, then runs the completion function it
//! registered the way bash runs it on a `<Tab>`. So the test covers the glue,
//! the binary's completion engine and the completer behind the argument.
#![cfg(unix)]

use std::{path::PathBuf, process::Command};

fn nros() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nros"))
}

/// The SDK root this test's binary reads its board catalog from.
fn sdk_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// What bash offers for `words` with the cursor on the last one.
fn bash_complete(words: &[&str]) -> Vec<String> {
    let bin = nros();
    let dir = bin.parent().unwrap();
    let quoted: Vec<String> = words.iter().map(|w| format!("'{w}'")).collect();
    let script = format!(
        "source <(COMPLETE=bash nros)\n\
         COMP_WORDS=({words}); COMP_CWORD={cword}; COMP_TYPE=9\n\
         _clap_complete_nros nros '{cur}' '{prev}'\n\
         printf '%s\\n' \"${{COMPREPLY[@]}}\"\n",
        words = quoted.join(" "),
        cword = words.len() - 1,
        cur = words[words.len() - 1],
        prev = words[words.len() - 2],
    );
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .env(
            "PATH",
            format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
        )
        .env("NROS_WORKSPACE_ROOT", sdk_root())
        .output()
        .expect("bash runs");
    assert!(
        out.status.success(),
        "bash failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn bash_completes_board_show_with_board_keys() {
    let got = bash_complete(&["nros", "board", "show", ""]);
    assert!(
        got.iter().any(|c| c == "mps2-an385-freertos"),
        "no board key offered: {got:?}"
    );
}

#[test]
fn bash_completes_a_prefix_to_the_names_it_starts() {
    let got = bash_complete(&["nros", "rmw", "show", "zen"]);
    assert_eq!(got, ["zenoh"], "offered: {got:?}");
}

#[test]
fn the_registration_line_is_the_one_bash_sources() {
    let out = Command::new(nros())
        .args(["completions", "bash"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.lines().any(|l| l == "source <(COMPLETE=bash nros)"),
        "{text}"
    );
}

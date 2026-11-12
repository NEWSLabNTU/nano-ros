//! Issue 2400 W5 — the stack a board's build STATES, for the cargo road's check.
//!
//! On the cargo road `nros build` walks the linked image's deepest path after
//! cargo and compares its floor with the stack the image was built with. Only
//! the board's build script knows that number (it resolves the knob's whole
//! ladder), so it writes it here, into its own `OUT_DIR`. `nros build` finds
//! the file through cargo's `build-script-executed` message for the exact unit
//! the image linked, so a sibling build of another feature set is never read.
//!
//! A board that writes no file states no stack, and its image is reported as
//! "not checked", never as a pass.

use std::{fmt::Write as _, path::Path};

/// The file's name in a build script's `OUT_DIR`. `nros build` reads this
/// name; keep the two in step.
pub const FILE: &str = "nros-stack-stated.json";

/// One thread's stated stack.
pub struct Stated<'a> {
    /// The walker's role (`OPEN`: registration and `Executor::open`).
    pub role: &'a str,
    /// The thread entry: a symbol, or `~text` for every function whose
    /// symbol contains `text` (a generic Rust entry).
    pub root: &'a str,
    /// The stack the image was built with, in bytes.
    pub bytes: u32,
    /// What a user sets to raise it; `{bytes}` / `{kb}` stand for the floor
    /// in that knob's unit.
    pub knob: &'a str,
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The file's text: `{"schema":1,"stacks":[{role,root,bytes,knob},...]}`.
pub fn render(stacks: &[Stated<'_>]) -> String {
    let rows: Vec<String> = stacks
        .iter()
        .map(|s| {
            format!(
                "{{\"role\":{},\"root\":{},\"bytes\":{},\"knob\":{}}}",
                json_str(s.role),
                json_str(s.root),
                s.bytes,
                json_str(s.knob)
            )
        })
        .collect();
    format!("{{\"schema\":1,\"stacks\":[{}]}}\n", rows.join(","))
}

/// Write [`FILE`] into `out_dir`.
///
/// # Panics
/// When the file cannot be written: a build script has no other way to fail.
pub fn emit(out_dir: &Path, stacks: &[Stated<'_>]) {
    let path = out_dir.join(FILE);
    std::fs::write(&path, render(stacks))
        .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stated_stack_renders_as_json_the_cli_reads() {
        let t = render(&[Stated {
            role: "OPEN",
            root: "~app_task_entry_",
            bytes: 131_072,
            knob: "NROS_FREERTOS_APP_STACK_KB={kb}",
        }]);
        assert_eq!(
            t,
            "{\"schema\":1,\"stacks\":[{\"role\":\"OPEN\",\"root\":\"~app_task_entry_\",\
             \"bytes\":131072,\"knob\":\"NROS_FREERTOS_APP_STACK_KB={kb}\"}]}\n"
        );
    }
}

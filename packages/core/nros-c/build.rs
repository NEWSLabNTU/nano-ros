//! Build script for nros-c
//!
//! 1. Reads NROS_* environment variables and generates `nros_c_config.rs`
//!    with compile-time configurable constants for the executor.
//! 2. Runs cbindgen to generate `include/nros/nros_generated.h` from
//!    Rust `#[repr(C)]` types, preventing C/Rust struct layout drift.
//! 3. Post-processes the generated header: converts Rust Markdown doc
//!    comments (`# Parameters`, `# Returns`, `# Safety`) into Doxygen
//!    tags (`@param`, `@retval`, `@pre`) so C users get structured docs.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    generate_config(&out_dir);
    generate_header(&manifest_dir);

    // Re-run if source files change (for library rebuild + header regen)
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

/// Generate `nros_c_config.rs` with build-time configurable constants.
fn generate_config(out_dir: &str) {
    let executor_max_handles = env_usize("NROS_EXECUTOR_MAX_HANDLES", 16);
    let max_subscriptions = env_usize("NROS_MAX_SUBSCRIPTIONS", 8);
    let max_timers = env_usize("NROS_MAX_TIMERS", 8);
    let max_services = env_usize("NROS_MAX_SERVICES", 4);
    let let_buffer_size = env_usize("NROS_LET_BUFFER_SIZE", 512);
    let message_buffer_size = env_usize("NROS_MESSAGE_BUFFER_SIZE", 4096);

    let contents = format!(
        "/// Maximum number of handles in an executor \
         (set via NROS_EXECUTOR_MAX_HANDLES, default 16).\n\
         pub const NROS_EXECUTOR_MAX_HANDLES: usize = {executor_max_handles};\n\
         \n\
         /// Maximum number of subscriptions in an executor \
         (set via NROS_MAX_SUBSCRIPTIONS, default 8).\n\
         pub const NROS_MAX_SUBSCRIPTIONS: usize = {max_subscriptions};\n\
         \n\
         /// Maximum number of timers in an executor \
         (set via NROS_MAX_TIMERS, default 8).\n\
         pub const NROS_MAX_TIMERS: usize = {max_timers};\n\
         \n\
         /// Maximum number of services in an executor \
         (set via NROS_MAX_SERVICES, default 4).\n\
         pub const NROS_MAX_SERVICES: usize = {max_services};\n\
         \n\
         /// Buffer size for LET semantics per handle \
         (set via NROS_LET_BUFFER_SIZE, default 512).\n\
         pub const LET_BUFFER_SIZE: usize = {let_buffer_size};\n\
         \n\
         /// Maximum buffer size for subscription/service data \
         (set via NROS_MESSAGE_BUFFER_SIZE, default 4096).\n\
         pub const MESSAGE_BUFFER_SIZE: usize = {message_buffer_size};\n"
    );

    std::fs::write(Path::new(out_dir).join("nros_c_config.rs"), contents).unwrap();
}

/// Generate `include/nros/nros_generated.h` using cbindgen.
///
/// cbindgen reads Rust source files and generates C header declarations
/// for all `#[repr(C)]` structs, enums, type aliases, constants, and
/// `extern "C"` functions. The generated header is the single source of
/// truth for C/Rust type layout compatibility.
fn generate_header(manifest_dir: &Path) {
    let config_path = manifest_dir.join("cbindgen.toml");
    let output_path = manifest_dir.join("include/nros/nros_generated.h");

    let config = match cbindgen::Config::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            println!("cargo:warning=Failed to load cbindgen config: {e}");
            return;
        }
    };

    let result = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_config(config)
        .generate();

    match result {
        Ok(bindings) => {
            bindings.write_to_file(&output_path);
            doxygen_postprocess(&output_path);
        }
        Err(e) => {
            // cbindgen may fail if dependencies aren't available (e.g.,
            // during no-default-features builds). This is expected —
            // the generated header is only needed for builds with an
            // RMW backend enabled.
            println!("cargo:warning=cbindgen header generation skipped: {e}");
        }
    }
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Doxygen post-processor
// ---------------------------------------------------------------------------

/// Which doc-comment section the state machine is currently inside.
#[derive(PartialEq)]
enum DocSection {
    None,
    Params,
    Returns,
    Safety,
}

/// Post-process cbindgen output: convert Rust Markdown doc comments to Doxygen.
///
/// Runs a single-pass line-by-line state machine over the generated header:
///
///   `# Parameters` + `` * `name` - desc ``   →  `@param name desc`
///   `# Returns`    + `` * `NROS_RET_*` … ``  →  `@retval NROS_RET_* …`
///   `# Returns`    + `* plain text`          →  `@return plain text`
///   `# Safety`     + `* text`                →  `@pre text.`
///
/// Also performs global Rust-ism replacements (usize::MAX → SIZE_MAX, etc.).
fn doxygen_postprocess(path: &Path) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut out = String::with_capacity(content.len());
    let mut section = DocSection::None;
    let mut prev_blank = false; // tracks whether the last emitted line was " *"
    let mut added_file_tag = false; // inject @file once in the first doc block

    for line in content.lines() {
        // Inject @file into the first doc comment so Doxygen generates
        // a "File Members" page listing all global functions/defines.
        if !added_file_tag
            && line.trim()
                == "* nros C API — Auto-generated type definitions and function declarations"
        {
            push_line(&mut out, " * @file");
            push_line(
                &mut out,
                " * @brief nros C API — types and function declarations",
            );
            added_file_tag = true;
            continue;
        }
        // Detect section headers: " * # Parameters", " * # Returns", " * # Safety"
        if let Some(heading) = line.strip_prefix(" * # ") {
            let lower = heading.trim().to_ascii_lowercase();
            if lower == "parameters" {
                section = DocSection::Params;
                if !prev_blank {
                    push_line(&mut out, " *");
                    prev_blank = true;
                }
                continue;
            } else if lower.starts_with("return") {
                section = DocSection::Returns;
                if !prev_blank {
                    push_line(&mut out, " *");
                    prev_blank = true;
                }
                continue;
            } else if lower == "safety" {
                section = DocSection::Safety;
                if !prev_blank {
                    push_line(&mut out, " *");
                    prev_blank = true;
                }
                continue;
            } else {
                section = DocSection::None;
                // fall through — keep unknown headings as-is
            }
        }

        // A blank doc line (" *") ends the current section
        if line.trim_end() == " *" && section != DocSection::None {
            section = DocSection::None;
            // fall through — emit the blank line normally
        }

        // Transform bullets according to current section
        if section == DocSection::Params {
            //  " * * `name` - description"  →  " * @param name description"
            if let Some(rest) = line.strip_prefix(" * * `")
                && let Some((name, desc)) = rest.split_once("` - ")
            {
                push_line(&mut out, &format!(" * @param {name} {desc}"));
                prev_blank = false;
                continue;
            }
        } else if section == DocSection::Returns {
            //  " * * `NROS_RET_OK` on success"  →  " * @retval NROS_RET_OK on success"
            //  " * * `true` if …"               →  " * @return true if …"
            //  " * * Non-zero if …"             →  " * @return Non-zero if …"
            if let Some(rest) = line.strip_prefix(" * * `")
                && let Some((val, desc)) = rest.split_once("` ")
            {
                if val.starts_with("NROS_RET_") {
                    push_line(&mut out, &format!(" * @retval {val} {desc}"));
                } else {
                    push_line(&mut out, &format!(" * @return {val} {desc}"));
                }
                prev_blank = false;
                continue;
            } else if let Some(rest) = line.strip_prefix(" * * ") {
                push_line(&mut out, &format!(" * @return {rest}"));
                prev_blank = false;
                continue;
            }
        } else if section == DocSection::Safety {
            //  " * * All pointers must be valid"  →  " * @pre All pointers must be valid."
            if let Some(rest) = line.strip_prefix(" * * ") {
                let clean = rest.replace('`', "").replace("usize", "size_t");
                if clean.ends_with('.') {
                    push_line(&mut out, &format!(" * @pre {clean}"));
                } else {
                    push_line(&mut out, &format!(" * @pre {clean}."));
                }
                prev_blank = false;
                continue;
            }
        }

        // Global Rust-ism replacements
        let line = line
            .replace("usize::MAX", "SIZE_MAX")
            .replace(" (`Box<CExecutor>`)", "")
            .replace(" (`Box<ActionServerInternal>`)", "")
            .replace("`nros_node::Executor`", "the internal executor")
            .replace("nros_node::Executor", "the internal executor")
            .replace(
                "`add_action_server_raw_sized()`",
                "add_action_server_raw_sized()",
            )
            .replace("`spin_once()`", "spin_once()");

        prev_blank = line.trim_end() == " *";
        push_line(&mut out, &line);
    }

    let _ = std::fs::write(path, out);
}

/// Append a line (with newline) to the output buffer.
fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

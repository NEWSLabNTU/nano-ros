//! Phase 172.E (driver) — metadata-mode build + run.
//!
//! Produces a component's `source-metadata.json` by compiling a tiny **host**
//! harness that links the component crate, runs its `Component::register`
//! against the in-memory `MetadataRecorder` (no transport, no RTOS task), and
//! serializes the recorder via `to_source_metadata_json`. This is the "compile
//! each component in a host-side metadata mode and invoke its entry path with a
//! fake `ComponentContext`" step from the workflow design — the input `nros
//! metadata` collects + the planner consumes.
//!
//! Scope (chosen 2026-05-28): the **driver** only. Hardening this execution
//! (resource limits, fs/network sandbox for untrusted component crates) is the
//! deferred 172.E sandbox; it wraps the `cargo` invocation here when it lands.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use eyre::{Result, WrapErr, bail, eyre};

#[derive(Debug, Clone)]
pub struct MetadataBuildOptions {
    /// Component id (e.g. `demo_pkg::talker`, or a bare `talker` for the
    /// Cargo `[package.metadata.nros.node]` shape). Diagnostics + probe dir
    /// naming; the Rust identity comes from `class` / `crate_name`.
    pub component_id: String,
    /// phase-307 W1 — the registered type's fully qualified path, verbatim
    /// from the manifest's `class`. `None` keeps the legacy
    /// `<crate>::<module>::Component` derivation from `component_id`.
    pub class: Option<String>,
    /// phase-307 W1 — rustc-visible crate name for the harness path dep.
    /// `None` falls back to `component_id`'s first `::` segment.
    pub crate_name: Option<String>,
    /// ROS package name (the `package` field of the emitted metadata).
    pub package: String,
    /// Component name (`Component::NAME`).
    pub component: String,
    pub executable: Option<String>,
    pub exported_symbol: Option<String>,
    /// The component crate directory (a Cargo path dependency of the harness).
    pub component_dir: PathBuf,
    /// nano-ros workspace root (for the `nros` path dependency).
    pub nano_ros_workspace: PathBuf,
    /// Where the harness writes the source-metadata JSON.
    pub output_path: PathBuf,
    /// Scratch directory for the generated harness crate.
    pub harness_dir: PathBuf,
}

/// The registered type's path.
///
/// A declared `class` is authoritative: the shipping `nros::node!(Class)` shape
/// is `impl Node for Class` in the crate root, so the historical positional
/// guess below (`crate::module::Component`) names a type that does not exist
/// and the harness fails to compile. The guess survives only as the fallback
/// for legacy `crate::module` component manifests, where it IS the convention.
fn component_type_path(o: &MetadataBuildOptions) -> Option<String> {
    if let Some(class) = o.class.as_deref().filter(|c| !c.is_empty()) {
        return Some(class.to_string());
    }
    let mut parts = o.component_id.split("::").filter(|p| !p.is_empty());
    let krate = parts.next()?;
    let module = parts.next()?;
    Some(format!("{krate}::{module}::Component"))
}

fn crate_name(o: &MetadataBuildOptions) -> Option<&str> {
    if let Some(name) = o.crate_name.as_deref().filter(|n| !n.is_empty()) {
        return Some(name);
    }
    // A declared class is `<crate>::<Type>`; the legacy id is `<crate>::<module>`.
    // Either way the crate is the first segment.
    o.class
        .as_deref()
        .unwrap_or(&o.component_id)
        .split("::")
        .next()
        .filter(|s| !s.is_empty())
}

/// Why a component cannot be metadata-probed, when it cannot (issue 0286).
///
/// The probe is compiled AND RUN, so it is only meaningful where a runnable
/// binary can be produced. `--target <host>` (above) handles the common case
/// of a workspace whose `.cargo/config.toml` selects a board target: the
/// harness ignores that target and builds for the host. One case defeats it.
///
/// **`[unstable] build-std` is not target-scoped.** A config that sets
/// `build-std = ["std", "panic_abort"]` makes cargo rebuild `std` FROM SOURCE
/// for whatever it is building — including the harness under
/// `--target <host>`. NuttX's workspace does exactly this and additionally
/// points `libc` at a NuttX-patched copy (declared by
/// `packages/boards/nros-board-nuttx-qemu/nros-board.toml`, delivered by sync
/// as a `# nros-managed` row since phase-351 W3), so the host `std` rebuild
/// fails on members that patched libc does not carry:
///
/// ```text
/// error[E0599]: no function or associated item named `default` found for
///               struct `timespec`
/// error: could not compile `std` (lib) due to 3 previous errors
/// Error: refresh source metadata for `nuttx_listener`
/// ```
///
/// There is no flag that turns `build-std` back off for one invocation, and
/// running outside the config walk-up is not an option — the `[patch]` entries
/// the harness needs live there. So such a component is simply un-probeable,
/// and the caller degrades to the sidecar-less path (the SystemModel bound)
/// rather than failing the build, exactly as it already does for a deploy-bound
/// crate or a non-Rust component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeBlocker {
    /// `[unstable] build-std` with a non-host `[build] target`: the host
    /// harness would rebuild `std` against that target's patched sysroot deps.
    BuildStdForForeignTarget { target: String },
}

impl core::fmt::Display for ProbeBlocker {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProbeBlocker::BuildStdForForeignTarget { target } => write!(
                f,
                "cargo config sets `[unstable] build-std` for target `{target}`; \
                 build-std is not target-scoped, so the host probe would rebuild \
                 std against that target's patched sysroot deps and fail to compile"
            ),
        }
    }
}

/// Decide whether the component at `component_dir` can be probed, from the
/// cargo config governing it. `None` means "probe away".
///
/// `host` is the host triple. When it cannot be determined nothing is skipped
/// — the probe is attempted, keeping the pre-0286 behaviour rather than
/// silently dropping components.
pub fn probe_blocker(component_dir: &Path, host: Option<&str>) -> Option<ProbeBlocker> {
    let cfg = cargo_config_facts(component_dir)?;
    let target = cfg.build_target?;
    if !cfg.build_std {
        return None; // `--target <host>` already handles this case
    }
    if host.is_some_and(|h| h == target) {
        return None; // build-std, but FOR the host — nothing foreign about it
    }
    Some(ProbeBlocker::BuildStdForForeignTarget { target })
}

struct CargoConfigFacts {
    build_target: Option<String>,
    build_std: bool,
}

/// Read the nearest `.cargo/config.toml` walking up from `dir`.
///
/// Cargo merges configs closest-first; the first file that declares
/// `build.target` is the one that decides it, and `[unstable] build-std` is
/// read from that same file (the workspace/example root that sets a board
/// target is where build-std is configured in this repo).
fn cargo_config_facts(dir: &Path) -> Option<CargoConfigFacts> {
    for ancestor in dir.ancestors() {
        let Ok(raw) = std::fs::read_to_string(ancestor.join(".cargo").join("config.toml")) else {
            continue;
        };
        let Ok(parsed) = raw.parse::<toml::Value>() else {
            continue;
        };
        let build_target = parsed
            .get("build")
            .and_then(|b| b.get("target"))
            .and_then(|t| t.as_str())
            .map(str::to_string);
        if build_target.is_none() {
            continue;
        }
        let build_std = parsed
            .get("unstable")
            .and_then(|u| u.get("build-std"))
            .is_some();
        return Some(CargoConfigFacts {
            build_target,
            build_std,
        });
    }
    None
}

/// A component id as a cargo package-name segment.
///
/// issue 0522 — the harness package and its `[[bin]]` used to be named
/// `nros-metadata-probe` / `probe` for EVERY component. That is fine while each
/// probe owns a private target dir and fatal once they share one: cargo does not
/// hash the final artifact name, so two components would write the same
/// `<target>/<host>/<profile>/probe` and the second `cargo run` could execute
/// the first one's binary. Phase-340 W1 measured exactly that failure on the
/// fixture lane (four different talker binaries, one artifact path, silently
/// last-writer-wins).
fn probe_slug(component_id: &str) -> String {
    component_id
        .replace("::", "__")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Where the probe harness compiles — issue 0522.
///
/// The harness SOURCE stays per component (it is generated, and each one deps a
/// different component crate). What moves is the cargo target dir, which used to
/// be `<harness>/target` — a private, full host build of the component and its
/// whole dependency graph, once per component. Measured 2026-08-12: **108 dirs,
/// 82.4 GiB**, and 162 of those trees held 312 `libnros_core` rlibs with only
/// **16 distinct `-C metadata` identities**, i.e. 296 literal repeats.
///
/// Resolution, widest sharing first:
///
/// 1. `$NROS_BUILD_ROOT/metadata-probe` — RFC-0070 R2's `<root>/<kind>` shape.
/// 2. `<nano-ros workspace>/build/metadata-probe` — the same default the shell's
///    `nros_build_root` uses (`<repo>/build`), reached through the checkout the
///    harness ALREADY points its `nros` path dep at, so this needs no new input.
///    Every probe in a checkout shares one dir.
/// 3. `<probe-root>/metadata-probe/.shared-target` — when (2) cannot be created,
///    which is the out-of-tree case where the nano-ros workspace is a read-only
///    installed SDK. Still shares within a workspace, and is writable by
///    construction because the per-component harness dirs already live there.
///
/// (2) rather than "(1) or per-component" on purpose: `NROS_BUILD_ROOT` is a
/// function-local default in `build-root.sh`, not an exported variable, so a
/// plain `nros metadata --build` — and every cmake configure that shells the
/// CLI — sees it UNSET. A fix that only worked when it was set would have left
/// the measured 108 dirs exactly as they were.
///
/// Cargo already separates by triple below the root (`--target <host>` gives
/// `<dir>/<host>/<profile>/`), so the coordinate is not repeated here.
fn probe_target_dir(o: &MetadataBuildOptions) -> PathBuf {
    if let Some(root) = std::env::var_os("NROS_BUILD_ROOT").filter(|v| !v.is_empty()) {
        return PathBuf::from(root).join("metadata-probe");
    }
    let shared = o.nano_ros_workspace.join("build").join("metadata-probe");
    if std::fs::create_dir_all(&shared).is_ok() {
        return shared;
    }
    match o.harness_dir.parent() {
        Some(parent) => parent.join(".shared-target"),
        None => o.harness_dir.join("target"),
    }
}

pub fn render_harness_cargo_toml(o: &MetadataBuildOptions) -> Result<String> {
    let krate = crate_name(o)
        .ok_or_else(|| eyre!("component id '{}' has no crate segment", o.component_id))?;
    // `[workspace]` — the harness is generated into an arbitrary scratch dir;
    // without its own (empty) workspace table cargo walks up and captures it
    // into whatever workspace encloses that dir ("current package believes
    // it's in a workspace when it's not") — e.g. a user running `nros
    // metadata --build` anywhere under a cargo workspace (issue #202 triage).
    Ok(format!(
        "[package]\n\
         name = \"nros-metadata-probe-{slug}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\n\
         [workspace]\n\n\
         # issue 0288 — the component may be `#![no_std]`, and a host target\n\
         # defaults to `panic = \"unwind\"`, which needs std:\n\
         #   error: unwinding panics are not supported without std\n\
         # The probe only ever builds to RECORD what a component registers, so\n\
         # abort is the right strategy as well as the working one. Set here\n\
         # rather than asked of every example, because this is the probe\'s\n\
         # requirement, not theirs.\n\
         [profile.dev]\n\
         panic = \"abort\"\n\n\
         [profile.release]\n\
         panic = \"abort\"\n\n\
         [[bin]]\n\
         name = \"probe-{slug}\"\n\
         path = \"src/main.rs\"\n\n\
         [dependencies]\n\
         nros = {{ path = {nros:?}, features = [\"std\"] }}\n\
         # issue 0288 layer 5 — a deploy-bound example deps its BOARD crate,\n\
         # whose C build provides the ~90 `nros_platform_*` extern-C ABI symbols\n\
         # that `nros-platform-cffi`'s `CffiPlatform` calls. On the host probe the\n\
         # board's cross-compiled C is skipped (`host_probe::skip_cross_build`), so\n\
         # those symbols go undefined and the probe fails at LINK. `posix-c-port`\n\
         # is the host-buildable C port that DEFINES them; depping it here makes\n\
         # the sole provider on the host (feature-unified onto the same\n\
         # `nros-platform-cffi` the component pulls) so the probe links and yields\n\
         # exact executor sizing instead of degrading to the timer-blind bound.\n\
         nros-platform-cffi = {{ path = {platform_cffi:?}, features = [\"posix-c-port\"] }}\n\
         {krate} = {{ path = {comp:?}, package = {pkg:?} }}\n",
        slug = probe_slug(&o.component_id),
        nros = o
            .nano_ros_workspace
            .join("packages/api/nros")
            .display()
            .to_string(),
        platform_cffi = o
            .nano_ros_workspace
            .join("packages/platform/nros-platform-cffi")
            .display()
            .to_string(),
        comp = o.component_dir.display().to_string(),
        pkg = cargo_package_name(&o.component_dir).unwrap_or_else(|| krate.to_string()),
    ))
}

/// The component crate's REAL Cargo package name, read from its manifest.
///
/// The harness dep must be keyed by the rustc-visible crate name (so
/// `use <krate>::…` in `main.rs` resolves) but a path dependency is looked up
/// by PACKAGE name, and the two differ whenever an example is named with
/// hyphens (`qemu-rtic-action-client` → crate `qemu_rtic_action_client`). Cargo
/// resolves that with the `package = "…"` rename field; without it the build
/// fails "no matching package named `qemu_rtic_action_client` found".
///
/// Falls back to the crate name when the manifest can't be read — the
/// underscored-name case, which is the majority and was already working.
fn cargo_package_name(component_dir: &Path) -> Option<String> {
    let manifest = std::fs::read_to_string(component_dir.join("Cargo.toml")).ok()?;
    let parsed: toml::Value = manifest.parse().ok()?;
    parsed
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

pub fn render_harness_main(o: &MetadataBuildOptions) -> Result<String> {
    let type_path = component_type_path(o).ok_or_else(|| {
        eyre!(
            "component '{}' declares no `class` and its id is not `crate::module` \
             — cannot name the registered type",
            o.component_id
        )
    })?;
    let exe = o
        .executable
        .as_deref()
        .map(|e| format!("\n        .executable({e:?})"))
        .unwrap_or_default();
    let sym = o
        .exported_symbol
        .as_deref()
        .map(|s| format!("\n        .exported_symbol({s:?})"))
        .unwrap_or_default();
    Ok(format!(
        "// Generated metadata-mode harness (Phase 172.E). Records {type_path}'s\n\
         // declarations against an in-memory recorder; opens no transport.\n\
         fn main() {{\n\
         \x20   // Bare type ⇒ default capacity const-params.\n\
         \x20   let mut recorder: nros::MetadataRecorder = nros::MetadataRecorder::default();\n\
         \x20   nros::record_node_metadata::<{type_path}>(&mut recorder)\n\
         \x20       .expect(\"component register (metadata mode)\");\n\
         \x20   let export = nros::SourceMetadataExport::new({pkg:?}, {comp:?}){exe}{sym};\n\
         \x20   let json = recorder\n\
         \x20       .to_source_metadata_json(&export)\n\
         \x20       .expect(\"serialize source metadata\");\n\
         \x20   // Issue 0498 — temp + rename, NOT `fs::write`, which truncates\n\
         \x20   // to zero and then fills. Concurrent fixture rows of one leaf\n\
         \x20   // read this path; an empty read surfaces as \"EOF at line 1\n\
         \x20   // column 0\" and reads like a bug in this harness. Inlined\n\
         \x20   // rather than calling `nros_cli_core::atomic_file` because this\n\
         \x20   // is a standalone generated crate that does not depend on the\n\
         \x20   // CLI. The temp is a sibling: rename(2) is atomic only within\n\
         \x20   // one filesystem.\n\
         \x20   let out = std::path::Path::new({out:?});\n\
         \x20   let tmp = out.with_extension(format!(\"tmp.{{}}\", std::process::id()));\n\
         \x20   std::fs::write(&tmp, json).expect(\"write source metadata\");\n\
         \x20   std::fs::rename(&tmp, out).expect(\"rename source metadata\");\n\
         }}\n",
        pkg = o.package,
        comp = o.component,
        out = o.output_path.display().to_string(),
    ))
}

/// Generate the harness crate, then `cargo run` it so it writes the
/// source-metadata JSON to `output_path`.
pub fn build_metadata(o: &MetadataBuildOptions) -> Result<()> {
    let src = o.harness_dir.join("src");
    std::fs::create_dir_all(&src).wrap_err_with(|| format!("create {}", src.display()))?;
    write_if_changed(
        &o.harness_dir.join("Cargo.toml"),
        &render_harness_cargo_toml(o)?,
    )?;
    write_if_changed(&src.join("main.rs"), &render_harness_main(o)?)?;
    if let Some(parent) = o.output_path.parent() {
        std::fs::create_dir_all(parent).wrap_err_with(|| format!("create {}", parent.display()))?;
    }

    // The probe is a HOST binary — that is the whole reason one probe covers
    // every deploy target. But a standalone embedded example's
    // `.cargo/config.toml` sets `[build] target = "thumbv7m-none-eabi"`, and
    // the harness inherits it through cargo's config walk-up (which it must,
    // for the `[patch.crates-io]` entries). Without an explicit `--target` the
    // probe cross-compiles for the board and dies on `can't find crate for
    // std`. An explicit flag beats config, so name the host triple.
    let host = host_triple();
    let manifest = o.harness_dir.join("Cargo.toml");
    let target_dir = probe_target_dir(o);
    std::fs::create_dir_all(&target_dir)
        .wrap_err_with(|| format!("create probe target dir {}", target_dir.display()))?;
    // #0390 — capture stderr (still echoed live below) so a harness that dies
    // because a vendored `[source.*]` tree is absent can be translated from cargo's
    // raw four-`Caused by:` path error into `nros setup --source <name>`, the
    // vocabulary a CLI-provisioned user actually has. stdout stays inherited, so
    // only stderr is buffered (spawn + wait_with_output, not `.output()`, which
    // would pipe stdout too).
    let child = Command::new("cargo")
        // phase-307 W1 — cargo discovers `.cargo/config.toml` by walking up
        // from its CWD, and a Node pkg's generated interface deps
        // (`example_interfaces = { version = "*" }` and friends) exist ONLY as
        // `[patch.crates-io]` entries in the consuming workspace's config. Run
        // from the harness dir (which the caller places inside the workspace)
        // so those patches are in scope; inheriting the invoking cwd resolved
        // the interface crates against crates.io and failed with "no matching
        // package named `example_interfaces` found".
        .current_dir(&o.harness_dir)
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target")
        .arg(&host)
        .arg("--target-dir")
        .arg(&target_dir)
        // The harness inherits no pinned toolchain so a generated
        // `rust-toolchain.toml` elsewhere can't force a re-resolve.
        .env_remove("RUSTUP_TOOLCHAIN")
        // phase-359 W7 — `[unstable] build-std` is the SIBLING of the `--target`
        // override above, and it comes from the same place for the same reason:
        // the board's `.cargo/nros-board.toml`, inherited by the config walk-up
        // the `[patch.crates-io]` entries require. `--target` had to be
        // overridden because the board's is wrong for a HOST probe; `build-std`
        // is wrong for exactly the same reason, and an env var beats config the
        // way an explicit flag does.
        //
        // It stayed invisible while the NuttX boards said `build-std = ["std",
        // …]`: cargo then built `std` from source for the host too, which is
        // consistent, merely slow. Narrowing that to `["core", "alloc", …]`
        // made it build `core` from source and link it beside the PREBUILT host
        // `std` that depends on a different `core` — `duplicate lang item in
        // crate `core` (which `std` depends on): `sized``. The probe wants the
        // host's ordinary prebuilt libraries, which is what an empty value
        // selects.
        .env("CARGO_UNSTABLE_BUILD_STD", "")
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .wrap_err_with(|| format!("run metadata-mode harness for '{}'", o.component_id))?;
    let out = child
        .wait_with_output()
        .wrap_err_with(|| format!("run metadata-mode harness for '{}'", o.component_id))?;
    // Echo the captured stderr so the harness's own diagnostics are not lost —
    // but only when the probe SUCCEEDED.
    //
    // issue 0426: on failure this echo dumped the probe's full rustc output and
    // the caller then degraded QUIETLY (deploy-bound examples fall back to the
    // SystemModel bound). The operator saw a screenful of `error[E0432]` from a
    // build that exits 0, with nothing tying the two together — and a REAL
    // compile error in that package looked exactly the same. A failure's cause
    // belongs in the failure MESSAGE, which the caller prints on the same line
    // as the degradation; see the excerpt folded into `bail!` below.
    if out.status.success() {
        use std::io::Write;
        let _ = std::io::stderr().write_all(&out.stderr);
    }
    if !out.status.success() {
        let code = out.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&out.stderr);
        // The first rustc diagnostic is what identifies the cause; the rest is
        // notes and a second copy under `--message-format`. Keep it short — this
        // rides inside a one-line "probe failed" report.
        let excerpt = first_diagnostic(&stderr);
        // #0390 — if the failure is a missing vendored `[source.*]` tree, name
        // the remedy in the user's vocabulary rather than leaving cargo's raw
        // path error. Index-driven (dest → package name), so it stays correct
        // as sources are added — never hand-written per build script.
        match load_source_index(&o.nano_ros_workspace)
            .as_ref()
            .and_then(|idx| missing_source_remedy(&stderr, idx))
        {
            Some(remedy) => bail!(
                "metadata-mode harness failed (exit {code}) for component '{}'\n  \
                 → a vendored source it resolves is not provisioned — {remedy}",
                o.component_id
            ),
            None => bail!(
                "metadata-mode harness failed (exit {code}) for component '{}': {excerpt}",
                o.component_id
            ),
        }
    }
    if !o.output_path.is_file() {
        bail!(
            "metadata-mode harness produced no source metadata at {}",
            o.output_path.display()
        );
    }
    relativise_source_artifacts(&o.output_path, &o.component_dir)?;
    Ok(())
}

/// The first rustc diagnostic line from a captured stderr, for folding a probe
/// failure's CAUSE into its one-line report (issue 0426).
///
/// Deliberately the first `error…` line and nothing else: the caller renders
/// this inside `"<pkg>::<component> (deploy-bound probe failed: …)"`, and a
/// multi-line rustc dump there is what this issue exists to stop. Falls back to
/// the last non-empty line when nothing matches, so a non-rustc failure (a
/// linker, a missing binary) still says something.
fn first_diagnostic(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("error"))
        .map(str::to_string)
        .or_else(|| panic_message(stderr))
        .or_else(|| {
            stderr
                .lines()
                .map(str::trim)
                .rfind(|l| !l.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "no diagnostic captured".to_string())
}

/// The message of a Rust panic in the harness — the line AFTER `panicked at …`.
///
/// issue 0699: the harness can fail at RUN time rather than build time, and then
/// there is no line starting with `error`. `first_diagnostic` fell through to
/// "last non-empty line", which is `note: run with RUST_BACKTRACE=1 …` — so a
/// `Metadata(NameTooLong)` panic reached the operator as four frames naming a
/// component and an exit code and nothing about the cause. Re-running the staged
/// harness by hand was the only way to see it, and nothing tells the user that
/// directory exists.
///
/// Same shape as `nros_tests::skip_marker` one lane over, and it is keyed the
/// same way for the same reason: the informative text is defined by its position
/// (immediately after the panic header), not by a substring search over the whole
/// stream — a build that merely PRINTS the word would otherwise be misreported.
fn panic_message(stderr: &str) -> Option<String> {
    let lines: Vec<&str> = stderr.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("thread '") && l.contains("panicked at"))?;
    let msg = lines
        .get(at + 1)
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .unwrap_or(lines[at]);
    Some(format!("{} ({})", msg, lines[at]))
}

/// Rewrite absolute source paths in the harness output to be relative to the
/// component package.
///
/// `SourceLocation::caller()` records `core::panic::Location::caller().file()`,
/// and rustc emits that ABSOLUTE whenever the recording crate is compiled as a
/// path dependency from a different working directory — which is exactly how
/// the harness builds it. So the JSON lands with the recording user's HOME
/// directory baked into `source.artifact` — in a TRACKED file, so anyone who
/// syncs the workspace dirties the tree with their own paths.
/// `check-absolute-paths` catches it, but only after the fact.
///
/// This is the issue-0320 class one layer over: models were taught to record
/// relative paths by passing `--bringup-root` to the resolver, and the metadata
/// writer never got the same treatment.
///
/// Done as a textual prefix strip rather than a parse/reserialise so the
/// harness keeps sole ownership of the file's formatting — reserialising would
/// silently reformat every generated metadata file the first time this ran.
/// `--remap-path-prefix` via `RUSTFLAGS` would fix it at the source, but the
/// env var REPLACES any `[build] rustflags` from the workspace's
/// `.cargo/config.toml`, which the embedded packages here rely on.
fn relativise_source_artifacts(output_path: &Path, component_dir: &Path) -> Result<()> {
    let text = std::fs::read_to_string(output_path)
        .wrap_err_with(|| format!("read source metadata {}", output_path.display()))?;
    // Both spellings: the recorded path may or may not be canonicalised.
    let mut prefixes: Vec<String> = Vec::new();
    if let Ok(canon) = component_dir.canonicalize() {
        prefixes.push(format!("{}/", canon.display()));
    }
    let plain = format!("{}/", component_dir.display());
    if !prefixes.contains(&plain) {
        prefixes.push(plain);
    }
    let mut out = text.clone();
    for prefix in &prefixes {
        // Only inside JSON string values, so a prefix appearing bare in some
        // other context is left alone.
        out = out.replace(&format!("\"{prefix}"), "\"");
    }
    if out != text {
        // Issue 0498 — atomic, like every other writer of this path.
        crate::atomic_file::atomic_write(output_path, &out)
            .wrap_err_with(|| format!("write source metadata {}", output_path.display()))?;
    }
    Ok(())
}

/// The host target triple, from `rustc -vV`.
///
/// Falls back to no explicit target when rustc cannot be read; a workspace
/// whose config sets no `[build] target` then behaves exactly as before.
pub fn host_triple() -> String {
    let out = Command::new("rustc").arg("-vV").output();
    if let Ok(out) = out
        && let Ok(text) = String::from_utf8(out.stdout)
        && let Some(line) = text.lines().find(|l| l.starts_with("host: "))
    {
        return line["host: ".len()..].trim().to_string();
    }
    // Best-effort default; the common host in this repo's CI + dev images.
    "x86_64-unknown-linux-gnu".to_string()
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    // issue 0562 — delegates; this copy also lacked the ATOMICITY half.
    crate::atomic_file::atomic_write(path, contents)
}

#[cfg(test)]
mod probe_blocker_tests {
    use super::*;

    fn write_cfg(dir: &Path, body: &str) {
        std::fs::create_dir_all(dir.join(".cargo")).unwrap();
        std::fs::write(dir.join(".cargo").join("config.toml"), body).unwrap();
    }

    /// Issue 0286 — the nuttx shape: a foreign `[build] target` PLUS
    /// `[unstable] build-std`. build-std is not target-scoped, so even the
    /// `--target <host>` harness would rebuild std against that target's
    /// patched sysroot deps. Must be reported un-probeable, not attempted.
    #[test]
    fn build_std_with_a_foreign_target_is_unprobeable() {
        let dir = tempfile::tempdir().unwrap();
        write_cfg(
            dir.path(),
            "[build]\ntarget = \"armv7a-nuttx-eabihf\"\n\
             [unstable]\nbuild-std = [\"std\", \"panic_abort\"]\n",
        );
        assert_eq!(
            probe_blocker(dir.path(), Some("x86_64-unknown-linux-gnu")),
            Some(ProbeBlocker::BuildStdForForeignTarget {
                target: "armv7a-nuttx-eabihf".to_string()
            })
        );
    }

    /// The qemu-arm-baremetal shape: a foreign target but NO build-std. The
    /// `--target <host>` flag already covers it, so it stays probeable —
    /// skipping it would silently cost that lane its exact executor sizing.
    #[test]
    fn a_foreign_target_without_build_std_stays_probeable() {
        let dir = tempfile::tempdir().unwrap();
        write_cfg(
            dir.path(),
            "[build]\ntarget = \"thumbv7m-none-eabi\"\n\
             [target.thumbv7m-none-eabi]\nrunner = \"qemu-system-arm -kernel\"\n",
        );
        assert_eq!(
            probe_blocker(dir.path(), Some("x86_64-unknown-linux-gnu")),
            None
        );
    }

    /// build-std FOR the host is not foreign — nothing to skip.
    #[test]
    fn build_std_for_the_host_target_is_probeable() {
        let dir = tempfile::tempdir().unwrap();
        write_cfg(
            dir.path(),
            "[build]\ntarget = \"x86_64-unknown-linux-gnu\"\n\
             [unstable]\nbuild-std = [\"std\"]\n",
        );
        assert_eq!(
            probe_blocker(dir.path(), Some("x86_64-unknown-linux-gnu")),
            None
        );
    }

    /// No config, or a config selecting no target: plain host build.
    #[test]
    fn no_build_target_is_probeable() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            probe_blocker(dir.path(), Some("x86_64-unknown-linux-gnu")),
            None
        );
        write_cfg(dir.path(), "[net]\nretry = 3\n");
        assert_eq!(
            probe_blocker(dir.path(), Some("x86_64-unknown-linux-gnu")),
            None
        );
    }

    /// An unknown host must not cause wholesale skipping — attempting the
    /// probe and failing loudly beats silently under-counting every executor.
    #[test]
    fn an_unknown_host_never_skips() {
        let dir = tempfile::tempdir().unwrap();
        write_cfg(
            dir.path(),
            "[build]\ntarget = \"armv7a-nuttx-eabihf\"\n\
             [unstable]\nbuild-std = [\"std\"]\n",
        );
        // Host unknown -> the foreign-target comparison can't be made, but the
        // blocker is still reported: build-std + a target we cannot prove is
        // the host is exactly the failing shape.
        assert!(probe_blocker(dir.path(), None).is_some());
    }

    /// The config nearest the component wins, matching cargo's own merge.
    #[test]
    fn the_nearest_config_decides() {
        let root = tempfile::tempdir().unwrap();
        write_cfg(
            root.path(),
            "[build]\ntarget = \"armv7a-nuttx-eabihf\"\n\
             [unstable]\nbuild-std = [\"std\"]\n",
        );
        let inner = root.path().join("pkg");
        write_cfg(&inner, "[build]\ntarget = \"x86_64-unknown-linux-gnu\"\n");
        assert_eq!(
            probe_blocker(&inner, Some("x86_64-unknown-linux-gnu")),
            None,
            "the closer config selects the host, so the outer nuttx one must not apply"
        );
    }
}

/// Load the SDK index from the nano-ros workspace root, or `None` if it is
/// absent / unparseable — a missing index must never turn a build failure into
/// a panic; the #0390 remedy hint is best-effort.
fn load_source_index(
    nano_ros_workspace: &Path,
) -> Option<crate::orchestration::sdk_index::SdkIndex> {
    crate::orchestration::sdk_index::SdkIndex::load(&nano_ros_workspace.join("nros-sdk-index.toml"))
        .ok()
}

/// #0390 — scan a metadata-harness stderr for the first `[source.*]` `dest` path
/// that appears and return `run: nros setup --source <name>`. cargo names the
/// ABSOLUTE path; `dest` is its workspace-relative suffix, so `contains`
/// matches. `None` when no known source path is implicated. Index-driven, so it
/// stays correct as sources are added — never hand-written per build script.
fn missing_source_remedy(
    stderr: &str,
    index: &crate::orchestration::sdk_index::SdkIndex,
) -> Option<String> {
    index.source.iter().find_map(|(name, src)| {
        let dest = src.dest.as_deref()?;
        (!dest.is_empty() && stderr.contains(dest))
            .then(|| format!("run: nros setup --source {name}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// issue 0699 — a RUNTIME failure in the harness must reach the operator.
    ///
    /// There is no `error`-prefixed line in this stream, so the old fallback
    /// ("last non-empty line") reported the backtrace note and the real cause —
    /// `Metadata(NameTooLong)`, which only happens at some workspace depths —
    /// never appeared in `nros sync`'s output at all.
    #[test]
    fn a_harness_panic_is_the_diagnostic_not_the_backtrace_note() {
        let stderr = "\
   Compiling listener_pkg v0.0.0
    Finished `dev` profile [unoptimized] target(s) in 1.2s
     Running `target/debug/metadata-probe`
thread 'main' panicked at src/main.rs:7:10:
component register (metadata mode): Metadata(NameTooLong)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
";
        let got = first_diagnostic(stderr);
        assert!(
            got.contains("Metadata(NameTooLong)"),
            "the panic message must survive, got {got:?}"
        );
        assert!(
            got.contains("src/main.rs:7:10"),
            "and so must where it happened, got {got:?}"
        );
        assert!(
            !got.starts_with("note:"),
            "the backtrace note is what this replaces, got {got:?}"
        );
    }

    /// A build failure still wins: a rustc diagnostic is the more specific
    /// answer, and it is the common case (#0426).
    #[test]
    fn a_rustc_error_still_outranks_a_panic() {
        let stderr = "\
error[E0432]: unresolved import `std_msgs`
thread 'main' panicked at src/main.rs:7:10:
some later noise
";
        assert!(first_diagnostic(stderr).starts_with("error[E0432]"));
    }

    #[test]
    fn missing_source_remedy_names_the_setup_command() {
        // Loads the REAL shipped index, so this also asserts the dest→package
        // mapping matches the sources #0390 hit (nuttx-libc, xrce).
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("nros-sdk-index.toml").is_file())
            .expect("nros-sdk-index.toml above the crate");
        let idx = load_source_index(root).expect("index loads");

        // Synthetic cargo output. The prefix is deliberately not a home-directory
        // path:
        // `check-absolute-paths` scans tracked sources textually and cannot tell
        // a fixture string from a real baked-in build-host path.
        let nuttx = "error: failed to load source for dependency `libc`\n  \
                     Caused by: unable to update \
                     /wsroot/third-party/nuttx/libc";
        assert_eq!(
            missing_source_remedy(nuttx, &idx).as_deref(),
            Some("run: nros setup --source nuttx-libc")
        );

        let xrce = "nros-rmw-xrce-cffi: vendored `micro-xrce-dds-client` source root \
                    /wsroot/packages/rmw/xrce/xrce-sys/micro-xrce-dds-client/src/c \
                    is missing";
        assert_eq!(
            missing_source_remedy(xrce, &idx).as_deref(),
            Some("run: nros setup --source micro-xrce-dds-client")
        );

        assert!(missing_source_remedy("error: some unrelated failure", &idx).is_none());
    }

    fn opts() -> MetadataBuildOptions {
        MetadataBuildOptions {
            component_id: "demo_pkg::talker".into(),
            class: None,
            crate_name: None,
            package: "demo_pkg".into(),
            component: "talker".into(),
            executable: Some("talker".into()),
            exported_symbol: Some("nros_component_talker".into()),
            component_dir: PathBuf::from("/ws/src/demo_pkg"),
            nano_ros_workspace: PathBuf::from("/nano-ros"),
            output_path: PathBuf::from("/out/talker.metadata.json"),
            harness_dir: PathBuf::from("/scratch/probe"),
        }
    }

    #[test]
    fn type_path_and_crate_name_legacy_positional_guess() {
        let o = opts();
        assert_eq!(
            component_type_path(&o).as_deref(),
            Some("demo_pkg::talker::Component")
        );
        assert_eq!(crate_name(&o), Some("demo_pkg"));
        let mut bare = opts();
        bare.component_id = "nocrate".into();
        assert_eq!(component_type_path(&bare), None); // needs crate::module
    }

    /// phase-307 W1 — the shipping `nros::node!(Class)` shape: a declared class
    /// names the type directly, and the crate name is carried, not guessed from
    /// a component id that no longer contains it.
    #[test]
    fn declared_class_wins_over_the_positional_guess() {
        let mut o = opts();
        o.component_id = "fibonacci_client".into();
        o.class = Some("action_client_pkg::FibonacciClient".into());
        o.crate_name = Some("action_client_pkg".into());
        assert_eq!(
            component_type_path(&o).as_deref(),
            Some("action_client_pkg::FibonacciClient")
        );
        assert_eq!(crate_name(&o), Some("action_client_pkg"));
        let main = render_harness_main(&o).unwrap();
        assert!(main.contains("record_node_metadata::<action_client_pkg::FibonacciClient>"));
        let toml = render_harness_cargo_toml(&o).unwrap();
        assert!(toml.contains(
            "action_client_pkg = { path = \"/ws/src/demo_pkg\", package = \"action_client_pkg\" }"
        ));
    }

    /// A class without an explicit crate name still resolves: the class's own
    /// first segment IS the crate.
    #[test]
    fn crate_name_falls_back_to_the_class_head() {
        let mut o = opts();
        o.component_id = "talker".into();
        o.class = Some("talker_pkg::Talker".into());
        assert_eq!(crate_name(&o), Some("talker_pkg"));
    }

    #[test]
    fn harness_main_calls_record_and_serialize() {
        let main = render_harness_main(&opts()).unwrap();
        assert!(main.contains("record_node_metadata::<demo_pkg::talker::Component>"));
        assert!(main.contains("SourceMetadataExport::new(\"demo_pkg\", \"talker\")"));
        assert!(main.contains(".executable(\"talker\")"));
        assert!(main.contains(".exported_symbol(\"nros_component_talker\")"));
        assert!(main.contains("to_source_metadata_json"));
        assert!(main.contains("/out/talker.metadata.json"));
    }

    /// Issue 0498 — the harness must write a temp sibling and RENAME, never
    /// `fs::write` the destination.
    ///
    /// Several fixture rows of one leaf sync concurrently and read this path,
    /// and `fs::write` truncates to zero before it fills — a reader in that
    /// window gets "EOF while parsing a value at line 1 column 0", which reads
    /// like a bug in this harness rather than a race.
    ///
    /// Asserted on the RENDERED text because that text is a string template:
    /// nothing type-checks it until a fixture build compiles the generated
    /// crate, which is minutes away and on someone else's machine.
    #[test]
    fn harness_main_writes_its_output_atomically() {
        let main = render_harness_main(&opts()).unwrap();
        assert!(
            main.contains("std::fs::rename(&tmp, out)"),
            "harness must rename into place:\n{main}"
        );
        assert!(
            !main.contains("std::fs::write(out,"),
            "harness must not truncate its destination:\n{main}"
        );
        // The temp is a SIBLING — rename(2) is atomic only within one
        // filesystem, so a temp in /tmp would silently degrade to a copy.
        assert!(
            main.contains("out.with_extension("),
            "temp must sit beside the destination:\n{main}"
        );
    }

    /// issue 0288 — the harness must build with `panic = "abort"`.
    ///
    /// A standalone example's lib is often `#![no_std]`, and a host target
    /// defaults to `panic = "unwind"`, which requires std:
    ///
    /// ```text
    /// error: unwinding panics are not supported without std
    /// ```
    ///
    /// That blocked host-probing those packages entirely, so their executor
    /// sizing fell back to the SystemModel's timer-blind bound. Asserted on the
    /// EMITTED manifest because that is the only place the setting exists —
    /// there is no type to hang it off, so nothing else would notice its
    /// removal.
    #[test]
    fn harness_cargo_toml_builds_with_panic_abort() {
        let toml = render_harness_cargo_toml(&opts()).expect("renders");
        assert!(
            toml.contains("[profile.dev]") && toml.contains("panic = \"abort\""),
            "harness must set panic=abort or a no_std component cannot be \
             host-probed:\n{toml}"
        );
    }

    #[test]
    fn harness_cargo_toml_path_deps_nros_std_and_component() {
        let toml = render_harness_cargo_toml(&opts()).unwrap();
        assert!(
            toml.contains(
                "nros = { path = \"/nano-ros/packages/api/nros\", features = [\"std\"] }"
            )
        );
        assert!(
            toml.contains("demo_pkg = { path = \"/ws/src/demo_pkg\", package = \"demo_pkg\" }")
        );
    }

    /// A path dependency is looked up by PACKAGE name while the dep key must be
    /// the rustc-visible CRATE name — they differ for every hyphenated example
    /// (`qemu-rtic-action-client` → crate `qemu_rtic_action_client`), which used
    /// to fail "no matching package named `qemu_rtic_action_client` found".
    /// Cargo's `package = "…"` rename bridges the two.
    #[test]
    fn harness_cargo_toml_renames_hyphenated_component_packages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"qemu-rtic-action-client\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let mut o = opts();
        o.component_dir = dir.path().to_path_buf();
        o.crate_name = Some("qemu_rtic_action_client".into());
        let toml = render_harness_cargo_toml(&o).unwrap();
        assert!(
            toml.contains("qemu_rtic_action_client = { path =")
                && toml.contains("package = \"qemu-rtic-action-client\""),
            "dep must be keyed by crate name and renamed to the package, got:\n{toml}"
        );
    }

    #[test]
    fn harness_main_omits_optional_export_fields_when_absent() {
        let mut o = opts();
        o.executable = None;
        o.exported_symbol = None;
        let main = render_harness_main(&o).unwrap();
        assert!(main.contains("SourceMetadataExport::new(\"demo_pkg\", \"talker\");"));
        assert!(!main.contains(".executable("));
        assert!(!main.contains(".exported_symbol("));
    }
}

//! `nros codegen cyclonedds-descriptors` — Phase 212.K.4
//!
//! Per-example Cyclone-DDS topic-descriptor emit.
//!
//! Reads one or more `.msg` sources (named by `<pkg>/<Msg>`), synthesises
//! Cyclone-shaped IDL via [`nros_msg_to_idl`], shells out to the host
//! `idlc` to produce the matching `<pkg>_<Msg>.{c,h}` pair, then writes
//! a tiny `register.{c,h}` translation unit that exposes a single
//! `extern "C"` entry per crate. Also drops a JSON manifest enumerating
//! every generated artifact so the consumer build script (the
//! `nros-build::cyclonedds::Descriptors` helper) can feed them into a
//! `cc::Build`.
//!
//! Usage:
//!
//!   nros codegen cyclonedds-descriptors \
//!       --idlc <path> \
//!       --include <path> \
//!       --msg std_msgs/Int32=/abs/path/to/Int32.msg \
//!       [--msg ...] \
//!       --crate-name <ident> \
//!       --out <dir>
//!
//! Or via JSON `--args-file <path>` mirroring the `nros codegen` surface
//! (the build-script helper uses argv directly; `--args-file` exists for
//! parity with the existing `nros codegen` shape).
//!
//! The K.2 baked-in defaults in `packages/dds/nros-rmw-cyclonedds-sys/build.rs`
//! (std_msgs/Int32 + rmw_dds_common_graph) stay as universal fallbacks —
//! this verb is purely additive (per-example descriptors layered on top).

use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

use clap::Args as ClapArgs;
use eyre::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Path to the host `idlc` binary (typically `build/cyclonedds/bin/idlc`).
    #[arg(long)]
    pub idlc: Option<PathBuf>,

    /// Path to the Cyclone include dir (`<sysroot>/include`).
    #[arg(long)]
    pub include: Option<PathBuf>,

    /// One `<pkg>/<Msg>=<msg-source-path>` per message. Repeatable.
    #[arg(long = "msg", value_name = "PKG/MSG=PATH")]
    pub msg: Vec<String>,

    /// Crate / consumer name. Used as the prefix of the generated
    /// `extern "C" void <name>_register_descriptors(void)` entry point.
    /// Defaults to `nros_descriptors`.
    #[arg(long = "crate-name")]
    pub crate_name: Option<String>,

    /// Output directory. Created if missing. Receives `*.{c,h}`,
    /// `register.{c,h}`, and `cyclonedds-descriptors.json`.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Optional JSON file carrying every flag as a structured doc.
    /// When set, takes precedence over the matching argv flag.
    #[arg(long)]
    pub args_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
struct ArgsFile {
    idlc: PathBuf,
    include: PathBuf,
    #[serde(default)]
    crate_name: Option<String>,
    out: PathBuf,
    messages: Vec<MsgSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct MsgSpec {
    /// `<pkg>/<Msg>` — e.g. `std_msgs/Int32`.
    name: String,
    /// Filesystem path to the `.msg` source.
    source: PathBuf,
}

/// One emitted descriptor entry recorded in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub pkg: String,
    pub msg: String,
    pub idl_path: PathBuf,
    pub c_path: PathBuf,
    pub h_path: PathBuf,
    pub type_name: String,
    pub descriptor_symbol: String,
}

/// Top-level manifest written next to the generated sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub crate_name: String,
    pub register_entry: String,
    pub register_c: PathBuf,
    pub register_h: PathBuf,
    pub descriptors: Vec<ManifestEntry>,
}

pub fn run(args: Args) -> Result<()> {
    let resolved = resolve_args(args)?;
    emit(&resolved).context("emit cyclonedds descriptors")?;
    Ok(())
}

#[derive(Debug)]
struct ResolvedArgs {
    idlc: PathBuf,
    /// Recorded for parity with the spec; `idlc` itself drives include
    /// resolution via the `-I` flag we don't currently emit (Cyclone's
    /// in-tree IDL set is fully self-contained for the bundled types).
    #[allow(dead_code)]
    include: PathBuf,
    crate_name: String,
    out: PathBuf,
    messages: Vec<(String, String, PathBuf)>, // (pkg, msg, source-path)
}

fn resolve_args(args: Args) -> Result<ResolvedArgs> {
    if let Some(path) = args.args_file {
        let body = fs::read_to_string(&path)
            .with_context(|| format!("read --args-file {}", path.display()))?;
        let doc: ArgsFile = serde_json::from_str(&body)
            .with_context(|| format!("parse --args-file {} as JSON", path.display()))?;
        let crate_name = doc
            .crate_name
            .unwrap_or_else(|| default_crate_name().to_string());
        let messages = doc
            .messages
            .into_iter()
            .map(|m| {
                let (pkg, msg) = split_pkg_msg(&m.name)?;
                Ok::<_, eyre::Report>((pkg, msg, m.source))
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(ResolvedArgs {
            idlc: doc.idlc,
            include: doc.include,
            crate_name,
            out: doc.out,
            messages,
        });
    }

    let idlc = args
        .idlc
        .ok_or_else(|| eyre::eyre!("--idlc is required (or use --args-file)"))?;
    let include = args
        .include
        .ok_or_else(|| eyre::eyre!("--include is required (or use --args-file)"))?;
    let out = args
        .out
        .ok_or_else(|| eyre::eyre!("--out is required (or use --args-file)"))?;
    if args.msg.is_empty() {
        bail!("at least one --msg <pkg/Msg=<path>> is required");
    }

    let messages = args
        .msg
        .iter()
        .map(|raw| {
            let (left, source) = raw
                .split_once('=')
                .ok_or_else(|| eyre::eyre!("--msg expects `<pkg>/<Msg>=<path>`, got {raw:?}"))?;
            let (pkg, msg) = split_pkg_msg(left)?;
            Ok::<_, eyre::Report>((pkg, msg, PathBuf::from(source)))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ResolvedArgs {
        idlc,
        include,
        crate_name: args
            .crate_name
            .unwrap_or_else(|| default_crate_name().to_string()),
        out,
        messages,
    })
}

fn default_crate_name() -> &'static str {
    "nros_descriptors"
}

fn split_pkg_msg(s: &str) -> Result<(String, String)> {
    let (p, m) = s
        .split_once('/')
        .ok_or_else(|| eyre::eyre!("--msg expects `<pkg>/<Msg>`, got {s:?}"))?;
    if p.is_empty() || m.is_empty() {
        bail!("--msg expects non-empty `<pkg>/<Msg>`, got {s:?}");
    }
    Ok((p.to_string(), m.to_string()))
}

fn emit(args: &ResolvedArgs) -> Result<()> {
    if !args.idlc.is_file() {
        bail!(
            "--idlc points at {} which is not a file",
            args.idlc.display()
        );
    }
    fs::create_dir_all(&args.out)
        .with_context(|| format!("create --out dir {}", args.out.display()))?;

    let mut entries: Vec<ManifestEntry> = Vec::with_capacity(args.messages.len());
    // Track stems to detect collisions.
    let mut stems: BTreeSet<String> = BTreeSet::new();

    for (pkg, msg, source) in &args.messages {
        let stem = format!("{pkg}_{msg}");
        if !stems.insert(stem.clone()) {
            bail!("duplicate --msg stem `{stem}` (pkg=`{pkg}` msg=`{msg}`)");
        }

        let msg_source = fs::read_to_string(source)
            .with_context(|| format!("read .msg source for {pkg}/{msg} at {}", source.display()))?;
        let idl_text = nros_msg_to_idl::msg_to_idl(&msg_source, pkg, msg)
            .map_err(|e| eyre::eyre!("nros-msg-to-idl({pkg}/{msg}): {e}"))?;

        let idl_path = args.out.join(format!("{stem}.idl"));
        fs::write(&idl_path, &idl_text).with_context(|| format!("write {}", idl_path.display()))?;

        // Shell idlc -t -l c -o <out> <idl-file>. Matches the helper
        // in nros-rmw-cyclonedds-sys/build.rs (Phase 212.K.2).
        let status = Command::new(&args.idlc)
            .args(["-t", "-l", "c", "-o"])
            .arg(&args.out)
            .arg(&idl_path)
            .status()
            .with_context(|| format!("spawn idlc at {}", args.idlc.display()))?;
        if !status.success() {
            bail!("idlc failed on {} (status: {status})", idl_path.display());
        }

        // idlc names outputs after the IDL stem.
        let c_path = args.out.join(format!("{stem}.c"));
        let h_path = args.out.join(format!("{stem}.h"));
        if !c_path.is_file() || !h_path.is_file() {
            bail!(
                "idlc did not emit the expected `{stem}.{{c,h}}` pair in {}",
                args.out.display()
            );
        }

        let type_name = format!("{pkg}::msg::dds_::{msg}_");
        let descriptor_symbol = format!("{pkg}_msg_dds__{msg}__desc");

        entries.push(ManifestEntry {
            pkg: pkg.clone(),
            msg: msg.clone(),
            idl_path,
            c_path,
            h_path,
            type_name,
            descriptor_symbol,
        });
    }

    // Emit register.{c,h}.
    let register_entry = format!("{}_register_descriptors", c_ident(&args.crate_name));
    let register_h = args.out.join("register.h");
    let register_c = args.out.join("register.c");
    fs::write(&register_h, render_register_h(&register_entry))
        .with_context(|| format!("write {}", register_h.display()))?;
    fs::write(&register_c, render_register_c(&register_entry, &entries))
        .with_context(|| format!("write {}", register_c.display()))?;

    let manifest = Manifest {
        crate_name: args.crate_name.clone(),
        register_entry,
        register_c: register_c.clone(),
        register_h: register_h.clone(),
        descriptors: entries,
    };
    let manifest_path = args.out.join("cyclonedds-descriptors.json");
    let body =
        serde_json::to_string_pretty(&manifest).context("serialize cyclonedds-descriptors.json")?;
    fs::write(&manifest_path, body)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    Ok(())
}

fn render_register_h(entry: &str) -> String {
    let mut s = String::new();
    s.push_str("/* Auto-generated by `nros codegen cyclonedds-descriptors`. */\n");
    s.push_str("#ifndef NROS_CYCLONEDDS_DESCRIPTORS_REGISTER_H\n");
    s.push_str("#define NROS_CYCLONEDDS_DESCRIPTORS_REGISTER_H\n\n");
    s.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");
    s.push_str(&format!("void {entry}(void);\n\n"));
    s.push_str("#ifdef __cplusplus\n}\n#endif\n\n");
    s.push_str("#endif /* NROS_CYCLONEDDS_DESCRIPTORS_REGISTER_H */\n");
    s
}

fn render_register_c(entry: &str, descriptors: &[ManifestEntry]) -> String {
    let mut s = String::new();
    s.push_str("/* Auto-generated by `nros codegen cyclonedds-descriptors`. */\n");
    s.push_str("#include \"dds/dds.h\"\n");
    for d in descriptors {
        let stem = format!("{}_{}", d.pkg, d.msg);
        s.push_str(&format!("#include \"{stem}.h\"\n"));
    }
    s.push_str("\n");
    for d in descriptors {
        s.push_str(&format!(
            "extern const dds_topic_descriptor_t {};\n",
            d.descriptor_symbol
        ));
    }
    s.push_str("\n");
    s.push_str(
        "extern void nros_rmw_cyclonedds_register_descriptor(\n    const char *type_name, const dds_topic_descriptor_t *desc);\n\n",
    );
    s.push_str(&format!("void {entry}(void) {{\n"));
    for d in descriptors {
        s.push_str(&format!(
            "    nros_rmw_cyclonedds_register_descriptor(\n        \"{}\",\n        &{});\n",
            c_escape(&d.type_name),
            d.descriptor_symbol,
        ));
    }
    s.push_str("}\n\n");
    // Also expose a constructor-priority hook so a +whole-archive
    // link pulls the call-sites in even when the consumer forgets
    // to invoke the explicit register entry.
    s.push_str(&format!(
        "__attribute__((constructor))\nstatic void {entry}_ctor(void) {{\n    {entry}();\n}}\n"
    ));
    s
}

fn c_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn c_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    fn scratch_dir(test: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("nros-cli-core-tests"));
        let dir = base.join(format!("codegen_cyclonedds_descriptors_{test}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Stub `idlc` that takes `-t -l c -o <out> <idl>` and writes
    /// `<stem>.c` + `<stem>.h` matching the real idlc's emit shape
    /// (just enough for the verb's output checks).
    fn write_stub_idlc(dir: &Path) -> PathBuf {
        let path = dir.join("idlc");
        let script = r#"#!/usr/bin/env bash
set -euo pipefail
out=""
idl=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|-l) shift; shift; continue;;
        -o) out="$2"; shift; shift; continue;;
        *) idl="$1"; shift;;
    esac
done
stem="$(basename "$idl" .idl)"
cat > "${out}/${stem}.c" <<EOF
/* stub idlc output for ${stem} */
const int ${stem}_marker = 1;
EOF
cat > "${out}/${stem}.h" <<EOF
/* stub idlc header for ${stem} */
#ifndef ${stem}_H
#define ${stem}_H
extern const int ${stem}_marker;
#endif
EOF
"#;
        fs::write(&path, script).expect("write stub idlc");
        let mut perm = fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(&path, perm).unwrap();
        path
    }

    /// Verifies CycloneDDS descriptor codegen emits C for `std_msgs/Int32`.
    #[test]
    fn codegen_cyclonedds_emits_std_msgs() {
        let dir = scratch_dir("emits_c_for_std_msgs_int32");
        let stub = write_stub_idlc(&dir);
        let include = dir.join("include");
        fs::create_dir_all(&include).unwrap();
        let msg_path = dir.join("Int32.msg");
        fs::write(&msg_path, "int32 data\n").unwrap();
        let out = dir.join("out");

        run(Args {
            idlc: Some(stub.clone()),
            include: Some(include.clone()),
            msg: vec![format!("std_msgs/Int32={}", msg_path.display())],
            crate_name: Some("nros_example_talker".into()),
            out: Some(out.clone()),
            args_file: None,
        })
        .expect("verb runs");

        assert!(out.join("std_msgs_Int32.idl").is_file());
        assert!(out.join("std_msgs_Int32.c").is_file());
        assert!(out.join("std_msgs_Int32.h").is_file());

        let manifest_path = out.join("cyclonedds-descriptors.json");
        let manifest_body = fs::read_to_string(&manifest_path).unwrap();
        let manifest: Manifest = serde_json::from_str(&manifest_body).unwrap();
        assert_eq!(manifest.crate_name, "nros_example_talker");
        assert_eq!(
            manifest.register_entry,
            "nros_example_talker_register_descriptors"
        );
        assert_eq!(manifest.descriptors.len(), 1);
        let d = &manifest.descriptors[0];
        assert_eq!(d.pkg, "std_msgs");
        assert_eq!(d.msg, "Int32");
        assert_eq!(d.type_name, "std_msgs::msg::dds_::Int32_");
        assert_eq!(d.descriptor_symbol, "std_msgs_msg_dds__Int32__desc");
    }

    #[test]
    fn nros_codegen_cyclonedds_descriptors_emits_register_tu() {
        let dir = scratch_dir("emits_register_tu");
        let stub = write_stub_idlc(&dir);
        let include = dir.join("include");
        fs::create_dir_all(&include).unwrap();
        let msg_path = dir.join("Int32.msg");
        fs::write(&msg_path, "int32 data\n").unwrap();
        let out = dir.join("out");

        run(Args {
            idlc: Some(stub),
            include: Some(include),
            msg: vec![format!("std_msgs/Int32={}", msg_path.display())],
            crate_name: Some("my_app".into()),
            out: Some(out.clone()),
            args_file: None,
        })
        .expect("verb runs");

        let reg_h = fs::read_to_string(out.join("register.h")).unwrap();
        assert!(
            reg_h.contains("void my_app_register_descriptors(void);"),
            "register.h: {reg_h}"
        );

        let reg_c = fs::read_to_string(out.join("register.c")).unwrap();
        assert!(
            reg_c.contains("#include \"std_msgs_Int32.h\""),
            "reg_c: {reg_c}"
        );
        assert!(
            reg_c.contains("void my_app_register_descriptors(void) {"),
            "reg_c: {reg_c}"
        );
        assert!(
            reg_c.contains("nros_rmw_cyclonedds_register_descriptor("),
            "reg_c: {reg_c}"
        );
        assert!(
            reg_c.contains("\"std_msgs::msg::dds_::Int32_\""),
            "reg_c: {reg_c}"
        );
        assert!(
            reg_c.contains("&std_msgs_msg_dds__Int32__desc"),
            "reg_c: {reg_c}"
        );
        assert!(
            reg_c.contains("__attribute__((constructor))"),
            "constructor hook missing: {reg_c}"
        );
    }

    #[test]
    fn nros_codegen_cyclonedds_descriptors_args_file_roundtrip() {
        let dir = scratch_dir("args_file_roundtrip");
        let stub = write_stub_idlc(&dir);
        let include = dir.join("include");
        fs::create_dir_all(&include).unwrap();
        let msg_path = dir.join("Int32.msg");
        fs::write(&msg_path, "int32 data\n").unwrap();
        let out = dir.join("out");

        let args_doc = serde_json::json!({
            "idlc": stub,
            "include": include,
            "crate_name": "from_args_file",
            "out": out,
            "messages": [
                { "name": "std_msgs/Int32", "source": msg_path }
            ]
        });
        let args_path = dir.join("args.json");
        fs::write(&args_path, args_doc.to_string()).unwrap();

        run(Args {
            idlc: None,
            include: None,
            msg: vec![],
            crate_name: None,
            out: None,
            args_file: Some(args_path),
        })
        .expect("verb runs from args-file");

        let manifest_body = fs::read_to_string(out.join("cyclonedds-descriptors.json")).unwrap();
        let manifest: Manifest = serde_json::from_str(&manifest_body).unwrap();
        assert_eq!(manifest.crate_name, "from_args_file");
        assert_eq!(
            manifest.register_entry,
            "from_args_file_register_descriptors"
        );
    }

    /// Verifies CycloneDDS descriptor codegen rejects duplicate message stems.
    #[test]
    fn codegen_cyclonedds_rejects_duplicate_stem() {
        let dir = scratch_dir("rejects_duplicate_msg_stem");
        let stub = write_stub_idlc(&dir);
        let include = dir.join("include");
        fs::create_dir_all(&include).unwrap();
        let msg_path = dir.join("Int32.msg");
        fs::write(&msg_path, "int32 data\n").unwrap();
        let out = dir.join("out");

        let err = run(Args {
            idlc: Some(stub),
            include: Some(include),
            msg: vec![
                format!("std_msgs/Int32={}", msg_path.display()),
                format!("std_msgs/Int32={}", msg_path.display()),
            ],
            crate_name: None,
            out: Some(out),
            args_file: None,
        })
        .unwrap_err();
        let s = format!("{err:#}");
        assert!(
            s.contains("duplicate"),
            "expected duplicate-stem error, got: {s}"
        );
    }
}

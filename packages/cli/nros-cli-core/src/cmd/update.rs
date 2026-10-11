//! RFC-0104 D7 (phase-485 W7) — `nros update`: the one verb that moves a pin.
//!
//! ```text
//! nros update --crate <name> [--precise <v>]
//! nros update --source <name> --ref <sha>
//! nros update --tool <name> --version <v>
//! nros update --west <module> --revision <rev>
//! ```
//!
//! Each edits the pin set — the registry pin file, a `[source.*]` gitlink and
//! `ref`, a `[tool.*]` row, a west manifest — and leaves ONE diff to review.
//! Resolution is cargo's (`cargo update -p --precise`, RFC-0104 D8); nothing
//! here re-implements it.
//!
//! The pin set is writable in a nano-ros CHECKOUT, which is where these verbs
//! edit it. An installed SDK's pin set is read-only; RFC-0104 open question 4
//! answers that a project then records its own overrides (phase-485 W10).

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

use clap::{ArgGroup, Args as ClapArgs};
use eyre::{Result, WrapErr, bail, eyre};

use crate::orchestration::{
    registry_pins::{RegistryPkg, compat_range, parse_registry, regenerate, render, version_key},
    sdk_index::SdkIndex,
};

#[derive(Debug, ClapArgs)]
#[command(group(ArgGroup::new("kind").required(true).args(["pins", "krate", "source", "tool", "west"])))]
pub struct Args {
    /// Regenerate the registry pin set from the locked roots as they stand —
    /// after a `cargo update` or a manifest edit in one root. Disagreements
    /// between roots are settled upward first.
    #[arg(long)]
    pub pins: bool,

    /// Move a Rust registry crate's pin: every locked root that uses it is
    /// re-resolved with `cargo update`, the roots are brought into agreement,
    /// and the pin set is regenerated from them.
    #[arg(long = "crate", value_name = "NAME")]
    pub krate: Option<String>,
    /// The exact version to move to. Without it, cargo picks the newest
    /// semver-compatible one. A different MAJOR is a manifest edit first.
    #[arg(long, value_name = "VERSION", requires = "krate")]
    pub precise: Option<String>,

    /// Move a vendored source (`[source.<name>]`) to a commit: its submodule is
    /// checked out there and the index `ref` rewritten, forward only.
    #[arg(long, value_name = "NAME")]
    pub source: Option<String>,
    /// The commit for `--source`.
    #[arg(long = "ref", value_name = "SHA", requires = "source")]
    pub git_ref: Option<String>,
    /// Let `--source` move a pin BACKWARDS (the submodule-pin rule forbids it
    /// otherwise; say why in the commit).
    #[arg(long, requires = "source")]
    pub allow_rewind: bool,

    /// Move a host tool (`[tool.<name>]`) to a version: the row's version
    /// strings are rewritten and every dist artifact is downloaded to recompute
    /// its sha256.
    #[arg(long, value_name = "NAME")]
    pub tool: Option<String>,
    /// The version for `--tool`.
    #[arg(long = "to-version", value_name = "VERSION", requires = "tool")]
    pub to_version: Option<String>,

    /// Move a Zephyr west project's revision in every manifest that lists it.
    #[arg(long, value_name = "MODULE")]
    pub west: Option<String>,
    /// The revision for `--west`.
    #[arg(long, value_name = "REV", requires = "west")]
    pub revision: Option<String>,

    /// The nano-ros checkout whose pin set to edit (else the usual ladder).
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &cwd)
        .ok_or_else(|| eyre!("no nano-ros root found; pass --nano-ros-path"))?;
    if !root.join(nros_build_paths::CHECKOUT_MARKER).is_file() {
        bail!(
            "{} is an installed SDK, whose pin set is read-only. Moving a pin there is \
             recorded as a project override (RFC-0104 open question 4, phase-485 W10).",
            root.display()
        );
    }
    let index_path = root.join("nros-sdk-index.toml");
    let index = SdkIndex::load(&index_path)?;
    if args.pins {
        update_pins(&root, &index)
    } else if let Some(name) = &args.krate {
        update_crate(&root, &index, name, args.precise.as_deref(), &cwd)
    } else if let Some(name) = &args.source {
        let r = args
            .git_ref
            .as_deref()
            .ok_or_else(|| eyre!("--source needs --ref"))?;
        update_source(&root, &index, &index_path, name, r, args.allow_rewind)
    } else if let Some(name) = &args.tool {
        let v = args
            .to_version
            .as_deref()
            .ok_or_else(|| eyre!("--tool needs --to-version"))?;
        update_tool(&index, &index_path, name, v)
    } else if let Some(module) = &args.west {
        let rev = args
            .revision
            .as_deref()
            .ok_or_else(|| eyre!("--west needs --revision"))?;
        update_west(&root, &index, module, rev)
    } else {
        unreachable!("clap requires one of the kinds")
    }
}

// ---- --pins / --crate -------------------------------------------------------

fn pins_path(root: &Path, index: &SdkIndex) -> Result<(String, PathBuf)> {
    let rel = index
        .rust
        .registry_pins
        .clone()
        .ok_or_else(|| eyre!("the index names no `[rust] registry_pins`"))?;
    let path = root.join(&rel);
    Ok((rel, path))
}

fn write_pins(path: &Path, pins: &[RegistryPkg]) -> Result<()> {
    let text = render(pins);
    if std::fs::read_to_string(path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(path, &text).wrap_err_with(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn update_pins(root: &Path, index: &SdkIndex) -> Result<()> {
    let (rel, path) = pins_path(root, index)?;
    let previous = read_pins(&path)?;
    let roots: Vec<PathBuf> = index
        .rust
        .locked_roots
        .iter()
        .map(|r| root.join(r))
        .collect();
    let next = loop_unify(&roots, root, &previous)?;
    write_pins(&path, &next)?;
    report_moves(&previous, &next, &rel);
    Ok(())
}

fn read_lock(dir: &Path) -> Result<Vec<RegistryPkg>> {
    let p = dir.join("Cargo.lock");
    parse_registry(&std::fs::read_to_string(&p).wrap_err_with(|| format!("read {}", p.display()))?)
}

/// What [`try_cargo_update`] found.
enum Updated {
    Moved,
    /// cargo could not select the version: the manifests do not accept it.
    ManifestRefuses,
}

/// [`cargo_update`], except that "the manifests do not accept that version"
/// is an ANSWER rather than an error — a breaking bump asks it of every root.
fn try_cargo_update(dir: &Path, spec: &str, precise: Option<&str>) -> Result<Updated> {
    let mut c = Command::new("cargo");
    c.current_dir(dir)
        .env("NROS_CARGO_FLAGS", "")
        .args(["update", "-p", spec]);
    if let Some(v) = precise {
        c.args(["--precise", v]);
    }
    let out = c.output().wrap_err("run `cargo update`")?;
    if out.status.success() {
        return Ok(Updated::Moved);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("failed to select a version") {
        return Ok(Updated::ManifestRefuses);
    }
    bail!(
        "`cargo update -p {spec}` in {} failed:\n{}",
        dir.display(),
        err.trim()
    )
}

/// `cargo update` in `dir`, with the PATH shim's `--locked` switched off: moving
/// a lock is the one operation that flag exists to forbid everywhere else.
fn cargo_update(dir: &Path, spec: &str, precise: Option<&str>) -> Result<()> {
    let mut c = Command::new("cargo");
    c.current_dir(dir)
        .env("NROS_CARGO_FLAGS", "")
        .args(["update", "-p", spec]);
    if let Some(v) = precise {
        c.args(["--precise", v]);
    }
    let out = c.output().wrap_err("run `cargo update`")?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let hint = if err.contains("failed to select a version") {
        "\n  A different MAJOR is a manifest edit first: change the requirement in the \
         manifests that name it (`cargo upgrade --incompatible -p <name>` from cargo-edit \
         does it across a workspace), then run this again."
    } else {
        ""
    };
    bail!(
        "`cargo update -p {spec}{}` in {} failed:\n{}{hint}",
        precise
            .map(|v| format!(" --precise {v}"))
            .unwrap_or_default(),
        dir.display(),
        err.trim()
    )
}

fn update_crate(
    root: &Path,
    index: &SdkIndex,
    name: &str,
    precise: Option<&str>,
    project: &Path,
) -> Result<()> {
    let (pins_rel, pins_path) = pins_path(root, index)?;
    let pins_rel = pins_rel.as_str();
    let previous = read_pins(&pins_path)?;
    let roots: Vec<PathBuf> = index
        .rust
        .locked_roots
        .iter()
        .map(|r| root.join(r))
        .collect();
    let want_range = precise.map(compat_range);

    // 1. Every locked root that holds the crate moves. With `--precise` in a
    //    DIFFERENT range than a root holds (a breaking bump), a root whose
    //    manifests do not accept it yet is a STRAGGLER — reported, not an
    //    error (RFC-0104 D7) — unless no root can move at all.
    let mut moved_any = false;
    let mut stragglers: Vec<(PathBuf, String)> = Vec::new();
    for dir in &roots {
        let held: Vec<String> = read_lock(dir)?
            .into_iter()
            .filter(|p| p.name == name)
            .map(|p| p.version)
            .collect();
        if precise.is_some_and(|v| held.iter().any(|h| h == v)) {
            moved_any = true;
            continue;
        }
        for v in held {
            let across = want_range.as_deref().is_some_and(|r| compat_range(&v) != r);
            match try_cargo_update(dir, &format!("{name}@{v}"), precise)? {
                Updated::Moved => moved_any = true,
                Updated::ManifestRefuses if across => stragglers.push((dir.clone(), v)),
                Updated::ManifestRefuses => {
                    cargo_update(dir, &format!("{name}@{v}"), precise)?;
                }
            }
        }
    }
    if !moved_any && !stragglers.is_empty() {
        bail!(
            "no locked root's manifests accept `{name}` {}: every one that uses it is on \
             another major ({}). A breaking bump is a manifest edit first — change the \
             requirement where it is named (`cargo upgrade --incompatible -p {name}` from \
             cargo-edit does a workspace), then run this again.",
            precise.unwrap_or("?"),
            stragglers
                .iter()
                .map(|(d, v)| format!("{} {v}", d.strip_prefix(root).unwrap_or(d).display()))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // 2. A crate no locked root holds lives only in the pin set; it moves
    //    through the project leaf it was asked from.
    let leaf = project.join("Cargo.lock");
    let leaf_is_root = roots.iter().any(|r| same(r, project));
    let mut leaf_pkgs: Option<Vec<RegistryPkg>> = None;
    if !moved_any {
        if leaf_is_root || !leaf.is_file() {
            bail!(
                "no locked root uses `{name}`{}, and {} has no leaf lock to resolve it in. \
                 Run this from a leaf that depends on it.",
                want_range
                    .map(|r| format!(" in range {r}"))
                    .unwrap_or_default(),
                project.display()
            );
        }
        let have: Vec<String> = read_lock(project)?
            .into_iter()
            .filter(|p| p.name == name)
            .map(|p| p.version)
            .collect();
        if have.is_empty() {
            bail!("{} does not use `{name}`", project.display());
        }
        for v in have {
            cargo_update(project, &format!("{name}@{v}"), precise)?;
        }
        leaf_pkgs = Some(read_lock(project)?);
    }

    // 3. The locked roots must agree within every range: a cargo update can
    //    move a crate's DEPENDENCIES too, and only in the roots that ran it.
    let new_pins = loop_unify(&roots, root, &previous)?;
    let mut new_pins = new_pins;
    if let Some(lp) = leaf_pkgs {
        let held: BTreeSet<(String, String)> = roots
            .iter()
            .flat_map(|d| read_lock(d).unwrap_or_default())
            .map(|p| (p.name.clone(), compat_range(&p.version)))
            .collect();
        let mut by_key: BTreeMap<(String, String), RegistryPkg> = new_pins
            .into_iter()
            .map(|p| ((p.name.clone(), compat_range(&p.version)), p))
            .collect();
        for p in lp {
            let k = (p.name.clone(), compat_range(&p.version));
            if !held.contains(&k) {
                by_key.insert(k, p);
            }
        }
        new_pins = by_key.into_values().collect();
    }

    // 4. Write, and say what moved.
    write_pins(&pins_path, &new_pins)?;
    report_moves(&previous, &new_pins, pins_rel);
    let majors: BTreeSet<String> = new_pins
        .iter()
        .filter(|p| p.name == name)
        .map(|p| p.version.clone())
        .collect();
    if majors.len() > 1 {
        println!(
            "nros update: `{name}` is pinned in {} semver ranges ({}). That is allowed \
             (transitive deps make some unavoidable); `cargo tree -d` in a root shows who \
             holds each.",
            majors.len(),
            majors.into_iter().collect::<Vec<_>>().join(", ")
        );
    }

    for (d, v) in &stragglers {
        println!(
            "nros update: {} still resolves `{name}` {v} — its manifests have not moved to {}.",
            d.strip_prefix(root).unwrap_or(d).display(),
            precise.unwrap_or("?")
        );
    }

    // 5. The project's own (seeded) leaf lock follows the moved pins.
    if !leaf_is_root && leaf.is_file() {
        realign_leaf(project, &new_pins)?;
    }
    println!("nros update: review `git diff -- '*Cargo.lock' {pins_rel}`, then `just ci gate`.");
    Ok(())
}

fn same(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    c(a) == c(b)
}

fn read_pins(path: &Path) -> Result<Vec<RegistryPkg>> {
    match std::fs::read_to_string(path) {
        Ok(t) => parse_registry(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).wrap_err_with(|| format!("read {}", path.display())),
    }
}

/// Regenerate the pin set, first moving the lower side of every disagreement
/// between locked roots up to the higher (a few rounds: a move can itself move
/// a dependency).
fn loop_unify(
    roots: &[PathBuf],
    base: &Path,
    previous: &[RegistryPkg],
) -> Result<Vec<RegistryPkg>> {
    for _ in 0..4 {
        let locks: Vec<(String, Vec<RegistryPkg>)> = roots
            .iter()
            .map(|d| {
                let rel = d.strip_prefix(base).unwrap_or(d).display().to_string();
                Ok((rel, read_lock(d)?))
            })
            .collect::<Result<_>>()?;
        match regenerate(&locks, previous) {
            Ok(p) => return Ok(p),
            Err(split) => {
                for d in split {
                    let hi = d
                        .versions
                        .iter()
                        .map(|(_, v)| v.clone())
                        .max_by_key(|v| version_key(v))
                        .unwrap_or_default();
                    for (lock, v) in &d.versions {
                        if *v != hi {
                            cargo_update(&base.join(lock), &format!("{}@{v}", d.name), Some(&hi))?;
                        }
                    }
                }
            }
        }
    }
    bail!("the locked roots still disagree after four rounds of `cargo update`; resolve by hand")
}

fn report_moves(previous: &[RegistryPkg], next: &[RegistryPkg], pins_rel: &str) {
    let key = |p: &RegistryPkg| (p.name.clone(), compat_range(&p.version));
    let before: BTreeMap<_, _> = previous
        .iter()
        .map(|p| (key(p), p.version.clone()))
        .collect();
    let mut n = 0;
    for p in next {
        match before.get(&key(p)) {
            Some(v) if *v == p.version => {}
            Some(v) => {
                println!("  {} {v} -> {}", p.name, p.version);
                n += 1;
            }
            None => {
                println!("  {} {} (new pin)", p.name, p.version);
                n += 1;
            }
        }
    }
    println!("nros update: {n} pin(s) moved in {pins_rel}");
}

/// Move every registry entry of a (seeded, untracked) leaf lock that is off the
/// pin set back onto it. Used after a pin moves, and named by the drift check.
pub fn realign_leaf(dir: &Path, pins: &[RegistryPkg]) -> Result<()> {
    let pinned: BTreeMap<(String, String), String> = pins
        .iter()
        .map(|p| {
            (
                (p.name.clone(), compat_range(&p.version)),
                p.version.clone(),
            )
        })
        .collect();
    for p in read_lock(dir)? {
        if let Some(want) = pinned.get(&(p.name.clone(), compat_range(&p.version)))
            && *want != p.version
        {
            cargo_update(dir, &format!("{}@{}", p.name, p.version), Some(want))?;
        }
    }
    Ok(())
}

// ---- --source ---------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    crate::source_stamp::nros_git_command("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .wrap_err("run git")
}

fn update_source(
    root: &Path,
    index: &SdkIndex,
    index_path: &Path,
    name: &str,
    new: &str,
    allow_rewind: bool,
) -> Result<()> {
    let row = index
        .source
        .get(name)
        .ok_or_else(|| eyre!("no [source.{name}] in the index"))?;
    let old = row.git_ref.clone().unwrap_or_default();
    if let Some(sub) = row.submodule.as_deref() {
        let dir = root.join(sub);
        let have = |rev: &str| {
            git(&dir, &["cat-file", "-e", &format!("{rev}^{{commit}}")])
                .is_ok_and(|o| o.status.success())
        };
        if !have(new) {
            git(&dir, &["fetch", "origin", new])?;
            if !have(new) {
                bail!(
                    "{new} is not a commit in {} (fetched origin)",
                    dir.display()
                );
            }
        }
        let full = String::from_utf8_lossy(&git(&dir, &["rev-parse", new])?.stdout)
            .trim()
            .to_string();
        if !old.is_empty() && !allow_rewind {
            match crate::source_stamp::nros_git_ancestry(&dir, &old, &full) {
                Some(true) => {}
                Some(false) => bail!(
                    "{full} is not a descendant of the current pin {old}: a submodule pin moves \
                     FORWARD only (`check-submodule-pins`). Pass --allow-rewind and say why."
                ),
                None => bail!(
                    "cannot tell whether {full} descends from the current pin {old}: {} has a \
                     truncated history (issue 1476). `git -C {} fetch --unshallow`, then retry.",
                    dir.display(),
                    dir.display()
                ),
            }
        }
        let co = git(&dir, &["checkout", "--detach", &full])?;
        if !co.status.success() {
            bail!(
                "checkout failed: {}",
                String::from_utf8_lossy(&co.stderr).trim()
            );
        }
        rewrite_row_key(index_path, &format!("source.{name}"), "ref", &full)?;
        println!(
            "nros update: {name} {old} -> {full}\n  stage both: git add {sub} {}",
            index_path.file_name().unwrap_or_default().to_string_lossy()
        );
    } else {
        rewrite_row_key(index_path, &format!("source.{name}"), "ref", new)?;
        println!("nros update: {name} {old} -> {new} (clone-mode: the index `ref` is the pin)");
    }
    Ok(())
}

/// Rewrite `key = "<value>"` inside the `[table]` row of a TOML file, keeping
/// every other byte (comments and layout are the review).
pub(crate) fn rewrite_row_key(path: &Path, table: &str, key: &str, value: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let header = format!("[{table}]");
    let mut in_row = false;
    let mut done = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') {
            in_row = t == header;
        }
        if in_row && !done && t.starts_with(&format!("{key} =")) {
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push_str(&format!("{indent}{key} = \"{value}\"\n"));
            done = true;
            continue;
        }
        out.push_str(line);
    }
    if !done {
        bail!("no `{key} =` in [{table}] of {}", path.display());
    }
    std::fs::write(path, out)?;
    Ok(())
}

// ---- --tool -----------------------------------------------------------------

fn update_tool(index: &SdkIndex, index_path: &Path, name: &str, new: &str) -> Result<()> {
    let row = index
        .tool
        .get(name)
        .ok_or_else(|| eyre!("no [tool.{name}] in the index"))?;
    let old = row.version.clone();
    if old == new {
        println!("nros update: {name} is already {new}");
        return Ok(());
    }
    let text = std::fs::read_to_string(index_path)?;
    let header = format!("[tool.{name}]");
    let sub = format!("[tool.{name}.");
    let mut in_row = false;
    let mut out = String::with_capacity(text.len());
    let mut urls = Vec::new();
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') {
            in_row = t == header;
            if t.starts_with(&sub) {
                println!(
                    "nros update: note — {t} (a recipe) still names its own sources; update it by hand"
                );
            }
        }
        if in_row && !t.starts_with('#') && line.contains(old.as_str()) {
            let moved = line.replace(old.as_str(), new);
            if let Some(u) = between(&moved, "url = \"", "\"") {
                urls.push(u);
            }
            out.push_str(&moved);
        } else {
            out.push_str(line);
        }
    }
    for url in urls {
        let tmp = tempfile::NamedTempFile::new()?;
        crate::orchestration::sdk_store::download(&url, tmp.path())
            .wrap_err_with(|| format!("download {url} to hash it"))?;
        let sha = crate::orchestration::sdk_store::sha256_file(tmp.path())?;
        out = replace_sha_after_url(&out, &url, &sha);
        println!("  {url}\n    sha256 = {sha}");
    }
    std::fs::write(index_path, out)?;
    println!(
        "nros update: {name} {old} -> {new}. Re-measure each dist row's `floor` \
         (`scripts/sdk/measure-dist-floor.py`) before committing (RFC-0099 D5)."
    );
    Ok(())
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)? + i;
    Some(s[i..j].to_string())
}

/// In the line carrying `url`, replace the `sha256 = "…"` value.
fn replace_sha_after_url(text: &str, url: &str, sha: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            if line.contains(url)
                && let Some(old) = between(line, "sha256 = \"", "\"")
            {
                return line.replace(
                    &format!("sha256 = \"{old}\""),
                    &format!("sha256 = \"{sha}\""),
                );
            }
            line.to_string()
        })
        .collect()
}

// ---- --west -----------------------------------------------------------------

fn update_west(root: &Path, index: &SdkIndex, module: &str, rev: &str) -> Result<()> {
    if index.west.manifests.is_empty() {
        bail!("the index names no `[west] manifests`");
    }
    let mut edited = Vec::new();
    for m in &index.west.manifests {
        let path = root.join(m);
        let text =
            std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
        if let Some(new) = rewrite_west_revision(&text, module, rev) {
            std::fs::write(&path, new)?;
            edited.push(m.clone());
        }
    }
    if edited.is_empty() {
        bail!(
            "no manifest lists `{module}` as a project with a revision. A module the \
             zephyr manifest IMPORTS (cmsis, picolibc, mbedtls …) follows `zephyr`'s \
             revision; move `--west zephyr` instead."
        );
    }
    println!("nros update: {module} -> {rev} in {}", edited.join(", "));
    Ok(())
}

/// The `revision:` of the `- name: <module>` project, rewritten; `None` when
/// the manifest does not list that project.
fn rewrite_west_revision(text: &str, module: &str, rev: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut in_proj = false;
    let mut done = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with("- name:") {
            in_proj = t.trim_start_matches("- name:").trim() == module;
        }
        if in_proj && !done && t.starts_with("revision:") {
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push_str(&format!("{indent}revision: {rev}\n"));
            done = true;
            continue;
        }
        out.push_str(line);
    }
    done.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_key_is_rewritten_in_its_own_row_only() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("i.toml");
        std::fs::write(
            &p,
            "[source.a]\nref = \"1\" # keep me? no, the value line is ours\n\n[source.b]\n# c\nref = \"2\"\n",
        )
        .unwrap();
        rewrite_row_key(&p, "source.b", "ref", "3").unwrap();
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains("[source.a]\nref = \"1\""), "{s}");
        assert!(s.contains("# c\nref = \"3\"\n"), "{s}");
        assert!(rewrite_row_key(&p, "source.c", "ref", "x").is_err());
    }

    #[test]
    fn a_west_revision_moves_only_for_the_named_project() {
        let m = "  projects:\n    - name: zephyr\n      revision: v3.7.0\n      import:\n        name-allowlist:\n          - cmsis\n    - name: zephyr-lang-rust\n      revision: abc\n      path: x\n";
        let z = rewrite_west_revision(m, "zephyr-lang-rust", "def").unwrap();
        assert!(z.contains("revision: v3.7.0"));
        assert!(z.contains("      revision: def\n"));
        assert!(
            rewrite_west_revision(m, "cmsis", "x").is_none(),
            "an imported module has no revision here"
        );
    }

    #[test]
    fn a_tool_sha_is_replaced_on_the_url_line() {
        let t = "dist.a = { url = \"https://x/1.2/a.zip\", sha256 = \"old\" }\ndist.b = { url = \"https://x/1.2/b.zip\", sha256 = \"keep\" }\n";
        let o = replace_sha_after_url(t, "https://x/1.2/a.zip", "new");
        assert!(o.contains("a.zip\", sha256 = \"new\""));
        assert!(o.contains("b.zip\", sha256 = \"keep\""));
    }
}

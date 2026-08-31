//! phase-351 W5 — resolve one deploy's board FACTS + SITE config, as env lines.
//!
//! [RFC-0072](../../../../../docs/design/0072-rtos-integration-nano-ros-is-a-guest.md)
//! §5 splits board information into A (board facts, in the board package), B
//! (site config, in the user's `[board_config.<board>]`) and C (test harness).
//! W1–W4 gave both halves a home and a validity domain. This is DELIVERY: the
//! resolved pair, printed in one shape, for whoever is about to invoke cargo.
//!
//! **Why the invoker and not the leaf.** Cargo config is discovered from the
//! invocation CWD upward, and corrosion runs cargo from `workspace_toml_dir` —
//! so a leaf's own `.cargo/config.toml` is never read for a workspace member
//! (phase-349 W2.0 measured exactly this, and it is why the `NROS_BOARD_TOML`
//! row that wave added could not reach the members it was written for). The
//! process environment is the only carrier that crosses that boundary, and the
//! thing that owns it is whoever spawns cargo: cmake, a `just` recipe, or west.
//!
//! **One board per emission.** Exactly one board is active per configure — the
//! cmake side selects it with `if/elseif` on `NANO_ROS_BOARD` and the toolchain
//! file must precede `project()` — so this prints ONE deploy's facts, never a
//! table the caller has to index.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use clap::Args as ClapArgs;
use eyre::{Result, eyre};

use crate::orchestration::{
    board_descriptor::{BoardCatalog, BoardDescriptor},
    site_config::SiteConfig,
};

#[derive(Debug, ClapArgs)]
pub struct BoardFactsArgs {
    /// Workspace (or bringup) dir to read `system.toml` from. Defaults to cwd.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Deploy name to resolve. Defaults to the only one, or the one whose
    /// `board` matches `--board`.
    #[arg(long)]
    pub deploy: Option<String>,

    /// Board name, when the deploy is identified by its board rather than its
    /// key (what the cmake lane knows — `NANO_ROS_BOARD`).
    #[arg(long)]
    pub board: Option<String>,

    /// nano-ros checkout holding `packages/boards`. Defaults to `NROS_REPO_DIR`
    /// then an upward search.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
}

/// The resolved facts, in emission order.
///
/// `BTreeMap` rather than a struct with named fields: the set is open (each
/// `sdk.*` and `config_files.*` key becomes its own variable) and every
/// consumer wants the same "export these" shape.
pub fn resolve(
    ws: &Path,
    nano_ros_root: &Path,
    deploy: Option<&str>,
    board: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<BTreeMap<String, String>> {
    // Several blocks may be in scope when the caller named neither a deploy
    // nor a board. Two naming the SAME board can no longer disagree — the site
    // table is keyed by board (issue 0951) — so what survives here is the
    // genuinely ambiguous case: blocks on DIFFERENT boards, where answering
    // with either one would hand the build another board's SDK roots.
    let (candidates, site, origin) = pick_deploys(ws, nano_ros_root, deploy, board)?;
    let mut resolved: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    for (name, board) in candidates {
        let facts = resolve_one(&origin, nano_ros_root, &name, &board, &site, env)?;
        resolved.push((name, facts));
    }
    if let Some((first_name, first)) = resolved.first()
        && let Some((other_name, other)) = resolved.iter().find(|(_, f)| f != first)
    {
        let differing: Vec<String> = first
            .keys()
            .chain(other.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|k| first.get(*k) != other.get(*k))
            .cloned()
            .collect();
        return Err(eyre!(
            "deploys `{first_name}` and `{other_name}` resolve DIFFERENTLY ({}); \
             pass --deploy or --board to say which one this build is",
            differing.join(", ")
        ));
    }
    resolved
        .into_iter()
        .next()
        .map(|(_, f)| f)
        .ok_or_else(|| eyre!("{}: no [deploy.*] blocks", ws.display()))
}

/// Do two spellings name the SAME board?
///
/// Compared by `names`, which is per-ENTRY, and NOT by `source`, which is the
/// descriptor FILE. `packages/boards/nros-board-nuttx/nros-board.toml` declares
/// two distinct boards — `nuttx-qemu-arm` and `nuttx-qemu-riscv` — so a
/// file-level compare answers "yes" for two boards that differ in ISA, and
/// `--board nuttx-qemu-riscv` silently resolves the arm board's facts. That is
/// the same collapse `check-site-config.py`'s alias map has to avoid, and it is
/// worth stating in both places: a directory is not a board.
fn same_board(a: &BoardDescriptor, b: &BoardDescriptor) -> bool {
    a.names == b.names
}

/// Find the `[board_config.<key>]` block that describes `descriptor`.
///
/// The key is matched by RESOLUTION, not by text: `resolve_board` is the same
/// rule `[deploy.*].board` and `[image.*].board` go through (issue 0606), so
/// every legal spelling of one board finds the same block. Two keys resolving
/// to one descriptor is an authoring mistake — it is the duplicate-fact shape
/// this table exists to remove — so it is refused rather than silently
/// order-dependent.
fn site_for_board<'a>(
    table: &'a BTreeMap<String, toml::Value>,
    catalog: &BoardCatalog,
    descriptor: &BoardDescriptor,
) -> Option<(&'a String, &'a toml::Value)> {
    table
        .iter()
        .find(|(key, _)| resolve_board(catalog, key).is_some_and(|d| same_board(d, descriptor)))
}

fn resolve_one(
    origin: &str,
    nano_ros_root: &Path,
    deploy_name: &str,
    board_name: &str,
    site_table: &BTreeMap<String, toml::Value>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<BTreeMap<String, String>> {
    let deploy_name = deploy_name.to_string();
    let board_name = board_name.to_string();

    let catalog = BoardCatalog::load(nano_ros_root)
        .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
    let descriptor = resolve_board(&catalog, &board_name).ok_or_else(|| {
        eyre!(
            "no board descriptor claims `{board_name}` (deploy `{deploy_name}`). \
             Descriptors are matched by their `names` and by their directory \
             (`packages/boards/nros-board-<name>`)."
        )
    })?;

    // The site block is keyed by BOARD (issue 0951), and a board has several
    // legal spellings — the descriptor's `names`, its directory, its
    // downstream framework id. Matching the key TEXTUALLY would make
    // `[board_config."qemu-armv7a-nsh"]` invisible to a build that spelled the
    // same board `nuttx-qemu-arm`, which is issue 0606 one table over. So
    // resolve both sides through the catalog and compare DESCRIPTORS.
    let (site_section, site) = match site_for_board(site_table, &catalog, descriptor) {
        Some((key, value)) => {
            let section = format!("board_config.{key}");
            (
                section.clone(),
                SiteConfig::from_value(value, &section, origin)?,
            )
        }
        None => (format!("board_config.{board_name}"), SiteConfig::default()),
    };

    let mut out = BTreeMap::new();

    // The board rung itself (RFC-0049): the descriptor's own path, which is
    // what `nros-zpico-build` reads for per-board knob deltas. Delivered HERE
    // rather than from a leaf `[env]` row, which is what phase-351 W6 retires.
    if let Some(src) = descriptor.source.as_deref() {
        out.insert(
            "NROS_BOARD_TOML".into(),
            nano_ros_root.join(src).display().to_string(),
        );
    }
    out.insert("NROS_BOARD".into(), board_name.clone());

    // phase-400 W6 — the board's PLATFORM, by name.
    //
    // `nros-node` deliberately has no `platform-*` cargo feature (phase-248 C2:
    // the core executor is platform-agnostic and reaches the platform through
    // the vtable), so its build script cannot know which platform it is
    // compiling for — and without that it cannot resolve the platform rung of
    // the RFC-0049 knob ladder. The descriptor knows; this is the seam that has
    // already resolved it.
    //
    // A NAME, not a path: `NROS_PLATFORM_NAME`, not `NROS_PLATFORM`, because
    // cmake's `-DNROS_PLATFORM=cffi` names the platform LAYER, a different
    // axis, and one env var meaning two things is how they start disagreeing.
    out.insert(
        "NROS_PLATFORM_NAME".into(),
        descriptor.platform.kebab().to_string(),
    );

    // W4 — the netstack, validated against the board's declared domain. An
    // unsupported pair fails HERE, at the seam that knows both halves, rather
    // than as a link error inside a stack nobody selected.
    if let Some(stack) = descriptor
        .resolve_netstack(site.netstack.as_deref())
        .map_err(|e| eyre!("{e}"))?
    {
        out.insert("NROS_NETSTACK".into(), stack.to_string());
    }

    for (name, raw) in &site.sdk {
        let r = site.interpolate(raw, &site_section, origin, env)?;
        out.insert(format!("NROS_SDK_{}", env_key(name)), r.value);
    }
    for (role, raw) in &site.config_files {
        let r = site.interpolate(raw, &site_section, origin, env)?;
        out.insert(format!("NROS_CONFIG_FILE_{}", env_key(role)), r.value);
    }
    if !site.include_dirs.is_empty() {
        let mut dirs = Vec::new();
        for raw in &site.include_dirs {
            dirs.push(site.interpolate(raw, &site_section, origin, env)?.value);
        }
        out.insert("NROS_INCLUDE_DIRS".into(), dirs.join(";"));
    }
    if !site.defines.is_empty() {
        out.insert("NROS_DEFINES".into(), site.defines.join(";"));
    }
    for (key, raw) in &site.upload {
        let r = site.interpolate(raw, &site_section, origin, env)?;
        out.insert(format!("NROS_UPLOAD_{}", env_key(key)), r.value);
    }
    Ok(out)
}

/// `freertos_plus_tcp` / `nuttx-apps` → `FREERTOS_PLUS_TCP` / `NUTTX_APPS`.
fn env_key(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Descriptor for a `[deploy.*].board` value — the catalog's ONE rule.
///
/// issue 0606: this used to carry its own `names`-then-directory fallback,
/// one of three such opinions in the tree. `BoardCatalog::resolve_deploy` owns
/// the rule now (names, then the directory alias, then the platform), so this
/// is a thin adapter that keeps the error shape callers expect.
pub fn resolve_board<'a>(catalog: &'a BoardCatalog, board: &str) -> Option<&'a BoardDescriptor> {
    match catalog.resolve_deploy(board) {
        crate::orchestration::board_descriptor::DeployResolution::Board(d) => Some(d),
        _ => None,
    }
}

/// One candidate: the name it was selected by, and the board it resolves to.
///
/// A BOARD, not the whole block: since the site table became board-keyed
/// (issue 0951) the board is the only thing `resolve_one` needs, and carrying
/// the deploy block instead is what kept images invisible here — the board can
/// come from `[image.<id>]` just as well, and in a migrated workspace that is
/// the ONLY place it comes from.
type PickedDeploy = (String, String);

/// A STANDALONE leaf's deploy, from its `Cargo.toml`.
///
/// RFC-0072 §5's site config has two homes, and this is the second: a copy-out
/// example is not a workspace, has no bringup, and declares its target as
/// `[package.metadata.nros.entry] deploy = "<board>"` with the per-deploy block
/// beside it. The deploy KEY is the board there — those manifests carry no
/// `board =` key at all — which is why this maps one onto the other rather than
/// looking for a field that does not exist.
fn deploys_from_manifest(ws: &Path) -> Result<Option<Vec<PickedDeploy>>> {
    let manifest = ws.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&manifest)
        .map_err(|e| eyre!("read {}: {e}", manifest.display()))?;
    let doc: toml::Value =
        toml::from_str(&raw).map_err(|e| eyre!("{}: {e}", manifest.display()))?;
    let Some(nros) = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
    else {
        return Ok(None);
    };
    // `[package.metadata.nros.entry] deploy` when the leaf declares an entry;
    // otherwise the single `[package.metadata.nros.deploy.<key>]` table. The
    // Zephyr examples are the second shape — they carry the deploy block with
    // no `entry` stanza — and requiring the first made them resolve to nothing
    // (issue 0605: that is the lane this wave was trying to reach).
    let deploy_tbl = nros.get("deploy").and_then(|d| d.as_table());
    let deploy_key: String = match nros
        .get("entry")
        .and_then(|e| e.get("deploy"))
        .and_then(|d| d.as_str())
    {
        Some(k) => k.to_string(),
        None => match deploy_tbl {
            Some(t) if t.len() == 1 => t.keys().next().expect("len == 1").clone(),
            // Several, and nothing says which this build is: that is a question
            // for the caller (`--deploy`), not a guess.
            _ => return Ok(None),
        },
    };
    // The deploy KEY is the board here, so the candidate is (key, key). The
    // per-block `nros` sub-table this used to read is gone with issue 0951 —
    // and it was already unreachable: `DeployTargetMetadata` is
    // `deny_unknown_fields` and declares no such field, and no leaf manifest in
    // the tree carries one.
    let board = deploy_key.clone();
    Ok(Some(vec![(deploy_key, board)]))
}

/// The picked deploys, the site table to resolve them against, and the file
/// that supplied both (for error text).
type Picked = (Vec<PickedDeploy>, BTreeMap<String, toml::Value>, String);

fn pick_deploys(
    ws: &Path,
    nano_ros_root: &Path,
    deploy: Option<&str>,
    board: Option<&str>,
) -> Result<Picked> {
    // A standalone leaf first: it has a Cargo.toml and no bringup, and asking
    // for a system.toml there would fail naming a file that is not supposed to
    // exist. Such a leaf carries no site table — it gets the board rung only.
    if let Some(from_manifest) = deploys_from_manifest(ws)? {
        return Ok((from_manifest, BTreeMap::new(), ws.display().to_string()));
    }
    let (path, system) = load_system_toml(ws)?;
    let origin = path.display().to_string();
    let site = system.board_config.clone();
    // Candidates come from `[image.*]` AND `[deploy.*]`, because either can
    // name the board and a migrated workspace has only the first. Reading
    // deploys alone is what made `examples/workspaces/realtime-rust` — seven
    // images, zero deploy blocks — unresolvable while its site table demanded
    // SDK roots: the gate and this resolver disagreeing about which boards a
    // workspace builds for.
    //
    // A name present in both tables is ONE candidate, with the image's board
    // winning: mid-migration the two can disagree, and the image is the half
    // that survives.
    let mut boards: BTreeMap<String, String> = BTreeMap::new();
    for (name, dt) in &system.deploy {
        if let Some(b) = &dt.board {
            boards.insert(name.clone(), b.clone());
        }
    }
    for name in system.image.keys() {
        if let Some(b) = system.image_for(name).and_then(|i| i.board) {
            boards.insert(name.clone(), b);
        }
    }

    let mut candidates: Vec<PickedDeploy> = boards.into_iter().collect();
    if let Some(want) = deploy {
        candidates.retain(|(k, _)| k == want);
        if candidates.is_empty() {
            return Err(eyre!(
                "{}: no [image.{want}] or [deploy.{want}] naming a board",
                path.display()
            ));
        }
    } else if let Some(want) = board {
        // Match by RESOLUTION, not by text. A board has several legal
        // spellings, and which one an author used is not a fact about the
        // build: `examples/workspaces/realtime-rust` writes `board =
        // "freertos"` on its image while cmake passes the directory spelling
        // `mps2-an385-freertos`, and `check-site-config` canonicalises both to
        // the same block. A textual compare here makes the gate and the
        // resolver disagree about which boards a workspace builds for — issue
        // 0606's failure, one table over.
        let catalog = BoardCatalog::load(nano_ros_root)
            .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
        let wanted = resolve_board(&catalog, want).map(|d| d.names.clone());
        candidates.retain(|(_, b)| {
            b == want
                || (wanted.is_some()
                    && resolve_board(&catalog, b).map(|d| d.names.clone()) == wanted)
        });
        if candidates.is_empty() {
            return Err(eyre!(
                "{}: no [image.*] or [deploy.*] with board = \"{want}\" \
                 (matched through the board catalog, so every spelling of one \
                 board is tried)",
                path.display()
            ));
        }
    }
    if candidates.is_empty() {
        return Err(eyre!(
            "{}: no [image.*] or [deploy.*] names a board",
            path.display()
        ));
    }
    Ok((candidates, site, origin))
}

fn load_system_toml(
    ws: &Path,
) -> Result<(
    PathBuf,
    crate::orchestration::cargo_metadata_schema::SystemToml,
)> {
    let mut found: Vec<PathBuf> = Vec::new();
    let direct = ws.join("system.toml");
    if direct.is_file() {
        found.push(direct);
    } else if let Ok(entries) = std::fs::read_dir(ws.join("src")) {
        for e in entries.flatten() {
            let p = e.path().join("system.toml");
            if p.is_file() {
                found.push(p);
            }
        }
    }
    found.sort();
    let path = match found.len() {
        0 => {
            return Err(eyre!(
                "no system.toml at {} or {}/src/*/",
                ws.display(),
                ws.display()
            ));
        }
        1 => found.remove(0),
        _ => {
            return Err(eyre!(
                "{}: {} bringups carry a system.toml; point --path at one: {}",
                ws.display(),
                found.len(),
                found
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    let raw = std::fs::read_to_string(&path).map_err(|e| eyre!("read {}: {e}", path.display()))?;
    let system = toml::from_str(&raw).map_err(|e| eyre!("{}: {e}", path.display()))?;
    Ok((path, system))
}

pub fn run(args: BoardFactsArgs) -> Result<()> {
    let ws = args
        .path
        .canonicalize()
        .map_err(|e| eyre!("{}: {e}", args.path.display()))?;
    let root = args
        .nano_ros_path
        .or_else(|| std::env::var_os("NROS_REPO_DIR").map(PathBuf::from))
        .or_else(|| crate::cmd::ws::autodetect_nano_ros_path(&ws))
        .ok_or_else(|| eyre!("no nano-ros checkout found (pass --nano-ros-path)"))?;

    let facts = resolve(
        &ws,
        &root,
        args.deploy.as_deref(),
        args.board.as_deref(),
        &|k| std::env::var(k).ok(),
    )?;
    for (k, v) in facts {
        println!("{k}={v}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws_with(system: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("system.toml"), system).unwrap();
        d
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            // packages/cli/nros-cli-core -> cli -> packages -> repo root
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    const FREERTOS_WS: &str = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[deploy.freertos]
board = "mps2-an385-freertos"
rmw = "zenoh"

[board_config."mps2-an385-freertos"]
netstack = "lwip"
sdk = { freertos = "{env:FREERTOS_DIR}", lwip = "{env:LWIP_DIR}" }
"#;

    /// The whole point of the wave: one call, both halves, in the shape a
    /// cargo invoker can export.
    #[test]
    fn resolves_board_rung_and_site_config_together() {
        let ws = ws_with(FREERTOS_WS);
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let facts = resolve(ws.path(), &repo_root(), None, None, &env).expect("resolves");

        assert_eq!(
            facts.get("NROS_BOARD").map(String::as_str),
            Some("mps2-an385-freertos")
        );
        assert_eq!(facts.get("NROS_NETSTACK").map(String::as_str), Some("lwip"));
        assert_eq!(
            facts.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/opt/freertos")
        );
        assert_eq!(
            facts.get("NROS_SDK_LWIP").map(String::as_str),
            Some("/opt/lwip")
        );
        // The board rung's own carrier — an ABSOLUTE path, because the consumer
        // is a build script whose CWD is its own crate, not the leaf.
        let toml_path = facts.get("NROS_BOARD_TOML").expect("board rung delivered");
        assert!(toml_path.ends_with("nros-board.toml"), "{toml_path}");
        assert!(Path::new(toml_path).is_absolute(), "{toml_path}");
        assert!(Path::new(toml_path).is_file(), "{toml_path} does not exist");
    }

    /// W4's domain is enforced at THIS seam — the one place that holds the
    /// board descriptor and the deploy's request at the same time.
    #[test]
    fn an_unsupported_netstack_fails_the_resolution() {
        let ws = ws_with(&FREERTOS_WS.replace("\"lwip\"", "\"netxduo\""));
        let err = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect_err("netxduo is not in the freertos board's domain");
        let msg = format!("{err:#}");
        assert!(msg.contains("netxduo") && msg.contains("lwip"), "{msg}");
    }

    /// An unset `{env:VAR}` is an error, not an empty string: a silently blank
    /// SDK root becomes a compile failure inside a vendor tree.
    #[test]
    fn an_unset_env_reference_is_refused() {
        let ws = ws_with(FREERTOS_WS);
        let err = resolve(ws.path(), &repo_root(), None, None, &|_| None).expect_err("no env set");
        assert!(format!("{err:#}").contains("FREERTOS_DIR"), "{err:#}");
    }

    /// Two deploy blocks on the same board are ONE answer — `examples/workspaces/mixed`
    /// ships exactly that pair, and refusing it would block every cmake build of
    /// that workspace over a distinction with no consequence.
    ///
    /// Under a board-keyed site table they cannot even differ: both reach the
    /// same `[board_config.*]` block. This used to need a per-block site table
    /// on each deploy, which is the duplication issue 0951 removed.
    #[test]
    fn duplicate_deploys_that_agree_are_not_ambiguous() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[deploy.mps2-an385-freertos]\nboard = \"mps2-an385-freertos\"\n\
             rmw = \"zenoh\"\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let facts = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("mps2-an385-freertos"),
            &env,
        )
        .expect("agreeing duplicates resolve");
        assert_eq!(facts.get("NROS_NETSTACK").map(String::as_str), Some("lwip"));
    }

    /// …and DISAGREEING duplicates are no longer REFUSED — they are
    /// UNREPRESENTABLE.
    ///
    /// Two deploy blocks naming one board used to carry a site table each, so
    /// they could state different SDK roots for the same hardware; `resolve`
    /// compared the resolutions and refused a conflict. Keying the table by
    /// board deletes the shape instead of detecting it, which is why the
    /// agree/disagree comparison in `resolve` now has nothing to catch. This
    /// test is what says that is deliberate rather than a lost check.
    #[test]
    fn two_deploys_on_one_board_cannot_disagree() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[deploy.other]\nboard = \"mps2-an385-freertos\"\nrmw = \"zenoh\"\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let by_board = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("mps2-an385-freertos"),
            &env,
        )
        .expect("one board, one answer");
        let by_name =
            resolve(ws.path(), &repo_root(), Some("other"), None, &env).expect("named deploy");
        assert_eq!(by_board, by_name, "the board decides, not the deploy key");
        assert_eq!(
            by_board.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/opt/freertos")
        );
    }

    /// issue 0755 — `--deploy` still selects, but among DIFFERENT BOARDS.
    ///
    /// That is what threading it from the cmake wrapper buys: one bringup with
    /// deploys for posix + fvp + hardware turns board facts into a silent skip
    /// without it. What it no longer arbitrates is two spellings of ONE board —
    /// see `two_deploys_on_one_board_cannot_disagree`.
    #[test]
    fn naming_the_deploy_picks_among_boards() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[deploy.nuttx]\nboard = \"nuttx-qemu-arm\"\nrmw = \"zenoh\"\n\n\
             [board_config.\"nuttx-qemu-arm\"]\n\
             sdk = {{ nuttx = \"{{env:NUTTX_DIR}}\" }}\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            "NUTTX_DIR" => Some("/opt/nuttx".to_string()),
            _ => None,
        };
        let nuttx = resolve(ws.path(), &repo_root(), Some("nuttx"), None, &env)
            .expect("the named deploy resolves");
        assert_eq!(
            nuttx.get("NROS_SDK_NUTTX").map(String::as_str),
            Some("/opt/nuttx")
        );
        assert!(
            !nuttx.contains_key("NROS_SDK_FREERTOS"),
            "the other board's site block must not leak in: {nuttx:?}"
        );
    }

    /// The regression this file's own gate demanded and this resolver could
    /// not serve: `examples/workspaces/realtime-rust` has SEVEN images and ZERO
    /// deploy blocks, so selecting candidates from `[deploy.*]` alone made its
    /// site table — which `check-site-config` requires — unreachable.
    #[test]
    fn an_image_only_workspace_resolves() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.fw]
board = "mps2-an385-freertos"

[board_config."mps2-an385-freertos"]
netstack = "lwip"
sdk = { freertos = "/opt/freertos", lwip = "/opt/lwip" }
"#,
        );
        let facts = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect("an image names the board just as a deploy does");
        assert_eq!(
            facts.get("NROS_BOARD").map(String::as_str),
            Some("mps2-an385-freertos")
        );
        assert_eq!(facts.get("NROS_NETSTACK").map(String::as_str), Some("lwip"));
        // And both selectors reach it.
        assert!(resolve(ws.path(), &repo_root(), Some("fw"), None, &|_| None).is_ok());
        assert!(
            resolve(
                ws.path(),
                &repo_root(),
                None,
                Some("mps2-an385-freertos"),
                &|_| None
            )
            .is_ok()
        );
    }

    /// Two boards share `packages/boards/nros-board-nuttx/nros-board.toml`, so
    /// a descriptor compared by its FILE answers "same board" for an arm and a
    /// riscv target. `--board nuttx-qemu-riscv` would then hand the build the
    /// arm board's facts — a wrong answer that reports success.
    #[test]
    fn two_boards_in_one_descriptor_file_stay_distinct() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.arm]
board = "nuttx-qemu-arm"

[image.riscv]
board = "nuttx-riscv"

[board_config."nuttx-qemu-arm"]
sdk = { nuttx = "/opt/arm", nuttx_apps = "/opt/arm-apps" }

[board_config."nuttx-qemu-riscv"]
sdk = { nuttx = "/opt/riscv", nuttx_apps = "/opt/riscv-apps" }
"#,
        );
        let arm = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("nuttx-qemu-arm"),
            &|_| None,
        )
        .expect("arm resolves");
        assert_eq!(
            arm.get("NROS_SDK_NUTTX").map(String::as_str),
            Some("/opt/arm")
        );
        // `nuttx-riscv` is the descriptor's declared name; `nuttx-qemu-riscv`
        // is another spelling of the SAME entry, so both must find the riscv
        // block and neither may find the arm one.
        for spelling in ["nuttx-riscv", "nuttx-qemu-riscv"] {
            let r = resolve(ws.path(), &repo_root(), None, Some(spelling), &|_| None)
                .unwrap_or_else(|e| panic!("{spelling} resolves: {e}"));
            assert_eq!(
                r.get("NROS_SDK_NUTTX").map(String::as_str),
                Some("/opt/riscv"),
                "{spelling} must not resolve the arm board"
            );
        }
    }

    /// A name in BOTH tables is one candidate, and the image's board wins —
    /// mid-migration they can disagree, and the image is the half that
    /// survives. Two candidates here would instead be reported as an ambiguity.
    #[test]
    fn an_image_outranks_a_deploy_of_the_same_name() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.fw]
board = "mps2-an385-freertos"

[deploy.fw]
kind = "embedded"
board = "threadx-linux"

[board_config."mps2-an385-freertos"]
netstack = "lwip"
sdk = { freertos = "/opt/freertos", lwip = "/opt/lwip" }
"#,
        );
        let facts = resolve(ws.path(), &repo_root(), None, None, &|_| None).expect("one answer");
        assert_eq!(
            facts.get("NROS_BOARD").map(String::as_str),
            Some("mps2-an385-freertos")
        );
    }

    /// The site key is matched by RESOLUTION, not by text — a board has several
    /// legal spellings and every one of them must find the same block. Keying
    /// on the string is issue 0606 one table over: a block authored under the
    /// descriptor's declared name would be invisible to a build spelling the
    /// board by its directory.
    #[test]
    fn a_site_block_under_an_alias_spelling_still_resolves() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[deploy.freertos]
board = "mps2-an385-freertos"

[board_config.freertos]
netstack = "lwip"
sdk = { freertos = "/opt/freertos" }
"#,
        );
        let facts = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect("the `freertos` key names the same board as the directory spelling");
        assert_eq!(facts.get("NROS_NETSTACK").map(String::as_str), Some("lwip"));
        assert_eq!(
            facts.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/opt/freertos")
        );
    }

    /// `board = "mps2-an385-freertos"` is the DIRECTORY spelling, which that
    /// descriptor does not carry in `names`. Every in-tree site block uses it.
    #[test]
    fn a_board_named_by_its_directory_still_resolves() {
        let catalog = BoardCatalog::load(&repo_root()).expect("in-tree catalog");
        assert!(resolve_board(&catalog, "mps2-an385-freertos").is_some());
        assert!(
            resolve_board(&catalog, "freertos").is_some(),
            "declared name"
        );
        assert!(resolve_board(&catalog, "no-such-board").is_none());
    }
}

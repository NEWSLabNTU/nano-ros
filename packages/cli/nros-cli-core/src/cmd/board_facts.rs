//! phase-351 W5 — resolve one deploy's board FACTS + SITE config, as env lines.
//!
//! [RFC-0072](../../../../../docs/design/0072-rtos-integration-nano-ros-is-a-guest.md)
//! §5 splits board information into A (board facts, in the board package), B
//! (site config, in the user's `[deploy.<name>.nros]`) and C (test harness).
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
    // Several deploy blocks may name the SAME board — `examples/workspaces/mixed`
    // has `[deploy.freertos]` and `[deploy.mps2-an385-freertos]`, both on
    // `mps2-an385-freertos`. That is only ambiguous if they RESOLVE differently:
    // the caller asked what this board builds with, and two blocks agreeing on
    // the answer are one answer. Refuse only when they actually disagree.
    let candidates = pick_deploys(ws, deploy, board)?;
    let mut resolved: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    for (name, target) in candidates {
        let facts = resolve_one(ws, nano_ros_root, &name, &target, env)?;
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
            "deploys `{first_name}` and `{other_name}` name the same board but resolve \
                 DIFFERENTLY ({}); pass --deploy to say which one this build is",
            differing.join(", ")
        ));
    }
    resolved
        .into_iter()
        .next()
        .map(|(_, f)| f)
        .ok_or_else(|| eyre!("{}: no [deploy.*] blocks", ws.display()))
}

fn resolve_one(
    ws: &Path,
    nano_ros_root: &Path,
    deploy_name: &str,
    target: &crate::orchestration::cargo_metadata_schema::DeployTarget,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<BTreeMap<String, String>> {
    let deploy_name = deploy_name.to_string();
    let board_name = target
        .board
        .clone()
        .ok_or_else(|| eyre!("[deploy.{deploy_name}] names no `board`"))?;

    let catalog = BoardCatalog::load(nano_ros_root)
        .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
    let descriptor = resolve_board(&catalog, &board_name).ok_or_else(|| {
        eyre!(
            "no board descriptor claims `{board_name}` (deploy `{deploy_name}`). \
             Descriptors are matched by their `names` and by their directory \
             (`packages/boards/nros-board-<name>`)."
        )
    })?;

    let site = match target.nros.as_ref() {
        Some(v) => SiteConfig::from_value(v, &deploy_name, "system.toml")?,
        None => SiteConfig::default(),
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

    // W4 — the netstack, validated against the board's declared domain. An
    // unsupported pair fails HERE, at the seam that knows both halves, rather
    // than as a link error inside a stack nobody selected.
    if let Some(stack) = descriptor
        .resolve_netstack(site.netstack.as_deref())
        .map_err(|e| eyre!("{e}"))?
    {
        out.insert("NROS_NETSTACK".into(), stack.to_string());
    }

    let origin = format!("{}: [deploy.{deploy_name}.nros]", ws.display());
    for (name, raw) in &site.sdk {
        let r = site.interpolate(raw, &deploy_name, &origin, env)?;
        out.insert(format!("NROS_SDK_{}", env_key(name)), r.value);
    }
    for (role, raw) in &site.config_files {
        let r = site.interpolate(raw, &deploy_name, &origin, env)?;
        out.insert(format!("NROS_CONFIG_FILE_{}", env_key(role)), r.value);
    }
    if !site.include_dirs.is_empty() {
        let mut dirs = Vec::new();
        for raw in &site.include_dirs {
            dirs.push(site.interpolate(raw, &deploy_name, &origin, env)?.value);
        }
        out.insert("NROS_INCLUDE_DIRS".into(), dirs.join(";"));
    }
    if !site.defines.is_empty() {
        out.insert("NROS_DEFINES".into(), site.defines.join(";"));
    }
    for (key, raw) in &site.upload {
        let r = site.interpolate(raw, &deploy_name, &origin, env)?;
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

type PickedDeploy = (
    String,
    crate::orchestration::cargo_metadata_schema::DeployTarget,
);

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
    let block = nros.get("deploy").and_then(|d| d.get(&deploy_key));
    let target = crate::orchestration::cargo_metadata_schema::DeployTarget {
        board: Some(deploy_key.to_string()),
        nros: block.and_then(|b| b.get("nros")).cloned(),
        ..Default::default()
    };
    Ok(Some(vec![(deploy_key, target)]))
}

fn pick_deploys(ws: &Path, deploy: Option<&str>, board: Option<&str>) -> Result<Vec<PickedDeploy>> {
    // A standalone leaf first: it has a Cargo.toml and no bringup, and asking
    // for a system.toml there would fail naming a file that is not supposed to
    // exist.
    if let Some(from_manifest) = deploys_from_manifest(ws)? {
        return Ok(from_manifest);
    }
    let (path, system) = load_system_toml(ws)?;
    let mut candidates: Vec<(String, _)> = system.deploy.into_iter().collect();
    if let Some(want) = deploy {
        candidates.retain(|(k, _)| k == want);
        if candidates.is_empty() {
            return Err(eyre!("{}: no [deploy.{want}]", path.display()));
        }
    } else if let Some(want) = board {
        candidates.retain(|(_, t)| t.board.as_deref() == Some(want));
        if candidates.is_empty() {
            return Err(eyre!(
                "{}: no [deploy.*] with board = \"{want}\"",
                path.display()
            ));
        }
    }
    if candidates.is_empty() {
        return Err(eyre!("{}: no [deploy.*] blocks", path.display()));
    }
    Ok(candidates)
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

[deploy.freertos.nros]
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

    /// Two deploy blocks on the same board are ONE answer when they resolve the
    /// same — `examples/workspaces/mixed` ships exactly that pair, and refusing
    /// it would block every cmake build of that workspace over a distinction
    /// with no consequence.
    #[test]
    fn duplicate_deploys_that_agree_are_not_ambiguous() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[deploy.mps2-an385-freertos]\nboard = \"mps2-an385-freertos\"\n\
             rmw = \"zenoh\"\n\n[deploy.mps2-an385-freertos.nros]\nnetstack = \"lwip\"\n\
             sdk = {{ freertos = \"{{env:FREERTOS_DIR}}\", lwip = \"{{env:LWIP_DIR}}\" }}\n"
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

    /// …and DISAGREEING duplicates are refused, naming the keys that differ —
    /// that is the case where picking one silently would build the wrong thing.
    #[test]
    fn duplicate_deploys_that_disagree_are_refused() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[deploy.other]\nboard = \"mps2-an385-freertos\"\n\
             rmw = \"zenoh\"\n\n[deploy.other.nros]\nnetstack = \"lwip\"\n\
             sdk = {{ freertos = \"/elsewhere\", lwip = \"{{env:LWIP_DIR}}\" }}\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let err = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("mps2-an385-freertos"),
            &env,
        )
        .expect_err("two different SDK roots for one board");
        let msg = format!("{err:#}");
        assert!(msg.contains("NROS_SDK_FREERTOS"), "{msg}");
        assert!(msg.contains("--deploy"), "{msg}");
    }

    /// issue 0755 — the ambiguity the previous test proves is REFUSABLE is
    /// also RESOLVABLE, by naming the deploy. That is the whole point of
    /// threading `--deploy` from the cmake wrapper: the entry knows which
    /// deploy this build is, and without it a multi-deploy `system.toml`
    /// (one bringup, deploys for posix + fvp + hardware) turns board facts
    /// into a silent skip.
    #[test]
    fn naming_the_deploy_resolves_what_the_board_alone_cannot() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[deploy.other]\nboard = \"mps2-an385-freertos\"\n\
             rmw = \"zenoh\"\n\n[deploy.other.nros]\nnetstack = \"lwip\"\n\
             sdk = {{ freertos = \"/elsewhere\", lwip = \"{{env:LWIP_DIR}}\" }}\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        // By board alone: refused (the previous test).
        assert!(
            resolve(
                ws.path(),
                &repo_root(),
                None,
                Some("mps2-an385-freertos"),
                &env
            )
            .is_err()
        );
        // By deploy name: each one answers for itself.
        let other = resolve(ws.path(), &repo_root(), Some("other"), None, &env)
            .expect("the named deploy resolves");
        assert_eq!(
            other.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/elsewhere"),
            "the named deploy's own SDK root, not the other one's"
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

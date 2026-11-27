//! The names `<Tab>` offers (RFC-0133 D7, phase-569 W4).
//!
//! Each completer answers one name argument from what the CLI already reads:
//! compiled-in tables and the files a `list` verb reads. A completer runs on
//! every `<Tab>`, so it never builds, scans a workspace or reaches the
//! network, and it never fails: a source it cannot read offers no names.

use clap_complete::engine::CompletionCandidate;
use eyre::{Result, eyre};

use crate::orchestration::{board_descriptor::BoardCatalog, rmw_index};

/// Board keys and their aliases, as `board = "…"` and `nros board show` take
/// them, from the board catalog of the SDK root `nros board list` reads.
pub fn boards() -> Vec<CompletionCandidate> {
    let Ok(root) = crate::cmd::board::catalog_root(None) else {
        return Vec::new();
    };
    let Ok(catalog) = BoardCatalog::load(&root) else {
        return Vec::new();
    };
    let names = catalog
        .descriptors()
        .iter()
        .flat_map(|d| d.names.iter().cloned());
    sorted(names.map(CompletionCandidate::new))
}

/// RMW names and their aliases, as `rmw = "…"` takes them: the SDK root's
/// backends, plus a workspace's own providers from the completion cache. The
/// SDK root alone is never a workspace scan; the workspace's providers would
/// be, so they come from the cache.
pub fn rmws() -> Vec<CompletionCandidate> {
    let mut out: Vec<CompletionCandidate> = cached()
        .map(|c| c.rmws.into_iter().map(CompletionCandidate::new).collect())
        .unwrap_or_default();
    let sdk = std::env::current_dir()
        .ok()
        .and_then(|cwd| crate::orchestration::nano_ros_root::resolve(None, &cwd))
        .and_then(|root| rmw_index::for_sdk_root_path(&root).ok());
    if let Some(index) = sdk {
        out.extend(
            index_names(&index)
                .into_iter()
                .map(CompletionCandidate::new),
        );
    }
    sorted(out)
}

/// Every name an RMW index resolves: each backend's canonical name and aliases.
pub(crate) fn index_names(index: &rmw_index::RmwIndex) -> Vec<String> {
    let mut out = Vec::new();
    for name in index.known() {
        match index.resolve(&name) {
            Ok(r) => out.extend(r.names.iter().cloned()),
            Err(_) => out.push(name),
        }
    }
    out
}

/// The workspace's images, as `nros build <image>` names them, from the
/// completion cache. No cache, no names.
pub fn images() -> Vec<CompletionCandidate> {
    sorted(
        cached()
            .map(|c| c.images)
            .unwrap_or_default()
            .into_iter()
            .map(CompletionCandidate::new),
    )
}

/// The extension verbs the workspace's packages announce (`nros <verb>`),
/// from the completion cache, beside the built-in verbs clap offers.
pub fn extension_verbs() -> Vec<CompletionCandidate> {
    sorted(
        cached()
            .map(|c| c.commands)
            .unwrap_or_default()
            .into_iter()
            .map(|(verb, summary)| CompletionCandidate::new(verb).help(Some(summary.into()))),
    )
}

/// The interface types the workspace reaches, from the completion cache. No
/// cache, no names: listing them reads the whole ament environment, which no
/// completer may do.
pub fn interface_types() -> Vec<CompletionCandidate> {
    sorted(
        cached()
            .map(|c| c.interfaces)
            .unwrap_or_default()
            .into_iter()
            .map(CompletionCandidate::new),
    )
}

/// The workspace's packages, from the completion cache. No cache, no names.
pub fn packages() -> Vec<CompletionCandidate> {
    sorted(
        cached()
            .map(|c| c.packages)
            .unwrap_or_default()
            .into_iter()
            .map(CompletionCandidate::new),
    )
}

// ---- the completion cache --------------------------------------------------
//
// The names a workspace scan finds (images, packages, its own RMW providers),
// written by `nros configure` and `nros build` from the scans they already
// run, and read only by completion. It records the search path it was built
// for, the key the provider index uses: a cache for another search path is
// WRONG, not stale, and offers nothing. Within one search path, stale entries
// are expected; a package added since the last configure is offered after the
// next one.

/// The cache's schema; a cache with another is not read.
pub const CACHE_SCHEMA: u32 = 1;

/// `build/nros/complete.json`.
#[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cache {
    pub schema: u32,
    /// The provider search path the names were found on, in order.
    pub search_path: Vec<String>,
    pub images: Vec<String>,
    pub packages: Vec<String>,
    /// The workspace's own RMW names (the SDK root's are not cached).
    pub rmws: Vec<String>,
    /// The top-level extension verbs the workspace's packages announce, each
    /// with its summary.
    #[serde(default)]
    pub commands: Vec<(String, String)>,
    /// The interface types the workspace reaches, as `nros interface show`
    /// names them (phase-569 W4d). A cache written before it reads as none.
    #[serde(default)]
    pub interfaces: Vec<String>,
}

/// Where `workspace`'s completion cache lives.
pub fn cache_path(workspace: &std::path::Path) -> std::path::PathBuf {
    workspace.join("build/nros/complete.json")
}

/// The search-path key for `workspace`: the provider index's search path.
pub fn search_key(workspace: &std::path::Path) -> Result<Vec<String>> {
    Ok(crate::cmd::ws::rmw_search_path_for(workspace, &[])?
        .paths()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Write `workspace`'s cache, atomically: a completion reading it while a
/// configure rewrites it sees the old cache or the new one, never a torn one.
pub fn write_cache(workspace: &std::path::Path, cache: &Cache) -> Result<()> {
    let path = cache_path(workspace);
    let dir = path
        .parent()
        .ok_or_else(|| eyre!("{} has no parent", path.display()))?;
    std::fs::create_dir_all(dir)?;
    crate::atomic_file::atomic_write(&path, &serde_json::to_string_pretty(cache)?)?;
    Ok(())
}

/// The cache, if `bytes` is one this build reads and its key is `key`.
fn accept(bytes: &[u8], key: &[String]) -> Option<Cache> {
    let cache: Cache = serde_json::from_slice(bytes).ok()?;
    (cache.schema == CACHE_SCHEMA && cache.search_path == key).then_some(cache)
}

/// The cache of the workspace the current directory is in, when it was built
/// for that workspace's search path.
fn cached() -> Option<Cache> {
    cached_in(&std::env::current_dir().ok()?)
}

/// [`cached`] for the workspace `dir` is in.
fn cached_in(dir: &std::path::Path) -> Option<Cache> {
    let ws = crate::cmd::image_facts::enclosing_workspace(dir)?;
    let bytes = std::fs::read(cache_path(&ws)).ok()?;
    accept(&bytes, &search_key(&ws).ok()?)
}

/// Public knob names, as `[image.<id>.knobs]` keys them, each with its prompt.
/// Read from the registry directly rather than through `cmd::knob`, which
/// would pull that verb into the code `nros build` runs.
pub fn knobs() -> Vec<CompletionCandidate> {
    sorted(nros_zephyr_build::public_knobs().map(|r| {
        CompletionCandidate::new(r.image_key).help(r.prompt.map(|p| p.to_string().into()))
    }))
}

/// The profiles `nros build --profile=` and `[image.<id>] profile` take: each
/// cargo profile a CMake build type maps to, then the CMake spellings.
pub fn profiles() -> Vec<CompletionCandidate> {
    let cargo = nros_cargo_profile::BUILD_TYPE_MAP.iter().map(|(t, p)| {
        CompletionCandidate::new(*p).help(Some(format!("cargo profile (CMake: {t})").into()))
    });
    let cmake = ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"]
        .map(|t| CompletionCandidate::new(t).help(Some("CMake build type".into())));
    sorted(cargo.chain(cmake))
}

/// The toolchain versions installed in the store (`toolchains/<version>`),
/// a directory listing and nothing more.
pub fn toolchains() -> Vec<CompletionCandidate> {
    use crate::orchestration::store;
    let Ok(dir) = std::fs::read_dir(store::toolchains_dir(&store::root())) else {
        return Vec::new();
    };
    sorted(dir.flatten().filter(|e| e.path().is_dir()).filter_map(|e| {
        e.file_name()
            .to_str()
            .map(|n| CompletionCandidate::new(n.to_string()))
    }))
}

/// Every live diagnostic code (`NROS-NNNN`), each with its one-line title.
/// A retired code has no title and is not offered.
pub fn codes() -> Vec<CompletionCandidate> {
    sorted(
        nros_diagnostic_codes::ALL
            .iter()
            .filter(|e| !e.title.is_empty())
            .map(|e| CompletionCandidate::new(e.code).help(Some(e.title.into()))),
    )
}

// The registration of dynamic completion. Here rather than in the
// `completions` verb so `nros setup` can print it without reaching that verb.

/// The shells `nros` completes in.
pub const SHELLS: [&str; 4] = ["bash", "zsh", "fish", "powershell"];

pub fn shell_from_env(shell_var: Option<&str>) -> Option<String> {
    let base = shell_var?.rsplit('/').next()?;
    SHELLS.iter().find(|s| **s == base).map(|s| s.to_string())
}

/// The line that registers dynamic completion for `shell`, to add to the
/// shell's startup file.
pub fn registration_line(shell: &str) -> Option<&'static str> {
    Some(match shell {
        "bash" => "source <(COMPLETE=bash nros)",
        "zsh" => "source <(COMPLETE=zsh nros)",
        "fish" => "COMPLETE=fish nros | source",
        "powershell" => {
            "$env:COMPLETE = \"powershell\"; nros | Out-String | Invoke-Expression; \
             Remove-Item Env:\\COMPLETE"
        }
        _ => return None,
    })
}

/// The startup file each shell reads, for the message around the line.
fn startup_file(shell: &str) -> &'static str {
    match shell {
        "bash" => "~/.bashrc",
        "zsh" => "~/.zshrc",
        "fish" => "~/.config/fish/config.fish",
        _ => "your PowerShell profile ($PROFILE)",
    }
}

/// The text `nros completions` prints: the line, and where it goes.
pub fn render(shell: &str) -> Result<String> {
    let line =
        registration_line(shell).ok_or_else(|| eyre!("`nros` does not complete in `{shell}`"))?;
    Ok(format!(
        "# Turn on `nros` completion: add this line to {}\n{line}\n",
        startup_file(shell)
    ))
}

/// Sorted by name, one candidate per name.
fn sorted(items: impl IntoIterator<Item = CompletionCandidate>) -> Vec<CompletionCandidate> {
    let mut v: Vec<CompletionCandidate> = items.into_iter().collect();
    v.sort_by(|a, b| a.get_value().cmp(b.get_value()));
    v.dedup_by(|a, b| a.get_value() == b.get_value());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(c: &[CompletionCandidate]) -> Vec<String> {
        c.iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn knobs_offers_every_public_knob_with_a_summary() {
        let k = knobs();
        assert!(values(&k).iter().any(|n| n == "net.max_sockets"));
        assert_eq!(k.len(), nros_zephyr_build::public_knobs().count());
    }

    #[test]
    fn sorted_keeps_one_candidate_per_name() {
        let v = sorted(["b", "a", "b"].map(CompletionCandidate::new));
        assert_eq!(values(&v), ["a", "b"]);
    }

    #[test]
    fn every_shell_has_a_registration_line_naming_complete() {
        for s in SHELLS {
            let line = registration_line(s).expect(s);
            assert!(
                line.contains("COMPLETE") && line.contains("nros"),
                "{s}: {line}"
            );
        }
        assert!(registration_line("tcsh").is_none());
    }

    #[test]
    fn the_shell_defaults_to_the_one_shell_names() {
        assert_eq!(shell_from_env(Some("/usr/bin/zsh")).as_deref(), Some("zsh"));
        assert_eq!(shell_from_env(Some("fish")).as_deref(), Some("fish"));
        assert_eq!(shell_from_env(Some("/bin/tcsh")), None);
        assert_eq!(shell_from_env(None), None);
    }

    #[test]
    fn render_says_where_the_line_goes() {
        assert_eq!(
            render("bash").unwrap(),
            "# Turn on `nros` completion: add this line to ~/.bashrc\n\
             source <(COMPLETE=bash nros)\n"
        );
    }

    #[test]
    fn a_cache_for_another_search_path_offers_nothing() {
        let c = Cache {
            schema: CACHE_SCHEMA,
            search_path: vec!["/sdk".into(), "/ws/src".into()],
            images: vec!["linux".into()],
            ..Cache::default()
        };
        let bytes = serde_json::to_vec(&c).unwrap();
        assert_eq!(accept(&bytes, &["/sdk".into(), "/ws/src".into()]), Some(c));
        assert_eq!(accept(&bytes, &["/sdk".into(), "/other/src".into()]), None);
    }

    #[test]
    fn a_cache_of_another_schema_is_not_read() {
        let c = Cache {
            schema: CACHE_SCHEMA + 1,
            ..Cache::default()
        };
        assert_eq!(accept(&serde_json::to_vec(&c).unwrap(), &[]), None);
        assert_eq!(accept(b"not json", &[]), None);
    }

    #[test]
    fn write_cache_round_trips() {
        let ws = tempfile::tempdir().unwrap();
        let c = Cache {
            schema: CACHE_SCHEMA,
            search_path: vec!["/k".into()],
            packages: vec!["talker".into()],
            ..Cache::default()
        };
        write_cache(ws.path(), &c).unwrap();
        let bytes = std::fs::read(cache_path(ws.path())).unwrap();
        assert_eq!(accept(&bytes, &["/k".into()]), Some(c));
    }

    #[test]
    fn a_leaf_reads_the_cache_its_configure_wrote_and_not_a_foreign_one() {
        let leaf = tempfile::tempdir().unwrap();
        std::fs::write(
            leaf.path().join("system.toml"),
            "[image.linux]\nboard = \"linux\"\n",
        )
        .unwrap();
        let mut c = Cache {
            schema: CACHE_SCHEMA,
            search_path: search_key(leaf.path()).unwrap(),
            images: vec!["linux".into()],
            ..Cache::default()
        };
        write_cache(leaf.path(), &c).unwrap();
        assert_eq!(
            cached_in(leaf.path()).map(|c| c.images),
            Some(vec!["linux".into()])
        );
        c.search_path.push("/another/root".into());
        write_cache(leaf.path(), &c).unwrap();
        assert_eq!(cached_in(leaf.path()), None);
    }

    /// phase-569 W4d: interface types come from the cache, and a cache
    /// written before the field existed reads as none rather than failing.
    #[test]
    fn interface_types_round_trip_and_an_older_cache_has_none() {
        let leaf = tempfile::tempdir().unwrap();
        std::fs::write(
            leaf.path().join("system.toml"),
            "[image.linux]\nboard = \"linux\"\n",
        )
        .unwrap();
        let key = search_key(leaf.path()).unwrap();
        let c = Cache {
            schema: CACHE_SCHEMA,
            search_path: key.clone(),
            interfaces: vec!["std_msgs/msg/String".into()],
            ..Cache::default()
        };
        write_cache(leaf.path(), &c).unwrap();
        assert_eq!(
            cached_in(leaf.path()).map(|c| c.interfaces),
            Some(vec!["std_msgs/msg/String".to_string()])
        );
        let older = serde_json::json!({
            "schema": CACHE_SCHEMA, "search_path": key, "images": [], "packages": [], "rmws": [],
        });
        let read = accept(&serde_json::to_vec(&older).unwrap(), &key).expect("still read");
        assert!(read.interfaces.is_empty());
    }

    #[test]
    fn profiles_offer_both_vocabularies_build_accepts() {
        for p in values(&profiles()) {
            assert!(
                nros_cargo_profile::cmake_build_type(&p).is_some(),
                "`{p}` is offered but `nros build --profile` refuses it"
            );
        }
        assert!(values(&profiles()).iter().any(|p| p == "release"));
    }

    #[test]
    fn codes_offer_every_live_code_and_no_retired_one() {
        let c = values(&codes());
        let live = nros_diagnostic_codes::ALL
            .iter()
            .filter(|e| !e.title.is_empty());
        assert_eq!(c.len(), live.count());
        assert!(c.iter().all(|v| v.starts_with("NROS-")));
    }
}

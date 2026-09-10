//! RFC-0098 D3/D5/D8 (phase-445 W3) — what ONE package deploys to, and where
//! that is written.
//!
//! A single-package example (a directory holding `Cargo.toml` or
//! `CMakeLists.txt` + `package.xml`, with no enclosing workspace) states its
//! board, RMW, domain, network identity and node declaration in a
//! `system.toml` beside its manifest — the SAME schema a workspace bringup
//! uses (`[system]`, `[[component]]`, `[image.<id>]`). This module is the one
//! reader of that declaration, shared by every consumer that needs it:
//!
//! * `nros::main!()` (the proc-macro cannot depend on `nros-cli-core`, which is
//!   why this lives here — the same reason `model_location` does);
//! * `nros sync` / `nros ws board-facts` / `nros ws leaf-system` and the entity
//!   budget derivation in `nros-cli-core`;
//! * the C/C++ lanes, through `nros ws leaf-system`, which prints this module's
//!   answer as `KEY=VALUE` lines for cmake.
//!
//! It walks `toml::Value` rather than deserialising a mirror struct, on
//! purpose: the typed, `deny_unknown_fields` schema is `nros-cli-core`'s
//! `SystemToml`, and every `nros sync` parses the file through it, so a typo is
//! still refused. A second typed struct here would be a second schema that can
//! drift — exactly what this crate exists to prevent.
//!
//! # The retiring manifest keys — ONE fallback, deletable in one commit
//!
//! Before RFC-0098 a Rust leaf spelled all of this in `Cargo.toml`:
//! `[package.metadata.nros.entry] deploy`, `[package.metadata.nros.deploy.<b>]`,
//! `[package.metadata.nros.node]` and `[package.metadata.nros.component]`.
//! While the remaining leaves are converted, [`read`] still accepts those keys
//! when no `system.toml` exists, and marks the answer [`Origin::Manifest`] so
//! the CLI can print [`LeafSystem::deprecation`]. The whole fallback is
//! [`from_manifest`] plus its one call in [`read`]; deleting both retires the
//! old spelling everywhere at once. A leaf carrying BOTH spellings is refused,
//! never merged: two sources for one fact is how they disagree.

use std::path::{Path, PathBuf};

/// The file a single-package leaf states its deployment in.
pub const SYSTEM_TOML: &str = "system.toml";

/// Where a [`LeafSystem`] was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `<leaf>/system.toml` — the RFC-0098 home.
    SystemToml(PathBuf),
    /// `<leaf>/Cargo.toml`'s retiring `[package.metadata.nros.*]` keys.
    Manifest(PathBuf),
}

/// How the board was named, which matters to exactly one consumer: the board
/// `cargo_config` projection has only ever followed an explicit board choice
/// (`[image] board`, `[package.metadata.nros.entry] deploy`), never a lone
/// `[package.metadata.nros.deploy.<key>]` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardFrom {
    /// `[image.<id>] board` in `system.toml`.
    Image,
    /// `[package.metadata.nros.entry] deploy` (retiring).
    EntryDeploy,
    /// The key of the manifest's only `[package.metadata.nros.deploy.<key>]`
    /// table (retiring; the Zephyr leaves' spelling).
    DeployTable,
}

/// Deployment identity (RFC-0098 D5): what the image dials and what it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Network {
    pub domain_id: Option<u32>,
    pub locator: Option<String>,
    pub ip: Option<String>,
    pub gateway: Option<String>,
    pub netmask: Option<String>,
    pub transport: Option<String>,
}

/// One declared node (`[[component]]`, RFC-0098 D8 for `entities`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeafComponent {
    /// `package.xml` name of the package providing the class. `None` only in
    /// the manifest fallback, where the table has no such key.
    pub pkg: Option<String>,
    pub class: Option<String>,
    pub name: Option<String>,
    /// Declared entities, in `EntityDecl::parse` grammar, for a board whose
    /// component cannot be host-probed (issue 1265). `None` = not declared;
    /// `Some(vec![])` = declared to have none.
    pub entities: Option<Vec<String>>,
}

/// A leaf's resolved deployment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafSystem {
    pub origin: Origin,
    /// The `[image.<id>]` this answer describes. `None` from the fallback.
    pub image: Option<String>,
    /// The board the image is built for. `None` only from the fallback, for a
    /// manifest that declares a node but no deploy.
    pub board: Option<String>,
    pub board_from: Option<BoardFrom>,
    /// RMW: `[image] rmw` > `[image_defaults] rmw` > `[system] rmw`.
    pub rmw: Option<String>,
    pub network: Network,
    pub components: Vec<LeafComponent>,
}

impl LeafSystem {
    /// Did this come from the retiring manifest keys?
    pub fn is_fallback(&self) -> bool {
        matches!(self.origin, Origin::Manifest(_))
    }

    /// The file the answer came from.
    pub fn origin_path(&self) -> &Path {
        match &self.origin {
            Origin::SystemToml(p) | Origin::Manifest(p) => p,
        }
    }

    /// One line telling the user which file to write, when the answer came from
    /// the retiring keys. `None` for a `system.toml` answer.
    pub fn deprecation(&self) -> Option<String> {
        let Origin::Manifest(manifest) = &self.origin else {
            return None;
        };
        let dir = manifest.parent().unwrap_or(Path::new("."));
        Some(format!(
            "warning: {}: `[package.metadata.nros.{{entry,deploy,node,component}}]` is deprecated \
             (RFC-0098 D3/D5) — write {} (`[image.<id>] board = \"{}\"`, `[system] rmw/domain_id`, \
             `[[component]]`) and delete those tables",
            manifest.display(),
            dir.join(SYSTEM_TOML).display(),
            self.board.as_deref().unwrap_or("<board>"),
        ))
    }

    /// Every component's declared entities, concatenated. `None` when no
    /// component declares any (the probe is the only source then).
    pub fn declared_entities(&self) -> Option<Vec<String>> {
        let mut any = false;
        let mut out = Vec::new();
        for c in &self.components {
            if let Some(e) = &c.entities {
                any = true;
                out.extend(e.iter().cloned());
            }
        }
        any.then_some(out)
    }
}

/// Is `dir` a PACKAGE (as opposed to a workspace bringup, which carries a
/// `system.toml` and no build file)?
pub fn is_package_dir(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file() || dir.join("CMakeLists.txt").is_file()
}

/// The deployment `dir` declares, or `None` when it declares none.
///
/// * `<dir>/system.toml` beside a package manifest → read from it; a leftover
///   retiring key in `Cargo.toml` is an ERROR (both-present refusal).
/// * otherwise → the retiring manifest keys ([`from_manifest`]), if any.
///
/// A `system.toml` in a directory with no `Cargo.toml`/`CMakeLists.txt` is a
/// workspace BRINGUP, not a leaf, and yields `None` here: its images are
/// chosen by the workspace builder, not by this reader.
pub fn read(dir: &Path) -> Result<Option<LeafSystem>, String> {
    let system = dir.join(SYSTEM_TOML);
    if system.is_file() && is_package_dir(dir) {
        refuse_manifest_duplicate(dir, &system)?;
        return from_system_toml(&system).map(Some);
    }
    from_manifest(dir)
}

/// The board `dir` deploys to, from [`read`].
pub fn board(dir: &Path) -> Result<Option<String>, String> {
    Ok(read(dir)?.and_then(|l| l.board))
}

fn as_table(v: Option<&toml::Value>) -> Option<&toml::Table> {
    v.and_then(|v| v.as_table())
}

fn str_key(t: Option<&toml::Table>, key: &str) -> Option<String> {
    t?.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn u32_key(t: Option<&toml::Table>, key: &str, file: &Path) -> Result<Option<u32>, String> {
    let Some(v) = t.and_then(|t| t.get(key)) else {
        return Ok(None);
    };
    let i = v
        .as_integer()
        .ok_or_else(|| format!("{}: `{key}` must be an integer, got {v}", file.display()))?;
    u32::try_from(i)
        .map(Some)
        .map_err(|_| format!("{}: `{key}` = {i} is out of range", file.display()))
}

fn entities_key(t: &toml::Table, file: &Path) -> Result<Option<Vec<String>>, String> {
    let Some(v) = t.get("entities") else {
        return Ok(None);
    };
    let arr = v.as_array().ok_or_else(|| {
        format!(
            "{}: `entities` must be an ARRAY of strings, e.g. \
             [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"]",
            file.display()
        )
    })?;
    arr.iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                format!(
                    "{}: every `entities` element must be a string; found {item}",
                    file.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn parse_file(path: &Path) -> Result<toml::Table, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    raw.parse::<toml::Table>()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a leaf `system.toml`. A leaf builds ONE image: exactly one
/// `[image.<id>]`, or `[system] default_images = ["<id>"]` naming one of
/// several.
fn from_system_toml(path: &Path) -> Result<LeafSystem, String> {
    let doc = parse_file(path)?;
    let system = as_table(doc.get("system"));
    let defaults = as_table(doc.get("image_defaults"));
    let images = as_table(doc.get("image")).cloned().unwrap_or_default();

    let ids: Vec<&String> = images.keys().collect();
    let default_images: Vec<String> = system
        .and_then(|s| s.get("default_images"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let id: String = match (ids.as_slice(), default_images.as_slice()) {
        ([], _) => {
            return Err(format!(
                "{}: a single-package leaf must declare the image it builds — add \
                 `[image.<id>]` with `board = \"<board>\"` (RFC-0098 D3)",
                path.display()
            ));
        }
        ([one], _) => (*one).clone(),
        (_, [one]) => {
            if !images.contains_key(one) {
                return Err(format!(
                    "{}: `[system] default_images = [\"{one}\"]` names no `[image.{one}]`",
                    path.display()
                ));
            }
            one.clone()
        }
        (many, _) => {
            return Err(format!(
                "{}: a single-package leaf builds ONE image, and {} are declared ({}) — \
                 set `[system] default_images = [\"<id>\"]` to choose",
                path.display(),
                many.len(),
                many.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    let image = images.get(&id).and_then(|v| v.as_table());
    // `[image.<id>]` over `[image_defaults]` — the one overlay RFC-0065 D5.1
    // defines (`ImageBlock::with_base`), applied key by key.
    let pick = |key: &str| str_key(image, key).or_else(|| str_key(defaults, key));

    let board = pick("board").ok_or_else(|| {
        format!(
            "{}: `[image.{id}]` names no `board` — every image is built for exactly one \
             board (RFC-0098 D3)",
            path.display()
        )
    })?;
    let rmw = pick("rmw").or_else(|| str_key(system, "rmw"));
    let domain_id = match u32_key(image, "domain_id", path)? {
        Some(d) => Some(d),
        None => match u32_key(defaults, "domain_id", path)? {
            Some(d) => Some(d),
            None => u32_key(system, "domain_id", path)?,
        },
    };
    let network = Network {
        domain_id,
        locator: pick("locator").or_else(|| str_key(system, "locator")),
        ip: pick("ip"),
        gateway: pick("gateway"),
        netmask: pick("netmask"),
        transport: pick("transport"),
    };

    let mut components = Vec::new();
    if let Some(rows) = doc.get("component") {
        let rows = rows.as_array().ok_or_else(|| {
            format!(
                "{}: `component` must be an array of tables (`[[component]]`)",
                path.display()
            )
        })?;
        for row in rows {
            let t = row.as_table().ok_or_else(|| {
                format!("{}: every `[[component]]` must be a table", path.display())
            })?;
            components.push(LeafComponent {
                pkg: str_key(Some(t), "pkg"),
                class: str_key(Some(t), "class"),
                name: str_key(Some(t), "name"),
                entities: entities_key(t, path)?,
            });
        }
    }

    Ok(LeafSystem {
        origin: Origin::SystemToml(path.to_path_buf()),
        image: Some(id),
        board: Some(board),
        board_from: Some(BoardFrom::Image),
        rmw,
        network,
        components,
    })
}

/// The retiring tables a manifest still carries, by name.
fn retiring_manifest_keys(nros: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    if as_table(nros.get("entry")).is_some_and(|e| e.contains_key("deploy")) {
        out.push("[package.metadata.nros.entry] deploy".to_string());
    }
    if let Some(d) = as_table(nros.get("deploy")) {
        for k in d.keys() {
            out.push(format!("[package.metadata.nros.deploy.{k}]"));
        }
    }
    for t in ["node", "component"] {
        if nros.contains_key(t) {
            out.push(format!("[package.metadata.nros.{t}]"));
        }
    }
    out
}

fn manifest_nros(dir: &Path) -> Result<Option<(PathBuf, toml::Table)>, String> {
    let manifest = dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(None);
    }
    let doc = parse_file(&manifest)?;
    let nros = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.as_table())
        .cloned();
    Ok(nros.map(|n| (manifest, n)))
}

/// Both spellings present: refuse, naming both files and each leftover key.
fn refuse_manifest_duplicate(dir: &Path, system: &Path) -> Result<(), String> {
    let Some((manifest, nros)) = manifest_nros(dir)? else {
        return Ok(());
    };
    let left = retiring_manifest_keys(&nros);
    if left.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} states this leaf's deployment, and {} still carries {} — one source per fact \
         (RFC-0098 D5). Delete those tables from the manifest.",
        system.display(),
        manifest.display(),
        left.join(", ")
    ))
}

/// DEPRECATED FALLBACK (phase-445 W3) — the retiring `Cargo.toml` keys.
///
/// This function and its one call in [`read`] are the whole of the old
/// spelling's support. Delete both once no leaf under `examples/` carries
/// `[package.metadata.nros.{entry,deploy,node,component}]`.
pub fn from_manifest(dir: &Path) -> Result<Option<LeafSystem>, String> {
    let Some((manifest, nros)) = manifest_nros(dir)? else {
        return Ok(None);
    };
    if retiring_manifest_keys(&nros).is_empty() {
        return Ok(None);
    }
    let deploys = as_table(nros.get("deploy"));
    let (board, board_from) = match str_key(as_table(nros.get("entry")), "deploy") {
        Some(b) => (Some(b), Some(BoardFrom::EntryDeploy)),
        None => match deploys {
            // Several tables and nothing says which this build is: no guess.
            Some(t) if t.len() == 1 => (t.keys().next().cloned(), Some(BoardFrom::DeployTable)),
            _ => (None, None),
        },
    };
    let block = board
        .as_deref()
        .and_then(|b| as_table(deploys.and_then(|d| d.get(b))));
    let network = Network {
        domain_id: u32_key(block, "domain_id", &manifest)?,
        locator: str_key(block, "locator"),
        ip: str_key(block, "ip"),
        gateway: str_key(block, "gateway"),
        netmask: str_key(block, "netmask"),
        transport: str_key(block, "transport"),
    };
    let node = as_table(nros.get("node"));
    let component = as_table(nros.get("component"));
    let mut components = Vec::new();
    if node.is_some() || component.is_some() {
        let decl = node.or(component);
        let entities = match component.or(node) {
            Some(t) => entities_key(t, &manifest)?,
            None => None,
        };
        components.push(LeafComponent {
            pkg: None,
            class: str_key(decl, "class"),
            name: str_key(decl, "name"),
            entities,
        });
    }
    Ok(Some(LeafSystem {
        origin: Origin::Manifest(manifest),
        image: None,
        board,
        board_from,
        rmw: str_key(block, "rmw"),
        network,
        components,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(files: &[(&str, &str)]) -> tempdir::Dir {
        let d = tempdir::Dir::new();
        for (name, body) in files {
            std::fs::write(d.path().join(name), body).unwrap();
        }
        d
    }

    /// Minimal self-cleaning temp dir; this crate carries no dev-deps.
    mod tempdir {
        use std::path::{Path, PathBuf};
        pub struct Dir(PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicUsize, Ordering};
                static N: AtomicUsize = AtomicUsize::new(0);
                let p = std::env::temp_dir().join(format!(
                    "nros-leaf-system-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    const CARGO: &str = "[package]\nname = \"talker\"\nversion = \"0.1.0\"\n";

    const SYSTEM: &str = r#"
[system]
name = "talker"
rmw = "zenoh"
domain_id = 3
locator = "tcp/10.0.2.2:7447"

[[component]]
pkg = "esp32_talker"
class = "esp32_talker::Talker"
name = "talker"
entities = ["publisher:std_msgs/msg/String:/chatter", "timer"]

[image.esp32]
board = "esp32-c3-baremetal"
ip = "10.0.2.50"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:9800"
"#;

    #[test]
    fn a_single_package_leaf_resolves_from_its_system_toml() {
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", SYSTEM)]);
        let l = read(d.path()).unwrap().expect("declared");
        assert!(!l.is_fallback());
        assert!(l.deprecation().is_none());
        assert_eq!(l.image.as_deref(), Some("esp32"));
        assert_eq!(l.board.as_deref(), Some("esp32-c3-baremetal"));
        assert_eq!(l.board_from, Some(BoardFrom::Image));
        assert_eq!(l.rmw.as_deref(), Some("zenoh"));
        // The image's locator beats the system default; the domain falls
        // through to `[system]`.
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:9800"));
        assert_eq!(l.network.domain_id, Some(3));
        assert_eq!(l.network.ip.as_deref(), Some("10.0.2.50"));
        assert_eq!(l.network.netmask, None);
        assert_eq!(l.components.len(), 1);
        assert_eq!(l.components[0].pkg.as_deref(), Some("esp32_talker"));
        assert_eq!(
            l.declared_entities().unwrap(),
            vec!["publisher:std_msgs/msg/String:/chatter", "timer"]
        );
    }

    #[test]
    fn a_cmake_leaf_is_a_package_too() {
        let d = leaf(&[("CMakeLists.txt", "project(x)\n"), ("system.toml", SYSTEM)]);
        assert!(read(d.path()).unwrap().is_some());
    }

    #[test]
    fn a_bringup_system_toml_is_not_a_leaf() {
        // No build file beside it: a workspace bringup, whose images the
        // builder chooses. Not this reader's question.
        let d = leaf(&[("system.toml", SYSTEM)]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    #[test]
    fn the_retiring_keys_still_resolve_and_say_so() {
        let d = leaf(&[(
            "Cargo.toml",
            &format!(
                "{CARGO}\n[package.metadata.nros.entry]\ndeploy = \"freertos\"\n\n\
                 [package.metadata.nros.node]\nclass = \"t::Talker\"\nname = \"talker\"\n\n\
                 [package.metadata.nros.deploy.freertos]\nrmw = \"zenoh\"\ndomain_id = 0\n\
                 locator = \"tcp/10.0.2.2:7800\"\nip = \"10.0.2.15\"\n"
            ),
        )]);
        let l = read(d.path()).unwrap().expect("declared");
        assert!(l.is_fallback());
        assert_eq!(l.board.as_deref(), Some("freertos"));
        assert_eq!(l.board_from, Some(BoardFrom::EntryDeploy));
        assert_eq!(l.rmw.as_deref(), Some("zenoh"));
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:7800"));
        assert_eq!(l.network.ip.as_deref(), Some("10.0.2.15"));
        assert_eq!(l.components[0].class.as_deref(), Some("t::Talker"));
        let w = l.deprecation().expect("a fallback warns");
        assert!(!w.contains('\n'), "one line: {w}");
        assert!(w.contains("system.toml"), "names the file to write: {w}");
        assert!(w.contains("board = \"freertos\""), "{w}");
    }

    #[test]
    fn a_lone_deploy_table_names_the_board_but_is_marked_as_such() {
        let d = leaf(&[(
            "Cargo.toml",
            &format!("{CARGO}\n[package.metadata.nros.deploy.zephyr]\nrmw = \"zenoh\"\n"),
        )]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.board.as_deref(), Some("zephyr"));
        assert_eq!(l.board_from, Some(BoardFrom::DeployTable));
    }

    #[test]
    fn component_entities_survive_the_fallback() {
        let d = leaf(&[(
            "Cargo.toml",
            &format!("{CARGO}\n[package.metadata.nros.component]\nentities = [\"timer\"]\n"),
        )]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.board, None);
        assert_eq!(l.declared_entities(), Some(vec!["timer".to_string()]));
    }

    #[test]
    fn a_manifest_with_no_nros_metadata_declares_nothing() {
        let d = leaf(&[("Cargo.toml", CARGO)]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    #[test]
    fn both_spellings_present_is_refused_naming_each_leftover() {
        let d = leaf(&[
            (
                "Cargo.toml",
                &format!(
                    "{CARGO}\n[package.metadata.nros.entry]\ndeploy = \"native\"\n\n\
                     [package.metadata.nros.deploy.native]\nrmw = \"zenoh\"\n"
                ),
            ),
            ("system.toml", SYSTEM),
        ]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("[package.metadata.nros.entry] deploy"), "{e}");
        assert!(e.contains("[package.metadata.nros.deploy.native]"), "{e}");
        assert!(e.contains("system.toml"), "{e}");
    }

    #[test]
    fn an_entry_table_without_deploy_is_not_a_conflict() {
        // `max_callbacks` / `node_pkgs` are not deployment facts and do not
        // retire with RFC-0098.
        let d = leaf(&[
            (
                "Cargo.toml",
                &format!("{CARGO}\n[package.metadata.nros.entry]\nmax_callbacks = 8\n"),
            ),
            ("system.toml", SYSTEM),
        ]);
        assert!(read(d.path()).unwrap().is_some());
    }

    #[test]
    fn several_images_need_a_default() {
        let two = format!("{SYSTEM}\n[image.native]\nboard = \"native\"\n");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &two)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("default_images"), "{e}");

        let chosen = two.replace("[system]\n", "[system]\ndefault_images = [\"native\"]\n");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &chosen)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.board.as_deref(), Some("native"));
        // `native` sets no locator of its own: the system default applies.
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:7447"));
    }

    #[test]
    fn an_image_without_a_board_is_refused() {
        let bad = SYSTEM.replace("board = \"esp32-c3-baremetal\"\n", "");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        assert!(read(d.path()).unwrap_err().contains("names no `board`"));
    }

    #[test]
    fn image_defaults_fill_what_the_image_leaves_out() {
        let with_defaults = format!(
            "{}\n[image_defaults]\nrmw = \"xrce\"\nnetmask = \"255.255.255.0\"\n",
            SYSTEM
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &with_defaults)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.rmw.as_deref(), Some("xrce"));
        assert_eq!(l.network.netmask.as_deref(), Some("255.255.255.0"));
    }

    #[test]
    fn a_malformed_entities_list_names_the_file() {
        let bad = SYSTEM.replace(
            "entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"]",
            "entities = \"timer\"",
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("system.toml") && e.contains("ARRAY"), "{e}");
    }
}

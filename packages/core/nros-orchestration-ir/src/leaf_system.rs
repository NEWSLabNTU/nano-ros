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
//! # An ENTRY inside a workspace — the bringup's image states it
//!
//! A workspace entry package (a hand-written Zephyr west application, a test
//! fixture's entry, and the entry `nros build` GENERATES under
//! `build/<coord>/<id>_entry/`) is not its own image: the bringup's
//! `[image.<id>]` is (RFC-0065 D6, RFC-0098 D5). [`for_entry`] finds the image
//! that claims an entry — `entry = "<pkg>"`, else the `<id>_entry` name the
//! builder gives a generated one ([`entry_package_name`]) — and answers from it,
//! through the same overlay as a leaf's `system.toml`. A `system.toml` beside a
//! workspace entry would make `nros build` read it as a second bringup, so the
//! bringup is the only place such an entry's deployment can live.
//!
//! # The retired manifest keys are REFUSED
//!
//! Before RFC-0098 a Rust leaf spelled all of this in `Cargo.toml`:
//! `[package.metadata.nros.entry] deploy`, `[package.metadata.nros.deploy.<b>]`,
//! `[package.metadata.nros.node]` and `[package.metadata.nros.component]`.
//! phase-445 W3/W3b read them through a deprecated fallback while the leaves
//! were converted; W5 converted the last workspace entries and deleted it. The
//! deployment keys are now an ERROR wherever they appear ([`read`]) — naming
//! the file to write — because a key nothing reads is a fact that silently
//! stopped being true. `[package.metadata.nros.node]` / `component` on a
//! workspace NODE package are the metadata pipeline's (`nros sync`), not a
//! deployment, and are not this reader's business.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The file a single-package leaf states its deployment in.
pub const SYSTEM_TOML: &str = "system.toml";

/// Where a [`LeafSystem`] was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `<leaf>/system.toml` — the RFC-0098 home.
    SystemToml(PathBuf),
    /// A workspace bringup's `system.toml`, whose `[image.<id>]` claims the
    /// entry ([`for_entry`]).
    Bringup(PathBuf),
}

/// The package name `nros build` gives the entry it generates for
/// `[image.<id>]` — `native_robot1` → `native_robot1_entry`.
///
/// Lives here, not in the CLI's builder, because [`for_entry`] (and so
/// `nros::main!`, which cannot depend on the CLI) must map a generated entry
/// back to its image by the SAME rule the builder named it by. Two copies of
/// this rule would be a generated entry that cannot find its own image.
#[must_use]
pub fn entry_package_name(image_id: &str) -> String {
    format!("{}_entry", image_id.replace(['-', '.', '/'], "_"))
}

/// The link kinds `[image.<id>] transport` may name — the same three
/// `nros_platform_config::platform_config::TRANSPORT_KINDS` defines
/// (RFC-0086 D2).
///
/// Restated rather than imported: this crate is what the `nros::main!`
/// proc-macro reads, so it carries no dependencies. An unknown value is an
/// ERROR here for RFC-0086 D2's own reason — a typo that silently selected
/// nothing would leave every implication unapplied and the image would build
/// with the wrong links on, which is exactly what
/// `examples/mps2-an385-baremetal/rust/talker-xrce` did: it named the RMW
/// (`transport = "xrce"`) where the link kind goes, and nothing read the key,
/// so nothing said so.
pub const TRANSPORT_KINDS: &[&str] = &["serial", "tcp", "udp"];

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
    /// `package.xml` name of the package providing the class. `None` when the
    /// row leaves it out (the leaf's own package, then).
    pub pkg: Option<String>,
    pub class: Option<String>,
    pub name: Option<String>,
    /// Declared entities, in `EntityDecl::parse` grammar, for a board whose
    /// component cannot be host-probed (issue 1265). `None` = not declared;
    /// `Some(vec![])` = declared to have none.
    pub entities: Option<Vec<String>>,
    /// Dispatch strategy (`"inline"` | `"deferred"` | `"from_isr"`,
    /// phase-216 A.5), replacing `[package.metadata.nros.node] dispatch`
    /// (issue 1278). Read by `nros check`'s framework × dispatch lint; `None`
    /// = the trait default. Validated there, not here, so the lint keeps its
    /// one diagnostic for a typo.
    pub dispatch: Option<String>,
}

/// A leaf's resolved deployment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafSystem {
    pub origin: Origin,
    /// The `[image.<id>]` this answer describes.
    pub image: Option<String>,
    /// The board the image is built for, as the image names it.
    pub board: Option<String>,
    /// RMW: `[image] rmw` > `[image_defaults] rmw` > `[system] rmw`.
    pub rmw: Option<String>,
    pub network: Network,
    pub components: Vec<LeafComponent>,
    /// `[image.<id>] env` — the RFC-0049 APP rung, stated per image
    /// (RFC-0098 D4, phase-445 W6).
    ///
    /// A build knob whose value is neither a board fact (it differs between two
    /// images of the same board) nor derivable from the declarations. The KEY is
    /// the knob's env front-end — the spelling `executor_env_key` /
    /// `xrce_env_key` already publish and every build script already reads — so
    /// this rung needs no second name for anything.
    ///
    /// It replaces the `[env]` block a leaf used to hand-write in its own
    /// `.cargo/config.toml`, and it lands in `build/<image>/nros-cargo.toml`'s
    /// `[env]` WITHOUT `force`, so the ladder still holds: a lane that exports
    /// the variable outranks it.
    ///
    /// Reach for a board `[board.knobs]` FIRST. This is the rung for a fact
    /// that is genuinely this image's — `examples/mps2-an385-baremetal/rust/
    /// talker-xrce`'s XRCE transport budget, on a board whose other twelve
    /// images want the defaults.
    pub env: BTreeMap<String, String>,
}

impl LeafSystem {
    /// The file the answer came from.
    pub fn origin_path(&self) -> &Path {
        match &self.origin {
            Origin::SystemToml(p) | Origin::Bringup(p) => p,
        }
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
/// * `<dir>/system.toml` beside a package manifest → read from it.
/// * otherwise `None` — a workspace entry's deployment is its bringup's image
///   ([`for_entry`]).
///
/// Either way a retired deployment key left in `Cargo.toml` is an ERROR naming
/// where the fact lives now: with the fallback gone nothing reads it, and a key
/// nothing reads is a board choice that silently stopped being made.
///
/// A `system.toml` in a directory with no `Cargo.toml`/`CMakeLists.txt` is a
/// workspace BRINGUP, not a leaf, and yields `None` here: its images are
/// chosen by the workspace builder, not by this reader.
pub fn read(dir: &Path) -> Result<Option<LeafSystem>, String> {
    let system = dir.join(SYSTEM_TOML);
    refuse_retired_manifest_keys(dir)?;
    if system.is_file() && is_package_dir(dir) {
        return from_system_toml(&system).map(Some);
    }
    Ok(None)
}

/// The deployment of the ENTRY package at `entry_dir` (package name
/// `entry_pkg`), as stated by the bringup at `bringup_dir`: the `[image.<id>]`
/// that claims it.
///
/// An image claims an entry by naming it — `entry = "<pkg>"` — or, when it
/// names none, by being the image the builder would GENERATE that entry for
/// ([`entry_package_name`]`(id) == entry_pkg`). The directory name is accepted
/// beside the package name for both, because `entry =` has always accepted
/// either (`west_application_dir`).
///
/// `Ok(None)` when no image claims it. SEVERAL claiming it is an error, not a
/// first match: which image's locator an entry bakes is not a coin toss.
///
/// A leaf `system.toml` beside the entry answers first ([`read`]) — an entry
/// that states its own deployment is a single-package leaf, whatever encloses
/// it.
pub fn for_entry(
    entry_dir: &Path,
    entry_pkg: &str,
    bringup_dir: &Path,
) -> Result<Option<LeafSystem>, String> {
    if let Some(own) = read(entry_dir)? {
        return Ok(Some(own));
    }
    let path = bringup_dir.join(SYSTEM_TOML);
    if !path.is_file() {
        return Ok(None);
    }
    let doc = parse_file(&path)?;
    let dir_name = entry_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let names = |s: &str| s == entry_pkg || (!dir_name.is_empty() && s == dir_name);
    let images = as_table(doc.get("image")).cloned().unwrap_or_default();
    let mut explicit = Vec::new();
    let mut by_name = Vec::new();
    for (id, block) in &images {
        match str_key(block.as_table(), "entry") {
            Some(e) if names(&e) => explicit.push(id.clone()),
            // An image that names a DIFFERENT entry is that entry's, even when
            // its id happens to derive this one's name.
            Some(_) => {}
            None if names(&entry_package_name(id)) => by_name.push(id.clone()),
            None => {}
        }
    }
    let claims = if explicit.is_empty() {
        by_name
    } else {
        explicit
    };
    match claims.as_slice() {
        [] => Ok(None),
        [one] => image_system(&doc, &path, one, Origin::Bringup(path.clone())).map(Some),
        many => Err(format!(
            "{}: {} images claim the entry `{entry_pkg}` ({}) — an entry is ONE image's \
             program; name it from exactly one with `entry = \"{entry_pkg}\"`",
            path.display(),
            many.len(),
            many.join(", ")
        )),
    }
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
    image_system(&doc, path, &id, Origin::SystemToml(path.to_path_buf()))
}

/// `[image.<id>]` of the `system.toml` document `doc` (read from `path`),
/// resolved: the image over `[image_defaults]` over `[system]`. Shared by a
/// leaf's own file ([`from_system_toml`]) and a bringup's image for one of its
/// entries ([`for_entry`]) — one overlay, whichever file states it.
fn image_system(
    doc: &toml::Table,
    path: &Path,
    id: &str,
    origin: Origin,
) -> Result<LeafSystem, String> {
    let system = as_table(doc.get("system"));
    let defaults = as_table(doc.get("image_defaults"));
    let image = as_table(doc.get("image")).and_then(|t| as_table(t.get(id)));
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
    let transport = pick("transport");
    if let Some(t) = &transport
        && !TRANSPORT_KINDS.contains(&t.as_str())
    {
        return Err(format!(
            "{}: `[image.{id}] transport = \"{t}\"` is not a link kind — it is one of {} \
             (RFC-0086 D2). The RMW is `rmw = \"…\"`, which is a different choice.",
            path.display(),
            TRANSPORT_KINDS.join(", ")
        ));
    }
    let network = Network {
        domain_id,
        locator: pick("locator").or_else(|| str_key(system, "locator")),
        ip: pick("ip"),
        gateway: pick("gateway"),
        netmask: pick("netmask"),
        transport,
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
                dispatch: str_key(Some(t), "dispatch"),
            });
        }
    }

    // `[image.<id>] env` over `[image_defaults] env`, key by key — the same
    // overlay `ImageBlock::with_base` gives a map: the base is a default SET,
    // and the image overwrites only what it also names.
    let mut env = env_table(defaults, path, id)?;
    env.extend(env_table(image, path, id)?);

    Ok(LeafSystem {
        origin,
        image: Some(id.to_string()),
        board: Some(board),
        rmw,
        network,
        components,
        env,
    })
}

/// `env = { KEY = "VALUE" }` of one image block.
///
/// Values are strings, always: an environment variable IS a string, and
/// accepting an integer here would make `X = 2` and `X = "2"` two spellings of
/// one row that a later `deny_unknown_fields` schema would have to keep
/// agreeing about. The error says which spelling to write.
fn env_table(
    block: Option<&toml::Table>,
    path: &Path,
    id: &str,
) -> Result<BTreeMap<String, String>, String> {
    let Some(t) = block.and_then(|b| b.get("env")) else {
        return Ok(BTreeMap::new());
    };
    let t = t.as_table().ok_or_else(|| {
        format!(
            "{}: `[image.{id}] env` must be a TABLE of `KEY = \"VALUE\"` rows",
            path.display()
        )
    })?;
    t.iter()
        .map(|(k, v)| {
            v.as_str().map(|s| (k.clone(), s.to_string())).ok_or_else(|| {
                format!(
                    "{}: `[image.{id}] env` row `{k}` is a {}, not a string — an environment \
                     value is a string, so write `{k} = \"{v}\"`",
                    path.display(),
                    v.type_str()
                )
            })
        })
        .collect()
}

/// The retired DEPLOYMENT keys a manifest table still carries, by name.
///
/// `[package.metadata.nros.node]` / `component` are not among them: on a
/// single-package leaf they retired with W3b (and `check-leaf-deployment-
/// spelling` refuses them there), but on a workspace node package they are the
/// metadata pipeline's declaration of a class, not a deployment, and the
/// deployment reader has no business refusing them.
#[must_use]
pub fn retired_deployment_keys(nros: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    if as_table(nros.get("entry")).is_some_and(|e| e.contains_key("deploy")) {
        out.push("[package.metadata.nros.entry] deploy".to_string());
    }
    if let Some(d) = as_table(nros.get("deploy")) {
        for k in d.keys() {
            out.push(format!("[package.metadata.nros.deploy.{k}]"));
        }
    }
    out
}

/// Refuse a manifest still carrying a retired deployment key, naming each one
/// and where the fact lives now. Nothing reads those keys any more
/// (phase-445 W5 deleted the fallback), so leaving one is worse than an error:
/// it READS like the board choice and is not.
fn refuse_retired_manifest_keys(dir: &Path) -> Result<(), String> {
    let manifest = dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let doc = parse_file(&manifest)?;
    let Some(nros) = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.as_table())
    else {
        return Ok(());
    };
    let left = retired_deployment_keys(nros);
    if left.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} carries {} — retired (RFC-0098 D3/D5, phase-445 W5): nothing reads it. State the \
         deployment as `[image.<id>] board = \"<board>\"` (+ `rmw`, `domain_id`, `locator`, …) \
         in {} for a single-package example, or in the workspace bringup's `system.toml` image \
         that builds this entry (`entry = \"<pkg>\"`), and delete the table.",
        manifest.display(),
        left.join(", "),
        dir.join(SYSTEM_TOML).display(),
    ))
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
dispatch = "deferred"

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
        assert!(matches!(l.origin, Origin::SystemToml(_)));
        assert_eq!(l.image.as_deref(), Some("esp32"));
        assert_eq!(l.board.as_deref(), Some("esp32-c3-baremetal"));
        assert_eq!(l.rmw.as_deref(), Some("zenoh"));
        // The image's locator beats the system default; the domain falls
        // through to `[system]`.
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:9800"));
        assert_eq!(l.network.domain_id, Some(3));
        assert_eq!(l.network.ip.as_deref(), Some("10.0.2.50"));
        assert_eq!(l.network.netmask, None);
        assert_eq!(l.components.len(), 1);
        assert_eq!(l.components[0].pkg.as_deref(), Some("esp32_talker"));
        // Issue 1278 — the dispatch strategy has a home here too.
        assert_eq!(l.components[0].dispatch.as_deref(), Some("deferred"));
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
    fn a_retired_deployment_key_is_refused_naming_where_the_fact_lives() {
        // The fallback that read these is gone (phase-445 W5). A manifest still
        // carrying one must not resolve to "no deployment" and then fail far
        // away as "declares no board" — it is refused where it is.
        for (tables, key) in [
            (
                "[package.metadata.nros.entry]\ndeploy = \"freertos\"\n",
                "[package.metadata.nros.entry] deploy",
            ),
            (
                "[package.metadata.nros.deploy.zephyr]\nrmw = \"zenoh\"\n",
                "[package.metadata.nros.deploy.zephyr]",
            ),
        ] {
            let d = leaf(&[("Cargo.toml", &format!("{CARGO}\n{tables}"))]);
            let e = read(d.path()).unwrap_err();
            assert!(e.contains(key), "names the key: {e}");
            assert!(e.contains("system.toml"), "names where to write: {e}");
            assert!(!e.contains('\n'), "one line: {e}");
        }
    }

    #[test]
    fn node_and_component_tables_are_not_a_deployment() {
        // A workspace NODE package declares its class for the metadata
        // pipeline; that is not this reader's refusal to make.
        let d = leaf(&[(
            "Cargo.toml",
            &format!(
                "{CARGO}\n[package.metadata.nros.node]\nclass = \"t::Talker\"\n\
                 [package.metadata.nros.component]\nentities = [\"timer\"]\n"
            ),
        )]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    #[test]
    fn a_manifest_with_no_nros_metadata_declares_nothing() {
        let d = leaf(&[("Cargo.toml", CARGO)]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    const BRINGUP: &str = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image_defaults]
rmw = "zenoh"

[image.esp32]
board = "esp32-c3-baremetal"
locator = "tcp/10.0.2.2:9830"

[image.zephyr]
board = "zephyr"
entry = "zephyr_entry"
locator = "tcp/10.0.2.2:7430"

[image.zephyr_robot1]
board = "zephyr"
entry = "zephyr_entry_robot1"
domain_id = 4
"#;

    /// `<ws>/src/{demo_bringup,<entry>}` with the bringup's `system.toml`.
    fn workspace(entry: &str, entry_manifest: &str) -> (tempdir::Dir, PathBuf, PathBuf) {
        let d = tempdir::Dir::new();
        let bringup = d.path().join("src/demo_bringup");
        let e = d.path().join("src").join(entry);
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::create_dir_all(&e).unwrap();
        std::fs::write(bringup.join("system.toml"), BRINGUP).unwrap();
        std::fs::write(e.join("Cargo.toml"), entry_manifest).unwrap();
        (d, bringup, e)
    }

    #[test]
    fn a_generated_entry_finds_the_image_it_was_generated_for() {
        // `nros build` names the entry `<id>_entry`; that name is the claim.
        let (_d, bringup, e) = workspace("esp32_entry", CARGO);
        let l = for_entry(&e, "esp32_entry", &bringup)
            .unwrap()
            .expect("claimed");
        assert_eq!(l.image.as_deref(), Some("esp32"));
        assert_eq!(l.board.as_deref(), Some("esp32-c3-baremetal"));
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:9830"));
        // `[system]` fills what the image leaves out — the leaf overlay.
        assert_eq!(l.network.domain_id, Some(0));
        assert!(matches!(l.origin, Origin::Bringup(_)));
    }

    #[test]
    fn an_explicit_entry_key_claims_and_beats_the_derived_name() {
        let (_d, bringup, e) = workspace("zephyr_entry_robot1", CARGO);
        let l = for_entry(&e, "zephyr_entry_robot1", &bringup)
            .unwrap()
            .expect("claimed");
        assert_eq!(l.image.as_deref(), Some("zephyr_robot1"));
        assert_eq!(l.network.domain_id, Some(4));
        // `[image.zephyr] entry = "zephyr_entry"` derives `zephyr_entry` from
        // its id too, and names it — ONE claim, not two.
        let (_d, bringup, e) = workspace("zephyr_entry", CARGO);
        let l = for_entry(&e, "zephyr_entry", &bringup).unwrap().unwrap();
        assert_eq!(l.image.as_deref(), Some("zephyr"));
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:7430"));
    }

    #[test]
    fn an_entry_no_image_claims_has_no_deployment() {
        let (_d, bringup, e) = workspace("unrelated_entry", CARGO);
        assert_eq!(for_entry(&e, "unrelated_entry", &bringup).unwrap(), None);
    }

    #[test]
    fn two_images_claiming_one_entry_is_refused() {
        let (_d, bringup, e) = workspace("zephyr_entry", CARGO);
        let two =
            format!("{BRINGUP}\n[image.zephyr_b]\nboard = \"zephyr\"\nentry = \"zephyr_entry\"\n");
        std::fs::write(bringup.join("system.toml"), two).unwrap();
        let err = for_entry(&e, "zephyr_entry", &bringup).unwrap_err();
        assert!(err.contains("zephyr, zephyr_b"), "{err}");
    }

    #[test]
    fn an_entry_still_on_a_retired_key_is_refused_even_when_an_image_claims_it() {
        let (_d, bringup, e) = workspace(
            "zephyr_entry",
            &format!("{CARGO}\n[package.metadata.nros.entry]\ndeploy = \"zephyr\"\n"),
        );
        let err = for_entry(&e, "zephyr_entry", &bringup).unwrap_err();
        assert!(
            err.contains("[package.metadata.nros.entry] deploy"),
            "{err}"
        );
    }

    #[test]
    fn the_entry_name_rule_is_the_builders() {
        assert_eq!(entry_package_name("native_robot1"), "native_robot1_entry");
        assert_eq!(entry_package_name("esp32-qemu"), "esp32_qemu_entry");
        assert_eq!(entry_package_name("a.b/c"), "a_b_c_entry");
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

    /// phase-445 W6 — the APP rung. `[image.<id>] env` is what replaced the
    /// `[env]` block a leaf hand-wrote in its own `.cargo/config.toml`.
    #[test]
    fn an_image_states_its_own_build_knobs() {
        let with_env = format!(
            "{SYSTEM}\nenv = {{ NROS_XRCE_BUFFER_SIZE = \"256\", \
             NROS_XRCE_CUSTOM_TRANSPORT_MTU = \"512\" }}\n"
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &with_env)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.env["NROS_XRCE_BUFFER_SIZE"], "256");
        assert_eq!(l.env["NROS_XRCE_CUSTOM_TRANSPORT_MTU"], "512");
        // A leaf that states none has an empty table, never an absent one.
        let plain = leaf(&[("Cargo.toml", CARGO), ("system.toml", SYSTEM)]);
        assert!(read(plain.path()).unwrap().unwrap().env.is_empty());
    }

    #[test]
    fn image_defaults_env_is_a_default_set_the_image_overrides_key_by_key() {
        let both = format!(
            "{SYSTEM}\nenv = {{ A = \"image\" }}\n\n\
             [image_defaults]\nenv = {{ A = \"base\", B = \"base\" }}\n"
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &both)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.env["A"], "image", "the image wins the key it also names");
        assert_eq!(l.env["B"], "base", "and inherits the one it does not");
    }

    #[test]
    fn a_non_string_env_value_names_the_spelling_to_write() {
        let bad = format!("{SYSTEM}\nenv = {{ N = 256 }}\n");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("N = \"256\""), "{e}");
    }

    /// RFC-0086 D2 — a transport that is not a link kind is an ERROR, not a
    /// pass-through. `talker-xrce` named its RMW here for four phases.
    #[test]
    fn a_transport_that_is_not_a_link_kind_is_refused() {
        let bad = SYSTEM.replace(
            "[image.esp32]",
            "[image.esp32]\ntransport = \"xrce\"\n#",
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("serial, tcp, udp"), "{e}");

        let good = bad.replace("transport = \"xrce\"", "transport = \"serial\"");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &good)]);
        assert_eq!(
            read(d.path()).unwrap().unwrap().network.transport.as_deref(),
            Some("serial")
        );
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

//! phase-531 W4 (RFC-0121 D4) — `nros new` writes a workspace.
//!
//! Outside a workspace, `nros new <ws> --board <b> --template <t>` writes
//!
//! ```text
//! <ws>/
//! └── src/
//!     ├── <node>/            # node package: a library, no main, no board
//!     └── <node>_bringup/    # package.xml and system.toml; no launch/
//! ```
//!
//! `<node>` is the workspace name less a `_ws` suffix, plus `_node`
//! (`sensor_ws` → `sensor_node`). The bringup is named for its node
//! (`<node>_bringup`, RFC-0121's open question, settled here): the name
//! survives a second node without a rename, and it is the ROS convention a
//! reader already knows. The bringup's `system.toml` names the board, the RMW,
//! the one component and one image; its launch file is generated (RFC-0121
//! D3), so the reader never opens it to get output.
//!
//! Inside a workspace ([`enclosing_workspace`]), `nros new <node>` adds the
//! node package under `src/`, and with `--to <bringup>` also its
//! `[[component]]` row and an `<exec_depend>` in the bringup's `package.xml`.
//! Without `--to` it touches no bringup.

use std::path::{Path, PathBuf};

use cargo_nano_ros::scaffold::{ComponentScaffoldConfig, PackageMeta, Role, scaffold_component_at};
use eyre::{Result, WrapErr, bail};

/// The workspace `cwd` is inside: `cwd` or its nearest ancestor holding a
/// `nros.toml`, or a `src/` directory with a package (`src/<pkg>/package.xml`)
/// one level below. `None` outside any.
#[must_use]
pub fn enclosing_workspace(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|d| {
            d.join("nros.toml").is_file()
                || d.join(WORKSPACE_MARKER).is_file()
                || holds_packages(&d.join("src"))
        })
        .map(Path::to_path_buf)
}

fn holds_packages(src: &Path) -> bool {
    std::fs::read_dir(src)
        .is_ok_and(|rd| rd.flatten().any(|e| e.path().join("package.xml").is_file()))
}

/// The node package a new workspace holds: the workspace's name less a `_ws`
/// suffix, plus `_node` (`sensor_ws` → `sensor_node`, `demo` → `demo_node`).
#[must_use]
pub fn node_package_name(ws_name: &str) -> String {
    let stem = ws_name.strip_suffix("_ws").unwrap_or(ws_name);
    let stem = stem.replace('-', "_");
    if stem.ends_with("_node") {
        stem
    } else {
        format!("{stem}_node")
    }
}

/// The bringup a new workspace's node is deployed by.
#[must_use]
pub fn bringup_name(node: &str) -> String {
    // A ROS package name is `[a-z][a-z0-9_]*` (REP 144): a dashed node
    // package name (`sim-clock-publisher`, which cargo accepts) still gets a
    // bringup ROS tooling can name.
    format!("{}_bringup", node.replace('-', "_"))
}

/// The node package's facts, shared by both modes.
pub struct NodePackage<'a> {
    /// Package name (and crate / CMake project name).
    pub name: &'a str,
    pub template: Role,
    /// The node's ROS name; `None` is the package name.
    pub node_name: Option<&'a str>,
    pub meta: PackageMeta,
    pub lang: nros_lang::Language,
    pub force: bool,
}

impl NodePackage<'_> {
    fn ros_name(&self) -> String {
        self.node_name
            .map_or_else(|| self.name.replace('-', "_"), str::to_string)
    }

    /// The `[[component]]` row's `class`: `<crate>::<Class>` in every language
    /// (the C/C++ namespace is the sanitised package name, as the scaffold
    /// writes it).
    fn class(&self) -> String {
        format!("{}::{}", self.name.replace('-', "_"), self.template.class())
    }

    fn write(&self, dir: &Path) -> Result<()> {
        scaffold_component_at(
            dir,
            &ComponentScaffoldConfig {
                name: self.name.to_string(),
                template: self.template,
                node_name: Some(self.ros_name()),
                meta: self.meta.clone(),
                lang: self.lang,
                force: self.force,
                workspace_member: true,
            },
        )
    }
}

/// What `nros new <ws> --board <b>` writes beyond the node package.
pub struct WorkspaceProject<'a> {
    /// The workspace directory to create.
    pub dir: PathBuf,
    pub node: NodePackage<'a>,
    /// The board as written (`--board`, or the platform's default board).
    pub board: &'a str,
    /// The `[image.<id>]` id: the board's platform word (`baremetal`,
    /// `linux`), as the single-package scaffold named it.
    pub image_id: &'a str,
    pub rmw: &'a str,
    pub ros_edition: &'a str,
    /// The image's `locator`, from the board's `[board.net] host_serial`.
    pub locator: Option<String>,
    /// The main stack the image states through its `os_config`, from
    /// [`template_main_stack`]: `Some` only when the template's measured
    /// floor is above the stack the image would otherwise get.
    pub main_stack: Option<u64>,
}

/// The file that says "this directory is a workspace root" to every reader that
/// walks up from inside it: `nros::main!`'s pkg index (`nros_pkg_index::
/// detect_workspace_root`) and [`enclosing_workspace`]. Every in-tree workspace
/// tracks one (phase-383 W10.a).
pub const WORKSPACE_MARKER: &str = ".colcon_workspace";

const WORKSPACE_MARKER_BODY: &str = "\
# This directory is a workspace ROOT. `nros::main!` walks up from a generated
# entry under build/ to find the packages; without this marker, on a road
# that names no workspace (a bare `west build`, a bare `cargo` run inside an
# entry), it stopped at the entry's own `[workspace]` table and found none.
";

/// `nros new <ws> --board <b>`: the workspace, its node package and its
/// bringup. Returns the bringup directory.
pub fn scaffold_project(p: &WorkspaceProject<'_>) -> Result<PathBuf> {
    if p.dir.exists() && !p.node.force {
        bail!(
            "nros new: {} already exists (use --force to overwrite its packages)",
            p.dir.display()
        );
    }
    let src = p.dir.join("src");
    let node_dir = src.join(p.node.name);
    let bringup = bringup_name(p.node.name);
    let bringup_dir = src.join(&bringup);
    std::fs::create_dir_all(&src).wrap_err_with(|| format!("creating {}", src.display()))?;
    p.node.write(&node_dir)?;
    if bringup_dir.exists() {
        std::fs::remove_dir_all(&bringup_dir)
            .wrap_err_with(|| format!("replacing {}", bringup_dir.display()))?;
    }
    std::fs::create_dir_all(&bringup_dir)?;
    std::fs::write(
        bringup_dir.join("package.xml"),
        bringup_package_xml(&bringup, p.node.name),
    )?;
    std::fs::write(bringup_dir.join("system.toml"), bringup_system_toml(p))?;
    if let Some(bytes) = p.main_stack {
        std::fs::write(bringup_dir.join(STACK_CONF), stack_conf(p, bytes))?;
    }
    if let Some(contract) = bringup_contract(&p.node) {
        std::fs::write(bringup_dir.join(crate::cmd::ws::LEAF_CONTRACT), contract)?;
    }
    std::fs::write(
        p.dir.join(".gitignore"),
        "/build/\n/generated/\n/target/\n/.cargo/\nCargo.lock\n/src/*/metadata/\n",
    )?;
    std::fs::write(p.dir.join(WORKSPACE_MARKER), WORKSPACE_MARKER_BODY)?;
    Ok(bringup_dir)
}

/// The bringup's `system.toml`: the board, the RMW, the one component and one
/// image (RFC-0121 D4). No `launch/` beside it: its launch file is generated
/// from the `[[component]]` row (D3).
fn bringup_system_toml(p: &WorkspaceProject<'_>) -> String {
    let locator = p
        .locator
        .as_deref()
        .map(|l| format!("locator = \"{l}\"\n"))
        .unwrap_or_default();
    format!(
        "# What this workspace runs, and on what. Its launch file is\n\
         # generated from the [[component]] rows; add launch/system.launch.xml to\n\
         # this package when you need a launch argument, a remap or a parameter.\n\
         \n[system]\nname = \"{name}\"\nrmw = \"{rmw}\"\nros_edition = \"{ed}\"\ndomain_id = 0\n\
         \n[[component]]\npkg = \"{pkg}\"\nclass = \"{class}\"\nname = \"{ros}\"\n\
         \n[image.{id}]\nboard = \"{board}\"\n{locator}{os_config}\
         # The stack allowance this image is measured under: the\n\
         # template it was made from. Remove it once the image no longer is one.\n\
         stack_floor_image = \"{floor_key}\"\n",
        name = p.node.name,
        rmw = p.rmw,
        ed = p.ros_edition,
        pkg = p.node.name,
        class = p.node.class(),
        ros = p.node.ros_name(),
        id = p.image_id,
        board = p.board,
        floor_key = stack_floor_template_key(p.node.template, p.node.lang, p.rmw, p.board),
        os_config = if p.main_stack.is_some() {
            format!("os_config = [\"{STACK_CONF}\"]\n")
        } else {
            String::new()
        },
    )
}

/// The `os_config` fragment that raises the main stack to the template's
/// measured floor (issue 2400, ruling (A)(3)).
const STACK_CONF: &str = "stack.conf";

fn stack_conf(p: &WorkspaceProject<'_>, bytes: u64) -> String {
    format!(
        "# The main stack this image needs: the floor measured for the template it\n\
         # was made from ({key}, config/stack-floors.toml in nano-ros), which\n\
         # is above the stack the module gives this board by default. The build\n\
         # CHECKs it against this image's own frames (issue 2400) and says when it\n\
         # must move; it never lowers it.\n\
         CONFIG_MAIN_STACK_SIZE={bytes}\n",
        key = stack_floor_template_key(p.node.template, p.node.lang, p.rmw, p.board),
    )
}

/// The main stack a `nros new` image must state, or `None` when it needs none
/// (issue 2400 W4; ruling (A)(3): an image raises its own stack through its
/// `os_config`). `Some(floor)` only when ALL hold:
///
/// * the board is a Zephyr board (the knob is `CONFIG_MAIN_STACK_SIZE`);
/// * `config/stack-floors.toml` has an OPEN row for `key` recording the
///   static walk it was measured on (`[allowance.measured] static`);
/// * that row's floor is above the stack the image gets without one
///   ([`zephyr_main_stack_default`]).
///
/// The number is DERIVED from the row, by the same formula
/// `scripts/nros-stack-floor.py`'s `floor_formula` uses, so a re-measured row
/// moves what the next `nros new` writes. It is never below the default:
/// nothing lowers a stack.
#[must_use]
pub fn template_main_stack(
    root: &Path,
    key: &str,
    zephyr: bool,
    rust: bool,
    rmw: &str,
    board: &str,
) -> Option<u64> {
    if !zephyr {
        return None;
    }
    let table = std::fs::read_to_string(root.join("config/stack-floors.toml")).ok()?;
    let floor = recorded_floor(&table, key)?;
    let default = zephyr_main_stack_default(root, board, rust, rmw)?;
    (floor > default).then_some(floor)
}

/// The OPEN floor `config/stack-floors.toml` records for `key`:
/// `ceil8(ceil((static + allowance) * (100 + margin_pct) / 100))`, exactly
/// `floor_formula` in `scripts/nros-stack-floor.py`.
#[must_use]
pub fn recorded_floor(table: &str, key: &str) -> Option<u64> {
    let doc: toml::Value = toml::from_str(table).ok()?;
    let margin = doc.get("margin_pct")?.as_integer()?;
    doc.get("allowance")?
        .as_array()?
        .iter()
        .filter(|a| {
            a.get("image").and_then(|v| v.as_str()) == Some(key)
                && a.get("role").and_then(|v| v.as_str()) == Some("OPEN")
        })
        .filter_map(|a| {
            let bytes = a.get("bytes")?.as_integer()?;
            let stat = a.get("measured")?.get("static")?.as_integer()?;
            let raw = (stat + bytes).max(0) * (100 + margin);
            let raw = (raw + 99) / 100;
            u64::try_from((raw + 7) / 8 * 8).ok()
        })
        .max()
}

/// The `CONFIG_MAIN_STACK_SIZE` a Zephyr image gets with no `os_config` of its
/// own: the last value the board package's confs state (`prj.conf`, then
/// `boards/*.conf`), else the nano-ros module's `configdefault MAIN_STACK_SIZE`
/// in its `if NROS_RMW_<RMW>` block of `zephyr/Kconfig`, READ there so it
/// cannot drift from the default it describes. `None` when neither answers,
/// or a `default` carries a condition this reader does not know.
#[must_use]
pub fn zephyr_main_stack_default(root: &Path, board: &str, rust: bool, rmw: &str) -> Option<u64> {
    let pkg = root
        .join("packages/boards")
        .join(format!("nros-board-{board}"));
    let mut confs = vec![pkg.join("prj.conf")];
    if let Ok(rd) = std::fs::read_dir(pkg.join("boards")) {
        let mut more: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "conf"))
            .collect();
        more.sort();
        confs.extend(more);
    }
    let stated = confs
        .iter()
        .filter_map(|c| std::fs::read_to_string(c).ok())
        .flat_map(|t| {
            t.lines()
                .filter_map(|l| {
                    l.trim()
                        .strip_prefix("CONFIG_MAIN_STACK_SIZE=")?
                        .trim()
                        .parse()
                        .ok()
                })
                .collect::<Vec<u64>>()
        })
        .last();
    if stated.is_some() {
        return stated;
    }
    let kconfig = std::fs::read_to_string(root.join("zephyr/Kconfig")).ok()?;
    let open = format!("if NROS_RMW_{}", rmw.to_ascii_uppercase());
    let close = format!("endif # NROS_RMW_{}", rmw.to_ascii_uppercase());
    // the module opens one `if NROS_RMW_<RMW>` block several times; the
    // default is in whichever holds it
    let mut in_block = false;
    let mut lines = kconfig.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line == open {
            in_block = true;
        } else if line == close {
            in_block = false;
        } else if in_block && line == "configdefault MAIN_STACK_SIZE" {
            for d in lines.by_ref() {
                let rest = d.strip_prefix("default ")?;
                let (value, cond) = match rest.split_once(" if ") {
                    Some((v, c)) => (v, Some(c.trim())),
                    None => (rest, None),
                };
                let value: u64 = value.trim().parse().ok()?;
                match cond {
                    None => return Some(value),
                    // a row names a cross board, never ARCH_POSIX
                    Some("NROS_RUST_API && !ARCH_POSIX") if rust => return Some(value),
                    Some("NROS_RUST_API && !ARCH_POSIX") => {}
                    Some(_) => return None,
                }
            }
            return None;
        }
    }
    None
}

/// The ONE spelling of a template's stack-floor key (issue 2400, key (c),
/// ruled 2026-11-12): `<template>/<lang>/<rmw>@<board>`, which is how
/// `config/stack-floors.toml` files the allowance measured on a `nros new`
/// image. `nros new` writes it into `[image.<id>] stack_floor_image`, so the
/// key survives a project rename; `scripts/nros-stack-floor.py
/// --validate-table` refuses a row whose template or board no longer exists.
#[must_use]
pub fn stack_floor_template_key(
    template: Role,
    lang: nros_lang::Language,
    rmw: &str,
    board: &str,
) -> String {
    format!("{}/{}/{rmw}@{board}", template.name(), lang.as_str())
}

/// The bringup's `system.contract.yaml`: what the node DECLARES, or `None`.
///
/// Issue 2270. Without one, every buffer the declarations would size keeps a
/// default or a worst case, and a scaffolded talker's first build reported
/// six of them. The scaffold states what its own code does: the talker's
/// timer path, its VOLATILE publisher, and no parameters.
///
/// **A contract that names a node is that node's ENTITY declaration, so it
/// must be complete.** The first version declared the talker's publisher and
/// not its timer, and wrote `params: {}` alone for every other role. Wherever
/// the probe sidecar is not fresh, the contract is all the derivation reads.
/// A release is one such place, because it shipped no
/// `probe-source-dirs.txt`, so every sidecar read stale. There a talker
/// counted zero callbacks, and nros-0.6.0-nros7's own First Project failed
/// to compile on `max_cbs 0` (phase-523 round two). So only the role whose
/// whole surface is written here gets a contract. The others get none, which
/// is the documented "no contract" road: they keep the board's configured
/// pool knobs until a user states theirs.
fn bringup_contract(node: &NodePackage<'_>) -> Option<String> {
    if node.template != Role::Talker {
        return None;
    }
    let ros = node.ros_name();
    let (rows, _) = contract_node_rows(node);
    Some(format!(
        "# What this image's node DECLARES, beside system.toml. The build sizes its\n\
         # buffers and its callback table from this, so it must list everything the\n\
         # node creates: when you add a timer, a publisher, a subscription or a\n\
         # parameter in the code, state it here too.\n\
         # `nros build --explain-sizing` lists anything still left to a default.\n\
         \nversion: 1\n\nnodes:\n\
         {rows}\
         \ntopics:\n\
         \x20 /chatter:\n\
         \x20   type: std_msgs/msg/String\n\
         \x20   pub: [{ros}/chatter]\n\
         \x20   # Read outside this workspace, by `ros2 topic echo` or a ROS 2 node.\n\
         \x20   external: sub\n"
    ))
}

/// One endpoint a scaffolded node joins: under the contract's top-level
/// `section` (`topics`, `services` or `actions`), entry `name` of `type_`, on
/// list `side`, as `<node>/<local>`.
struct ContractWiring {
    section: &'static str,
    name: &'static str,
    type_: &'static str,
    side: &'static str,
    local: &'static str,
}

/// A node's whole entity declaration under a contract's `nodes:`, and the
/// endpoint it wires. Every role's template is known in full, per language:
/// the timers differ (a C or C++ client, action client and action server
/// tick every 500 ms; the Rust client every second, and the Rust action
/// nodes need no timer), and a contract row that omits a timer under-sizes
/// the callback table.
fn contract_node_rows(node: &NodePackage<'_>) -> (String, ContractWiring) {
    let ros = node.ros_name();
    let rust = node.lang == nros_lang::Language::Rust;
    let tick = |name: &str, hz: u32, output: &str| {
        format!(
            "\x20   paths:\n\
             \x20     {name}:\n\
             \x20       trigger: {{ timer: {{ rate_hz: {hz} }} }}\n\
             \x20       output: [{output}]\n"
        )
    };
    let (body, wiring) = match node.template {
        Role::Talker => (
            format!(
                "{}\x20   pub:\n\
                 \x20     chatter:\n\
                 \x20       qos:\n\
                 \x20         durability: volatile\n",
                tick("on_tick", 1, "chatter")
            ),
            ContractWiring {
                section: "topics",
                name: "/chatter",
                type_: "std_msgs/msg/String",
                side: "pub",
                local: "chatter",
            },
        ),
        Role::Listener => (
            "\x20   sub:\n\
             \x20     chatter:\n\
             \x20       qos:\n\
             \x20         durability: volatile\n"
                .to_string(),
            ContractWiring {
                section: "topics",
                name: "/chatter",
                type_: "std_msgs/msg/String",
                side: "sub",
                local: "chatter",
            },
        ),
        Role::Server => (
            "\x20   srv:\n\x20     add_two_ints: {}\n".to_string(),
            ContractWiring {
                section: "services",
                name: "/add_two_ints",
                type_: "example_interfaces/srv/AddTwoInts",
                side: "server",
                local: "add_two_ints",
            },
        ),
        Role::Client => (
            format!(
                "{}\x20   cli:\n\x20     add_two_ints: {{}}\n",
                if rust {
                    tick("issue_call", 1, "")
                } else {
                    tick("on_tick", 2, "")
                }
            ),
            ContractWiring {
                section: "services",
                name: "/add_two_ints",
                type_: "example_interfaces/srv/AddTwoInts",
                side: "client",
                local: "add_two_ints",
            },
        ),
        Role::ActionServer | Role::ActionClient => (
            if rust {
                String::new()
            } else {
                tick("on_tick", 2, "")
            },
            ContractWiring {
                section: "actions",
                name: "/fibonacci",
                type_: "example_interfaces/action/Fibonacci",
                side: if node.template == Role::ActionServer {
                    "server"
                } else {
                    "client"
                },
                local: "fibonacci",
            },
        ),
    };
    (format!("\x20 {ros}:\n\x20   params: {{}}\n{body}"), wiring)
}

/// Issue 2376 -- `--to` a bringup that has a contract. A contract that
/// declares `params:` on some of an image's nodes and not others is refused
/// on the CMake road (the parameter store is sized from every node or from
/// none), so the new node's whole declaration joins it. A file whose shape
/// the edit cannot follow is reported, never half-written.
fn add_to_contract(bringup: &Path, node: &NodePackage<'_>) -> Result<()> {
    let path = bringup.join(crate::cmd::ws::LEAF_CONTRACT);
    if !path.is_file() {
        return Ok(());
    }
    let text =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("reading {}", path.display()))?;
    let ros = node.ros_name();
    let doc: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))?;
    if !doc["nodes"][ros.as_str()].is_null() {
        return Ok(());
    }
    let (rows, wiring) = contract_node_rows(node);
    let edited = insert_contract_rows(&text, &rows, &wiring, &ros).filter(|edited| {
        // Only a result that still parses, and now names the node, is written.
        serde_yaml_ng::from_str::<serde_yaml_ng::Value>(edited)
            .is_ok_and(|check| !check["nodes"][ros.as_str()].is_null())
    });
    match edited {
        Some(edited) => {
            std::fs::write(&path, edited).wrap_err_with(|| format!("writing {}", path.display()))
        }
        None => {
            eprintln!(
                "nros new: {} does not name `{ros}`. State what it creates there \
                 (its `params:`, timers and endpoints), or the build sizes the image \
                 without it.",
                path.display()
            );
            Ok(())
        }
    }
}

/// The body of top-level `key:` as `(start, end)` byte offsets: from the line
/// after `key:` to the next column-0 line, less the blank lines before it.
fn top_level_block(text: &str, key: &str) -> Option<(usize, usize)> {
    let head = format!("{key}:\n");
    let at = if text.starts_with(&head) {
        0
    } else {
        text.find(&format!("\n{head}"))? + 1
    };
    let start = at + head.len();
    let mut end = text.len();
    let mut pos = start;
    for line in text[start..].split_inclusive('\n') {
        if !line.starts_with(' ') && !line.trim().is_empty() {
            end = pos;
            break;
        }
        pos += line.len();
    }
    while end > start && text[..end].ends_with("\n\n") {
        end -= 1;
    }
    Some((start, end))
}

/// Text edit of a scaffold-shaped contract: `rows` go at the end of `nodes:`,
/// and `<ros>/<local>` joins `wiring`'s entry, which is created -- and its
/// section with it -- when missing. `None` when the file has no `nodes:`
/// block or the entry has no line to anchor on.
fn insert_contract_rows(
    text: &str,
    rows: &str,
    wiring: &ContractWiring,
    ros: &str,
) -> Option<String> {
    let (_, nodes_end) = top_level_block(text, "nodes")?;
    let mut out = format!("{}{rows}{}", &text[..nodes_end], &text[nodes_end..]);
    let entry_ref = format!("{ros}/{}", wiring.local);
    let key = format!("    {}: [", wiring.side);
    let new_entry = format!(
        "\x20 {}:\n\x20   type: {}\n{key}{entry_ref}]\n",
        wiring.name, wiring.type_
    );
    let Some((start, end)) = top_level_block(&out, wiring.section) else {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("\n{}:\n{new_entry}", wiring.section));
        return Some(out);
    };
    let block = &out[start..end];
    let head = format!("  {}:\n", wiring.name);
    let entry_at = if block.starts_with(&head) {
        Some(0)
    } else {
        block.find(&format!("\n{head}")).map(|i| i + 1)
    };
    let Some(entry_at) = entry_at else {
        out.insert_str(end, &new_entry);
        return Some(out);
    };
    // The entry's lines: indented 4 or more, after its head line.
    let body_at = start + entry_at + head.len();
    let mut entry_end = end;
    let mut pos = body_at;
    for line in out[body_at..end].split_inclusive('\n') {
        if !line.starts_with("    ") && !line.trim().is_empty() {
            entry_end = pos;
            break;
        }
        pos += line.len();
    }
    let entry = &out[body_at..entry_end];
    if let Some(at) = entry.find(&key) {
        let close = body_at + at + entry[at..].find(']')?;
        out.insert_str(close, &format!(", {entry_ref}"));
    } else {
        // After the entry's last list line, else after its `type:` line.
        let anchor = ["    pub: [", "    server: [", "    type:"]
            .iter()
            .find_map(|k| entry.find(k))?;
        let line_end = body_at + anchor + entry[anchor..].find('\n')? + 1;
        out.insert_str(line_end, &format!("{key}{entry_ref}]\n"));
    }
    Some(out)
}

/// The bringup's `package.xml`: it builds nothing, and runs its node.
pub(crate) fn bringup_package_xml(bringup: &str, node: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{bringup}</name>
  <version>0.1.0</version>
  <description>What {node} runs on: the board, the RMW and the image, in system.toml.</description>
  <maintainer email="you@example.com">Your Name</maintainer>
  <license>Apache-2.0</license>
  <exec_depend>{node}</exec_depend>
  <export>
    <build_type>nros_cmake</build_type>
  </export>
</package>
"#
    )
}

/// `nros new <node> [--to <bringup>]` inside the workspace at `ws`: the node
/// package under `src/`, and with `to`, its `[[component]]` row and an
/// `<exec_depend>` in that bringup. Returns the bringup edited, if any.
pub fn scaffold_into_workspace(
    ws: &Path,
    node: &NodePackage<'_>,
    to: Option<&str>,
) -> Result<Option<PathBuf>> {
    let bringup = match to {
        Some(b) => Some(find_bringup(ws, b)?),
        None => None,
    };
    // Issue 2376 — the bringup's images must take a C/C++ entry before a C or
    // C++ node joins them; checked before anything is written.
    if let Some(dir) = &bringup {
        for board in image_boards(dir)? {
            crate::cmd::new::refuse_c_family_without_entry(node.lang, &board)?;
        }
    }
    let node_dir = ws.join("src").join(node.name);
    if node_dir.exists() && !node.force {
        bail!(
            "nros new: {} already exists (use --force to overwrite it)",
            node_dir.display()
        );
    }
    node.write(&node_dir)?;
    if let Some(dir) = &bringup {
        add_component(dir, node)?;
        add_exec_depend(dir, node.name)?;
        add_to_contract(dir, node)?;
    }
    Ok(bringup)
}

/// The `board` of every `[image.<id>]` in a bringup's `system.toml`.
fn image_boards(bringup: &Path) -> Result<Vec<String>> {
    let path = bringup.join("system.toml");
    let text =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("reading {}", path.display()))?;
    let doc: toml::Table = text
        .parse()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    Ok(doc
        .get("image")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|t| t.values())
        .filter_map(|img| img.get("board")?.as_str().map(str::to_string))
        .collect())
}

/// `--to <bringup>`: a path to the bringup, or its package name under `src/`.
fn find_bringup(ws: &Path, b: &str) -> Result<PathBuf> {
    let p = PathBuf::from(b);
    let candidates = [p.clone(), ws.join(&p), ws.join("src").join(&p)];
    if let Some(dir) = candidates.iter().find(|d| d.join("system.toml").is_file()) {
        return Ok(dir.clone());
    }
    let mut have: Vec<String> = std::fs::read_dir(ws.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("system.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    have.sort();
    bail!(
        "nros new --to {b}: no bringup `{b}` in {} (a bringup is a package holding a \
         system.toml). Bringups here: {}",
        ws.display(),
        if have.is_empty() {
            "(none)".to_string()
        } else {
            have.join(", ")
        }
    )
}

/// Append the node's `[[component]]` row to the bringup's `system.toml`,
/// keeping every other line. A row for the same package is refused.
fn add_component(bringup: &Path, node: &NodePackage<'_>) -> Result<()> {
    let path = bringup.join("system.toml");
    let text =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("reading {}", path.display()))?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    let rows = doc
        .entry("component")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    let Some(rows) = rows.as_array_of_tables_mut() else {
        bail!(
            "{}: `component` is not a `[[component]]` list",
            path.display()
        );
    };
    if rows
        .iter()
        .any(|r| r.get("pkg").and_then(|v| v.as_str()) == Some(node.name))
    {
        bail!(
            "{}: a `[[component]]` row for `{}` already exists",
            path.display(),
            node.name
        );
    }
    let mut row = toml_edit::Table::new();
    row.insert("pkg", toml_edit::value(node.name));
    row.insert("class", toml_edit::value(node.class()));
    row.insert("name", toml_edit::value(node.ros_name()));
    rows.push(row);
    std::fs::write(&path, doc.to_string())
        .wrap_err_with(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Add `<exec_depend>{node}</exec_depend>` to the bringup's `package.xml`,
/// after its last dependency row (or before `<export>`), unless present.
fn add_exec_depend(bringup: &Path, node: &str) -> Result<()> {
    let path = bringup.join("package.xml");
    let Ok(xml) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let row = format!("<exec_depend>{node}</exec_depend>");
    if xml.contains(&row) {
        return Ok(());
    }
    let anchor = ["</exec_depend>", "</depend>", "</build_depend>"]
        .iter()
        .filter_map(|t| xml.rfind(t).map(|i| i + t.len()))
        .max();
    let out = match anchor {
        Some(at) => format!("{}\n  {row}{}", &xml[..at], &xml[at..]),
        None => match xml.find("<export>").or_else(|| xml.find("</package>")) {
            Some(at) => format!("{}{row}\n  {}", &xml[..at], &xml[at..]),
            None => bail!("{}: no `</package>` to add `{row}` before", path.display()),
        },
    };
    std::fs::write(&path, out).wrap_err_with(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "nros-new-ws-{tag}-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn talker(name: &str) -> NodePackage<'_> {
        NodePackage {
            name,
            template: Role::Talker,
            node_name: None,
            meta: PackageMeta::default(),
            lang: nros_lang::Language::Rust,
            force: false,
        }
    }

    /// Issue 2376 -- every role added with `--to` joins the bringup's
    /// contract with `params:` and its whole surface, per language, or the
    /// CMake road refuses the image's parameter store (the Tutorial's
    /// services chapter added four nodes and stopped at step 630).
    #[test]
    fn every_role_added_with_to_joins_the_contract_whole() {
        for lang in [nros_lang::Language::Rust, nros_lang::Language::C] {
            let ws = tmp(&format!("roles-{lang:?}"));
            let mut sensor = talker("sensor_node");
            sensor.lang = lang;
            scaffold_project(&WorkspaceProject {
                dir: ws.clone(),
                node: sensor,
                board: "linux",
                image_id: "linux",
                rmw: "zenoh",
                ros_edition: "humble",
                locator: None,
                main_stack: None,
            })
            .unwrap();
            for (name, role) in [
                ("monitor_node", Role::Listener),
                ("server_node", Role::Server),
                ("client_node", Role::Client),
                ("action_server_node", Role::ActionServer),
                ("action_client_node", Role::ActionClient),
            ] {
                let mut n = talker(name);
                n.template = role;
                n.lang = lang;
                scaffold_into_workspace(&ws, &n, Some("sensor_node_bringup")).unwrap();
            }
            let text =
                std::fs::read_to_string(ws.join("src/sensor_node_bringup/system.contract.yaml"))
                    .unwrap();
            let c: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
            let nodes = c["nodes"].as_mapping().unwrap();
            assert_eq!(nodes.len(), 6, "{text}");
            for (_, n) in nodes {
                assert!(n["params"].is_mapping(), "{text}");
            }
            let pair = |section: &str, name: &str, a: &str, b: &str| {
                let e = &c[section][name];
                (
                    e[a].as_sequence().map(Vec::len),
                    e[b].as_sequence().map(Vec::len),
                )
            };
            assert_eq!(pair("topics", "/chatter", "pub", "sub"), (Some(1), Some(1)));
            assert_eq!(
                c["services"]["/add_two_ints"]["server"][0].as_str(),
                Some("server_node/add_two_ints"),
                "{text}"
            );
            assert_eq!(
                c["services"]["/add_two_ints"]["client"][0].as_str(),
                Some("client_node/add_two_ints"),
                "{text}"
            );
            assert_eq!(
                c["actions"]["/fibonacci"]["type"].as_str(),
                Some("example_interfaces/action/Fibonacci")
            );
            assert_eq!(
                pair("actions", "/fibonacci", "server", "client"),
                (Some(1), Some(1)),
                "{text}"
            );
            assert!(c["nodes"]["server_node"]["srv"]["add_two_ints"].is_mapping());
            assert!(c["nodes"]["client_node"]["cli"]["add_two_ints"].is_mapping());
            // The timers the templates create, per language.
            let rust = lang == nros_lang::Language::Rust;
            let client_paths = c["nodes"]["client_node"]["paths"].as_mapping().unwrap();
            let (_, p) = client_paths.iter().next().unwrap();
            assert_eq!(
                p["trigger"]["timer"]["rate_hz"].as_u64(),
                Some(if rust { 1 } else { 2 }),
                "{text}"
            );
            for a in ["action_server_node", "action_client_node"] {
                assert_eq!(c["nodes"][a]["paths"].is_mapping(), !rust, "{a}: {text}");
            }
            assert!(
                text.starts_with("# What this image's node DECLARES"),
                "{text}"
            );
            assert!(!text.contains("\n\n\n"), "{text}");
            std::fs::remove_dir_all(&ws).unwrap();
        }
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    /// Issue 2400 W4: the recorded floor is `floor_formula`'s, on the
    /// committed rows (the build reproduced each of these).
    #[test]
    fn a_rows_recorded_floor_is_the_check_formula() {
        let t = std::fs::read_to_string(repo_root().join("config/stack-floors.toml")).unwrap();
        let k = |s: &str| format!("{s}@nucleo-f767zi-zephyr");
        assert_eq!(
            recorded_floor(&t, &k("action-server/rust/zenoh")),
            Some(33288)
        );
        assert_eq!(recorded_floor(&t, &k("talker/rust/zenoh")), Some(31784));
        assert_eq!(recorded_floor(&t, &k("talker/c/zenoh")), Some(12872));
        assert_eq!(recorded_floor(&t, &k("listener/rust/zenoh")), None);
    }

    /// The default is READ from the module's Kconfig, per language.
    #[test]
    fn the_zephyr_main_stack_default_is_the_modules_kconfig() {
        let r = repo_root();
        let b = "nucleo-f767zi-zephyr";
        assert_eq!(zephyr_main_stack_default(&r, b, true, "zenoh"), Some(32768));
        assert_eq!(
            zephyr_main_stack_default(&r, b, false, "zenoh"),
            Some(16384)
        );
        // a board that states its own wins
        assert_eq!(
            zephyr_main_stack_default(&r, "mps2-an385-zephyr", true, "zenoh"),
            Some(131072)
        );
    }

    /// Ruling (A)(3), for EVERY row: the image `nros new` makes from a measured
    /// template gets a main stack at or above its floor, either the board's
    /// default or the `os_config` the scaffold writes, and never below the
    /// default. A row added with a floor above its default is covered with no
    /// edit here.
    #[test]
    fn every_measured_template_gets_a_main_stack_at_its_floor() {
        let r = repo_root();
        let t = std::fs::read_to_string(r.join("config/stack-floors.toml")).unwrap();
        let doc: toml::Value = toml::from_str(&t).unwrap();
        let rows = doc["allowance"].as_array().unwrap();
        assert!(!rows.is_empty());
        let mut raised = 0;
        for row in rows {
            let key = row["image"].as_str().unwrap();
            let Some((tmpl, board)) = key.split_once('@') else {
                continue; // not a template key
            };
            let mut it = tmpl.split('/');
            let (_, lang, rmw) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
            let rust = lang == "rust";
            let floor = recorded_floor(&t, key).expect(key);
            let default = zephyr_main_stack_default(&r, board, rust, rmw).expect(key);
            let got = template_main_stack(&r, key, true, rust, rmw, board);
            let stack = got.unwrap_or(default);
            assert!(
                stack >= floor,
                "{key}: {stack} B is below its floor {floor} B"
            );
            assert!(
                stack >= default,
                "{key}: {stack} B lowers the default {default} B"
            );
            raised += usize::from(got.is_some());
        }
        assert_eq!(
            template_main_stack(
                &r,
                "action-server/rust/zenoh@nucleo-f767zi-zephyr",
                true,
                true,
                "zenoh",
                "nucleo-f767zi-zephyr"
            ),
            Some(33288)
        );
        assert!(raised >= 1);
        // not a Zephyr board: the knob is not CONFIG_MAIN_STACK_SIZE
        assert_eq!(
            template_main_stack(
                &r,
                "action-server/rust/zenoh@nucleo-f767zi-zephyr",
                false,
                true,
                "zenoh",
                "nucleo-f767zi-zephyr"
            ),
            None
        );
    }

    #[test]
    fn a_raised_template_states_its_stack_through_os_config() {
        let ws = tmp("stackraise");
        let bringup = scaffold_project(&WorkspaceProject {
            dir: ws.clone(),
            node: talker("sensor_node"),
            board: "nucleo-f767zi-zephyr",
            image_id: "zephyr",
            rmw: "zenoh",
            ros_edition: "humble",
            locator: None,
            main_stack: Some(33288),
        })
        .unwrap();
        let sys = std::fs::read_to_string(bringup.join("system.toml")).unwrap();
        assert!(sys.contains("os_config = [\"stack.conf\"]"), "{sys}");
        let conf = std::fs::read_to_string(bringup.join("stack.conf")).unwrap();
        assert!(
            conf.lines().any(|l| l == "CONFIG_MAIN_STACK_SIZE=33288"),
            "{conf}"
        );
        // both readers still parse it
        let parsed: crate::orchestration::cargo_metadata_schema::SystemToml =
            toml::from_str(&sys).unwrap();
        assert_eq!(
            parsed.image["zephyr"].os_config,
            vec!["stack.conf".to_string()]
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn a_workspace_name_gives_its_node_and_bringup() {
        assert_eq!(node_package_name("sensor_ws"), "sensor_node");
        assert_eq!(node_package_name("host_ws"), "host_node");
        assert_eq!(node_package_name("demo"), "demo_node");
        assert_eq!(node_package_name("my-robot_ws"), "my_robot_node");
        assert_eq!(node_package_name("cam_node"), "cam_node");
        assert_eq!(bringup_name("sensor_node"), "sensor_node_bringup");
    }

    /// RFC-0121 D4: the workspace holds a library node package and a bringup
    /// of `package.xml` + `system.toml`, with no `launch/` and no root build
    /// file; the node names no board and is no cargo root of its own.
    #[test]
    fn only_a_role_whose_whole_surface_is_known_gets_a_contract() {
        // A contract that names a node is its entity declaration, so a partial
        // one under-sizes the callback table; every role but the talker
        // writes none and keeps the board's configured pools.
        for role in Role::ALL {
            let mut node = talker("x_node");
            node.template = role;
            let contract = bringup_contract(&node);
            assert_eq!(contract.is_some(), role == Role::Talker, "{}", role.name());
        }
    }

    #[test]
    fn the_project_scaffold_writes_a_node_package_and_a_bringup() {
        let ws = tmp("project");
        let bringup = scaffold_project(&WorkspaceProject {
            dir: ws.clone(),
            node: talker("sensor_node"),
            board: "mps2-an385-baremetal",
            image_id: "baremetal",
            rmw: "zenoh",
            ros_edition: "humble",
            locator: Some("serial/UART_0#baudrate=115200".into()),
            main_stack: None,
        })
        .unwrap();
        assert_eq!(bringup, ws.join("src/sensor_node_bringup"));
        let node = ws.join("src/sensor_node");
        assert!(node.join("src/lib.rs").is_file());
        assert!(
            !node.join("src/main.rs").exists(),
            "a node package is a library"
        );
        let manifest = std::fs::read_to_string(node.join("Cargo.toml")).unwrap();
        assert!(!manifest.contains("[workspace]"), "{manifest}");
        assert!(!manifest.contains("nros_selection"), "{manifest}");
        assert!(!manifest.contains("mps2"), "{manifest}");
        assert!(!node.join("system.toml").exists());
        assert!(!node.join("nros.toml").exists());

        let mut entries: Vec<String> = std::fs::read_dir(&bringup)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            ["package.xml", "system.contract.yaml", "system.toml"]
        );
        // issue 2400 -- the image names the template its stack allowance is
        // measured under, by the one spelling, and both readers accept it.
        let key = stack_floor_template_key(
            Role::Talker,
            nros_lang::Language::Rust,
            "zenoh",
            "mps2-an385-baremetal",
        );
        assert_eq!(key, "talker/rust/zenoh@mps2-an385-baremetal");
        let leaf = nros_orchestration_ir::leaf_system::read_image(&bringup, "baremetal")
            .unwrap()
            .unwrap();
        assert_eq!(leaf.stack_floor_image.as_deref(), Some(key.as_str()));
        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(bringup.join("system.toml")).unwrap()).unwrap();
        let block: crate::orchestration::image::ImageBlock =
            doc["image"]["baremetal"].clone().try_into().unwrap();
        assert_eq!(block.stack_floor_image.as_deref(), Some(key.as_str()));

        // issue 2270 -- the contract states what the talker's code does, so
        // the first build sizes from it rather than from defaults.
        let contract: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            &std::fs::read_to_string(bringup.join("system.contract.yaml")).unwrap(),
        )
        .unwrap();
        let node_c = &contract["nodes"]["sensor_node"];
        assert!(node_c["params"].as_mapping().is_some_and(|m| m.is_empty()));
        assert_eq!(
            node_c["pub"]["chatter"]["qos"]["durability"].as_str(),
            Some("volatile")
        );
        // A contract is the node's ENTITY declaration: the talker's 1 Hz
        // timer must be in it, or a contract-only derivation (any host where
        // the probe sidecar is not fresh) counts zero callbacks and the image
        // does not compile (phase-523 round two, on nros-0.6.0-nros7).
        assert_eq!(
            node_c["paths"]["on_tick"]["trigger"]["timer"]["rate_hz"].as_u64(),
            Some(1)
        );
        assert_eq!(
            node_c["paths"]["on_tick"]["output"][0].as_str(),
            Some("chatter")
        );
        assert_eq!(
            contract["topics"]["/chatter"]["pub"][0].as_str(),
            Some("sensor_node/chatter")
        );
        // issue 2332 -- the reader is outside the workspace, so the resolver's
        // `[dangling-entity]` warning does not fire on a clean scaffold.
        assert_eq!(
            contract["topics"]["/chatter"]["external"].as_str(),
            Some("sub")
        );
        let sys: toml::Table =
            toml::from_str(&std::fs::read_to_string(bringup.join("system.toml")).unwrap()).unwrap();
        assert_eq!(sys["system"]["rmw"].as_str(), Some("zenoh"));
        let rows = sys["component"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["pkg"].as_str(), Some("sensor_node"));
        assert_eq!(rows[0]["class"].as_str(), Some("sensor_node::Talker"));
        assert_eq!(rows[0]["name"].as_str(), Some("sensor_node"));
        let img = &sys["image"]["baremetal"];
        assert_eq!(img["board"].as_str(), Some("mps2-an385-baremetal"));
        assert_eq!(
            img["locator"].as_str(),
            Some("serial/UART_0#baudrate=115200")
        );
        let pxml = std::fs::read_to_string(bringup.join("package.xml")).unwrap();
        assert!(
            pxml.contains("<exec_depend>sensor_node</exec_depend>"),
            "{pxml}"
        );
        assert!(!ws.join("Cargo.toml").exists() && !ws.join("CMakeLists.txt").exists());
        // The marker `nros::main!` walks up to from a generated entry, which
        // is its own `[workspace]` root: without it a west-built entry found
        // no packages ("pkg `…_bringup` not found in workspace `…/zephyr_entry`").
        assert!(ws.join(WORKSPACE_MARKER).is_file());
        let entry = ws.join("build/zephyr-zenoh/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(entry.join("Cargo.toml"), "[workspace]\n").unwrap();
        // (`$NROS_WORKSPACE_ROOT` outranks every marker, so a caller that set
        // it is asking a different question.)
        if std::env::var_os("NROS_WORKSPACE_ROOT").is_none() {
            assert_eq!(
                nros_pkg_index::detect_workspace_root(&entry).unwrap(),
                ws.canonicalize().unwrap()
            );
        }
        assert_eq!(enclosing_workspace(&ws), Some(ws.clone()));
        assert_eq!(
            enclosing_workspace(&ws.join("src/sensor_node")),
            Some(ws.clone())
        );
        std::fs::remove_dir_all(&ws).unwrap();
    }

    /// A C or C++ node package names no `DEPLOY`: `nros build` reads that
    /// token as the board a package is for, and the `DEPLOY linux` the
    /// standalone scaffold writes left the node out of every non-Linux cmake
    /// root ("no cmake packages in this workspace", the NuttX and FreeRTOS C
    /// chapters).
    #[test]
    fn a_c_or_cpp_node_package_names_no_deploy_target() {
        for lang in [nros_lang::Language::C, nros_lang::Language::Cpp] {
            let ws = tmp(&format!("deploy-{lang:?}"));
            let mut node = talker("nx_node");
            node.lang = lang;
            scaffold_project(&WorkspaceProject {
                dir: ws.clone(),
                node,
                board: "qemu-armv7a-nuttx",
                image_id: "nuttx",
                rmw: "zenoh",
                ros_edition: "humble",
                locator: None,
                main_stack: None,
            })
            .unwrap();
            let cmake = std::fs::read_to_string(ws.join("src/nx_node/CMakeLists.txt")).unwrap();
            assert!(cmake.contains("nano_ros_add_node("), "{lang:?}: {cmake}");
            assert!(
                !cmake.lines().any(|l| l.trim_start().starts_with("DEPLOY")),
                "{lang:?}: {cmake}"
            );
            std::fs::remove_dir_all(&ws).unwrap();
        }
    }

    /// `nros new <node> --to <bringup>` adds the package, its component row
    /// and its `<exec_depend>`; without `--to` no bringup changes.
    #[test]
    fn a_node_added_to_a_workspace_touches_a_bringup_only_with_to() {
        let ws = tmp("into");
        scaffold_project(&WorkspaceProject {
            dir: ws.clone(),
            node: talker("sensor_node"),
            board: "linux",
            image_id: "linux",
            rmw: "zenoh",
            ros_edition: "humble",
            locator: None,
            main_stack: None,
        })
        .unwrap();
        let sys_path = ws.join("src/sensor_node_bringup/system.toml");
        let before = std::fs::read_to_string(&sys_path).unwrap();
        assert!(!before.contains("locator"), "{before}");

        let mut lone = talker("lone_node");
        lone.template = Role::Listener;
        assert_eq!(scaffold_into_workspace(&ws, &lone, None).unwrap(), None);
        assert!(ws.join("src/lone_node/src/lib.rs").is_file());
        assert_eq!(std::fs::read_to_string(&sys_path).unwrap(), before);

        let mut monitor = talker("monitor_node");
        monitor.template = Role::Listener;
        let edited = scaffold_into_workspace(&ws, &monitor, Some("sensor_node_bringup"))
            .unwrap()
            .unwrap();
        assert_eq!(edited, ws.join("src/sensor_node_bringup"));
        let after = std::fs::read_to_string(&sys_path).unwrap();
        assert!(after.starts_with(&before[..before.find("[[component]]").unwrap()]));
        let sys: toml::Table = toml::from_str(&after).unwrap();
        let rows = sys["component"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["pkg"].as_str(), Some("monitor_node"));
        assert_eq!(rows[1]["class"].as_str(), Some("monitor_node::Listener"));
        let pxml = std::fs::read_to_string(ws.join("src/sensor_node_bringup/package.xml")).unwrap();
        assert!(
            pxml.contains("<exec_depend>monitor_node</exec_depend>"),
            "{pxml}"
        );
        assert_eq!(pxml.matches("<exec_depend>").count(), 2, "{pxml}");

        // Issue 2376 -- the contract names every node, or the CMake road
        // refuses to size the parameter store: the listener's rows join it.
        let contract_path = ws.join("src/sensor_node_bringup/system.contract.yaml");
        let text = std::fs::read_to_string(&contract_path).unwrap();
        let contract: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        let m = &contract["nodes"]["monitor_node"];
        assert!(m["params"].is_mapping(), "{text}");
        assert_eq!(
            m["sub"]["chatter"]["qos"]["durability"].as_str(),
            Some("volatile"),
            "{text}"
        );
        assert!(contract["nodes"]["sensor_node"]["params"].is_mapping());
        let t = &contract["topics"]["/chatter"];
        assert_eq!(t["pub"][0].as_str(), Some("sensor_node/chatter"), "{text}");
        assert_eq!(t["sub"][0].as_str(), Some("monitor_node/chatter"), "{text}");
        assert_eq!(t["external"].as_str(), Some("sub"), "{text}");
        assert!(
            text.find("    pub: [").unwrap() < text.find("    sub: [").unwrap(),
            "{text}"
        );
        assert!(text.contains("# What this image's node DECLARES"), "{text}");
        assert!(!text.contains("\n\n\n"), "{text}");

        // A second row for the same package, and an unknown bringup, refuse.
        let mut again = talker("monitor_node");
        again.force = true;
        assert!(scaffold_into_workspace(&ws, &again, Some("sensor_node_bringup")).is_err());
        let e = scaffold_into_workspace(&ws, &talker("x_node"), Some("nope"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("sensor_node_bringup"), "{e}");
        std::fs::remove_dir_all(&ws).unwrap();
    }
}

#[cfg(test)]
mod c_family_into_workspace_tests {
    use super::*;

    /// Issue 2376 — `--to` a bringup whose image is bare metal refuses a C node
    /// before writing its package.
    #[test]
    fn a_c_node_into_a_bare_metal_bringup_is_refused_before_anything_is_written() {
        let ws = tempfile::tempdir().unwrap();
        let bringup = ws.path().join("src/demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            bringup.join("system.toml"),
            "[system]\nname = \"demo\"\n\n[image.baremetal]\nboard = \"mps2-an385-baremetal\"\n",
        )
        .unwrap();
        assert_eq!(
            image_boards(&bringup).unwrap(),
            vec!["mps2-an385-baremetal"]
        );
        let e = crate::cmd::new::refuse_c_family_without_entry(
            nros_lang::Language::C,
            "mps2-an385-baremetal",
        )
        .unwrap_err();
        assert!(e.to_string().contains("mps2-an385-freertos"));
        assert!(!ws.path().join("src/listener").exists());
    }
}

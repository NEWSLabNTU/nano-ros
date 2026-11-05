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
        .find(|d| d.join("nros.toml").is_file() || holds_packages(&d.join("src")))
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
    format!("{node}_bringup")
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
}

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
    std::fs::write(
        p.dir.join(".gitignore"),
        "/build/\n/generated/\n/target/\n/.cargo/\nCargo.lock\n/src/*/metadata/\n",
    )?;
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
         \n[image.{id}]\nboard = \"{board}\"\n{locator}",
        name = p.node.name,
        rmw = p.rmw,
        ed = p.ros_edition,
        pkg = p.node.name,
        class = p.node.class(),
        ros = p.node.ros_name(),
        id = p.image_id,
        board = p.board,
    )
}

/// The bringup's `package.xml`: it builds nothing, and runs its node.
fn bringup_package_xml(bringup: &str, node: &str) -> String {
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
    }
    Ok(bringup)
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
        assert_eq!(entries, ["package.xml", "system.toml"]);
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

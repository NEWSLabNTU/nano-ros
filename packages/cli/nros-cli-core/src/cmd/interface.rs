//! `nros interface generate|list|show` (RFC-0117 D2/D4).
//!
//! `generate` is the old `nros generate`, lowered to it (`cmd::user`). `list`
//! and `show` take `ros2 interface`'s names and meaning: `list` prints every
//! interface type the search path reaches, grouped as messages, services and
//! actions; `show` prints one type's definition with each nested message type
//! expanded beneath its field, indented by a tab.
//!
//! The search path is the one codegen resolves through
//! (`cargo_nano_ros::load_index_with_fallback`): `NROS_INTERFACE_SEARCH_PATH`,
//! then the ament environment, then the bundled interfaces. The current
//! workspace's own `src/` is searched first, so a type the user is editing
//! shadows an installed one of the same name, as it does in a build.

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, bail, eyre};
use rosidl_bindgen::ament::AmentIndex;

use serde::Serialize;

use crate::cmd::{frame, generate};

/// The `--json` schema names (RFC-0133 D1, phase-569 W5c). Adding a key keeps
/// the version; removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.interface.list.v1";
pub const SHOW_SCHEMA: &str = "nros.interface.show.v1";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// Generate Rust / C / C++ message bindings from `package.xml`.
    Generate(generate::Args),
    /// List the interface types the search path reaches.
    List(ListArgs),
    /// Show an interface type's definition, nested types expanded.
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Print only messages.
    #[arg(short = 'm', long = "only-msgs", conflicts_with_all = ["only_srvs", "only_actions"])]
    pub only_msgs: bool,
    /// Print only services.
    #[arg(short = 's', long = "only-srvs", conflicts_with = "only_actions")]
    pub only_srvs: bool,
    /// Print only actions.
    #[arg(short = 'a', long = "only-actions")]
    pub only_actions: bool,
    /// Workspace whose `src/` is searched first. Defaults to the current
    /// directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print one JSON object instead of the list
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The type, as `<package>/<msg|srv|action>/<Name>`.
    #[arg(value_name = "TYPE")]
    pub type_name: String,
    /// Workspace whose `src/` is searched first. Defaults to the current
    /// directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print one JSON object instead of the definition
    #[arg(long)]
    pub json: bool,
}

/// One interface type, as `list --json` reports it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InterfaceItem {
    /// `<package>/<msg|srv|action>/<Name>`.
    pub name: String,
    pub package: String,
    /// `msg`, `srv` or `action`.
    pub kind: &'static str,
}

/// One interface type, as `show --json` reports it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InterfaceShow {
    pub name: String,
    pub package: String,
    pub kind: String,
    /// The definition file the search path resolved it to.
    pub path: PathBuf,
    /// The file as written.
    pub definition: String,
    /// The definition with each nested message type expanded beneath its
    /// field, indented by a tab: what `show` prints.
    pub expanded: String,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::Generate(a) => generate::run(a),
        Args::List(a) => {
            let index = load_index(a.workspace.as_deref())?;
            if a.json {
                return frame::print_json(LIST_SCHEMA, "items", &list_items(&index, &a), &[]);
            }
            print!("{}", render_list(&index, &a));
            Ok(())
        }
        Args::Show(a) => {
            let index = load_index(a.workspace.as_deref())?;
            if a.json {
                return frame::print_json(
                    SHOW_SCHEMA,
                    "item",
                    &show_item(&index, &a.type_name)?,
                    &[],
                );
            }
            print!("{}", render_show(&index, &a.type_name)?);
            Ok(())
        }
    }
}

fn load_index(workspace: Option<&Path>) -> Result<AmentIndex> {
    let ws = match workspace {
        Some(w) => w.to_path_buf(),
        None => std::env::current_dir().wrap_err("resolving cwd")?,
    };
    let mut index = AmentIndex::from_path_string("")?;
    let src = ws.join("src");
    for dir in [src.as_path(), ws.as_path()] {
        if dir.is_dir()
            && let Ok(local) = AmentIndex::from_directory(dir)
        {
            index.merge(local);
        }
    }
    index.merge(cargo_nano_ros::load_index_with_fallback(false)?);
    Ok(index)
}

/// The interface types `args` selects, sorted within each kind, messages
/// first, then services, then actions.
pub fn list_items(index: &AmentIndex, args: &ListArgs) -> Vec<InterfaceItem> {
    let all = !(args.only_msgs || args.only_srvs || args.only_actions);
    let kinds: [(&'static str, bool); 3] = [
        ("msg", all || args.only_msgs),
        ("srv", all || args.only_srvs),
        ("action", all || args.only_actions),
    ];
    let mut out = Vec::new();
    for (kind, wanted) in kinds {
        if !wanted {
            continue;
        }
        let mut items: Vec<InterfaceItem> = index
            .packages()
            .values()
            .flat_map(|p| {
                let list = match kind {
                    "msg" => &p.interfaces.messages,
                    "srv" => &p.interfaces.services,
                    _ => &p.interfaces.actions,
                };
                list.iter().map(move |n| InterfaceItem {
                    name: format!("{}/{kind}/{n}", p.name),
                    package: p.name.clone(),
                    kind,
                })
            })
            .collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(items);
    }
    out
}

/// `ros2 interface list`'s layout: a heading per kind, one indented
/// `<package>/<kind>/<Name>` per line, sorted.
pub fn render_list(index: &AmentIndex, args: &ListArgs) -> String {
    let all = !(args.only_msgs || args.only_srvs || args.only_actions);
    let items = list_items(index, args);
    let mut out = String::new();
    for (heading, kind, wanted) in [
        ("Messages", "msg", all || args.only_msgs),
        ("Services", "srv", all || args.only_srvs),
        ("Actions", "action", all || args.only_actions),
    ] {
        if !wanted {
            continue;
        }
        out.push_str(heading);
        out.push_str(":\n");
        for i in items.iter().filter(|i| i.kind == kind) {
            out.push_str("    ");
            out.push_str(&i.name);
            out.push('\n');
        }
    }
    out
}

const PRIMITIVES: &[&str] = &[
    "bool", "byte", "char", "float32", "float64", "int8", "uint8", "int16", "uint16", "int32",
    "uint32", "int64", "uint64", "string", "wstring",
];

fn interface_path(index: &AmentIndex, pkg: &str, kind: &str, name: &str) -> Option<PathBuf> {
    let p = index.find_package(pkg)?;
    let path = match kind {
        "msg" => p.get_message_path(name),
        "srv" => p.get_service_path(name),
        "action" => p.get_action_path(name),
        _ => return None,
    };
    path.is_file().then_some(path)
}

/// One type's definition, each field of a nested message type followed by
/// that type's definition indented one tab deeper, as `ros2 interface show`
/// prints it.
pub fn render_show(index: &AmentIndex, type_name: &str) -> Result<String> {
    Ok(show_item(index, type_name)?.expanded)
}

/// `show`'s item: the type, the file it resolved to, and its definition as
/// written and expanded.
pub fn show_item(index: &AmentIndex, type_name: &str) -> Result<InterfaceShow> {
    let parts: Vec<&str> = type_name.split('/').collect();
    let [pkg, kind, name] = parts[..] else {
        bail!("`{type_name}` is not an interface type; write `<package>/<msg|srv|action>/<Name>`");
    };
    let path = interface_path(index, pkg, kind, name).ok_or_else(|| {
        eyre!(
            "`{type_name}` was not found on the interface search path \
             (the workspace's src/, NROS_INTERFACE_SEARCH_PATH, the ROS 2 \
             environment and the bundled interfaces)"
        )
    })?;
    let definition =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    let mut expanded = String::new();
    expand(index, pkg, &path, 0, &mut expanded)?;
    Ok(InterfaceShow {
        name: type_name.to_string(),
        package: pkg.to_string(),
        kind: kind.to_string(),
        path,
        definition,
        expanded,
    })
}

fn expand(
    index: &AmentIndex,
    pkg: &str,
    path: &Path,
    depth: usize,
    out: &mut String,
) -> Result<()> {
    if depth > 32 {
        bail!("interface nesting deeper than 32 at {}", path.display());
    }
    let text =
        std::fs::read_to_string(path).wrap_err_with(|| format!("read {}", path.display()))?;
    let indent = "\t".repeat(depth);
    for line in text.lines() {
        out.push_str(&indent);
        out.push_str(line);
        out.push('\n');
        if let Some((nested_pkg, nested)) = nested_type(line, pkg)
            && let Some(p) = interface_path(index, &nested_pkg, "msg", &nested)
        {
            expand(index, &nested_pkg, &p, depth + 1, out)?;
        }
    }
    Ok(())
}

/// The `(package, Name)` of a field's message type, when the field's type is
/// not a primitive. A bare `Name` is in the enclosing package.
fn nested_type(line: &str, pkg: &str) -> Option<(String, String)> {
    let code = line.split('#').next()?.trim();
    if code.is_empty() || code == "---" {
        return None;
    }
    let mut words = code.split_whitespace();
    let ty = words.next()?;
    words.next()?; // a field has a name; a constant has `NAME=value`
    if code.contains('=') && !code.contains("<=") {
        return None;
    }
    let base = ty.split('[').next()?.split("<=").next()?;
    if PRIMITIVES.contains(&base) {
        return None;
    }
    let mut it = base.split('/');
    match (it.next(), it.next(), it.next()) {
        (Some(n), None, None) => Some((pkg.to_string(), n.to_string())),
        (Some(p), Some(n), None) => Some((p.to_string(), n.to_string())),
        (Some(p), Some("msg"), Some(n)) => Some((p.to_string(), n.to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with(files: &[(&str, &str)]) -> (tempfile::TempDir, AmentIndex) {
        let dir = tempfile::tempdir().unwrap();
        for (rel, body) in files {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let index = AmentIndex::from_directory(dir.path()).unwrap();
        (dir, index)
    }

    #[test]
    fn nested_type_reads_ros_field_syntax() {
        assert_eq!(nested_type("int32 data", "p"), None);
        assert_eq!(nested_type("string<=10 name", "p"), None);
        assert_eq!(nested_type("uint8[] raw", "p"), None);
        assert_eq!(nested_type("int32 X=3", "p"), None);
        assert_eq!(nested_type("# Header h", "p"), None);
        assert_eq!(
            nested_type("Point[<=4] pts  # c", "p"),
            Some(("p".into(), "Point".into()))
        );
        assert_eq!(
            nested_type("std_msgs/Header header", "p"),
            Some(("std_msgs".into(), "Header".into()))
        );
    }

    #[test]
    fn list_and_show_expand_nested_types() {
        let (_d, index) = index_with(&[
            ("a/msg/Inner.msg", "int32 x\n"),
            ("a/msg/Outer.msg", "# outer\nInner in\nb/Leaf leaf\n"),
            ("b/msg/Leaf.msg", "bool ok\n"),
            ("b/srv/Ask.srv", "Leaf q\n---\nbool r\n"),
        ]);
        let all = ListArgs {
            only_msgs: false,
            only_srvs: false,
            only_actions: false,
            workspace: None,
            json: false,
        };
        assert_eq!(
            render_list(&index, &all),
            "Messages:\n    a/msg/Inner\n    a/msg/Outer\n    b/msg/Leaf\n\
             Services:\n    b/srv/Ask\nActions:\n"
        );
        assert_eq!(
            render_show(&index, "a/msg/Outer").unwrap(),
            "# outer\nInner in\n\tint32 x\nb/Leaf leaf\n\tbool ok\n"
        );
        assert_eq!(
            render_show(&index, "b/srv/Ask").unwrap(),
            "Leaf q\n\tbool ok\n---\nbool r\n"
        );
        assert!(render_show(&index, "a/msg/Missing").is_err());
        assert!(render_show(&index, "Outer").is_err());
    }

    #[test]
    fn json_schemas_are_pinned() {
        // Renaming a key or the schema name is a new version (RFC-0133 D1).
        assert_eq!(LIST_SCHEMA, "nros.interface.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.interface.show.v1");
        let (_d, index) = index_with(&[
            ("a/msg/Inner.msg", "int32 x\n"),
            ("a/msg/Outer.msg", "Inner in\n"),
            ("a/srv/Ask.srv", "---\nbool r\n"),
        ]);
        let args = ListArgs {
            only_msgs: false,
            only_srvs: true,
            only_actions: false,
            workspace: None,
            json: true,
        };
        let doc = frame::json_doc(LIST_SCHEMA, "items", &list_items(&index, &args), &[]).unwrap();
        assert_eq!(
            doc,
            serde_json::json!({
                "schema": "nros.interface.list.v1",
                "items": [{"name": "a/srv/Ask", "package": "a", "kind": "srv"}],
            })
        );
        let show = serde_json::to_value(show_item(&index, "a/msg/Outer").unwrap()).unwrap();
        let mut keys: Vec<&str> = show
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["definition", "expanded", "kind", "name", "package", "path"]
        );
        assert_eq!(show["definition"], "Inner in\n");
        assert_eq!(show["expanded"], "Inner in\n\tint32 x\n");
    }
}

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

use crate::cmd::generate;

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
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::Generate(a) => generate::run(a),
        Args::List(a) => {
            let index = load_index(a.workspace.as_deref())?;
            print!("{}", render_list(&index, &a));
            Ok(())
        }
        Args::Show(a) => {
            let index = load_index(a.workspace.as_deref())?;
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

/// `ros2 interface list`'s layout: a heading per kind, one indented
/// `<package>/<kind>/<Name>` per line, sorted.
pub fn render_list(index: &AmentIndex, args: &ListArgs) -> String {
    let all = !(args.only_msgs || args.only_srvs || args.only_actions);
    let mut out = String::new();
    let kinds: [(&str, &str, bool); 3] = [
        ("Messages", "msg", all || args.only_msgs),
        ("Services", "srv", all || args.only_srvs),
        ("Actions", "action", all || args.only_actions),
    ];
    for (heading, kind, wanted) in kinds {
        if !wanted {
            continue;
        }
        let mut names: Vec<String> = index
            .packages()
            .values()
            .flat_map(|p| {
                let list = match kind {
                    "msg" => &p.interfaces.messages,
                    "srv" => &p.interfaces.services,
                    _ => &p.interfaces.actions,
                };
                list.iter().map(move |n| format!("{}/{kind}/{n}", p.name))
            })
            .collect();
        names.sort();
        out.push_str(heading);
        out.push_str(":\n");
        for n in names {
            out.push_str("    ");
            out.push_str(&n);
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
    let mut out = String::new();
    expand(index, pkg, &path, 0, &mut out)?;
    Ok(out)
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
}

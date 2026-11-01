//! The single-package front door (RFC-0116 D6, phase-525 W2).
//!
//! `nros new <name> --board <board> --template <role>` writes ONE package: a
//! `system.toml` with one `[image.<id>]` naming the board, one node from the
//! role template, the manifest beside it, and the README below. Nothing in it
//! is a workspace word, and the build settings for the board are rendered under
//! `build/` by `nros sync` / `nros build` (RFC-0098 D1), never written here.
//!
//! The README is the first chapter's text: the book includes this template,
//! so the package and the chapter cannot describe two different things. Its
//! build lines are [`crate::scaffold::next_steps`], the lines `nros new`
//! prints, so the README, the terminal and the chapter are one set of lines.
//!
//! [`grow_workspace`] is the other half of D6: when the reader wants a second
//! node, `nros new <ws> --workspace --from <package>` carries the node into a
//! workspace unchanged, with the package's `system.toml` as its bringup.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{Result, bail};
use nros_lang::Language;

use crate::node_role::Role;

/// The README template. `@NAME@`, `@TEMPLATE@`, `@SUMMARY@`, `@IMAGE@`,
/// `@BOARD@` and `@STEPS@` are replaced; nothing else is a token.
pub const README_TEMPLATE: &str = include_str!("../templates/single-package/README.md");

/// What [`render_readme`] fills in.
pub struct ReadmeFacts<'a> {
    pub name: &'a str,
    pub role: Role,
    pub image_id: &'a str,
    pub board: &'a str,
    pub lang: Language,
    pub platform: &'a str,
}

/// The scaffolded package's README: [`README_TEMPLATE`] with the package's own
/// facts, and the build lines `nros new` printed (less the `cd`, since the
/// README is read from inside the package).
#[must_use]
pub fn render_readme(f: &ReadmeFacts<'_>) -> String {
    let steps: Vec<String> = crate::scaffold::next_steps(f.name, f.lang, f.platform)
        .into_iter()
        .filter(|l| !l.starts_with("cd "))
        .collect();
    README_TEMPLATE
        .replace("@NAME@", f.name)
        .replace("@TEMPLATE@", f.role.name())
        .replace("@SUMMARY@", f.role.summary())
        .replace("@IMAGE@", f.image_id)
        .replace("@BOARD@", f.board)
        .replace("@STEPS@", &steps.join("\n"))
}

/// The one `[image.<id>]` a scaffolded package's `system.toml` declares, as
/// `(id, board)`. `None` when the file does not hold exactly one.
#[must_use]
pub fn sole_image(system_toml: &str) -> Option<(String, String)> {
    let doc: toml::Table = toml::from_str(system_toml).ok()?;
    let images = doc.get("image")?.as_table()?;
    let mut it = images.iter();
    let (id, block) = it.next()?;
    if it.next().is_some() {
        return None;
    }
    let board = block.get("board")?.as_str()?;
    Some((id.clone(), board.to_string()))
}

/// What a scaffolded single package holds that `nros sync` / `nros build`
/// produced, and which therefore does not move into a workspace: build output,
/// generated message crates and their source metadata, a cargo target dir, the
/// sync-written `.cargo/` include (gitignored, absolute paths into the old
/// location) and the build's SDK record.
const NOT_CARRIED: [&str; 7] = [
    "build",
    "generated",
    "target",
    ".cargo",
    "Cargo.lock",
    "metadata",
    "nros-sdk.lock",
];

/// What the package's entry half is, and so stays behind when the node moves:
/// in a workspace the entry is generated from the bringup, so the package's own
/// `main.rs`, its `system.toml` (which moves to the bringup) and its README
/// (which describes the single-package shape) are not node files.
const ENTRY_HALF: [&str; 3] = ["system.toml", "README.md", "src/main.rs"];

/// `nros new <ws> --workspace --from <package>`: a workspace at `ws` holding
/// the package's node at `src/<package>/` and its `system.toml`, unchanged, as
/// the bringup `src/<package>_bringup/`. Returns the bringup directory.
///
/// The node source and `package.xml` move file for file (RFC-0116 D6: the node
/// carries over unchanged). What changes is only what made the package its own
/// image: `main.rs` stays behind, and the manifest loses its `[[bin]]` table
/// and the generated selection dependency, because in a workspace the entry is
/// generated from the bringup. A second node is `nros new node <name>` inside
/// the workspace, which declares itself in the bringup. The original package
/// is left where it was.
///
/// A Rust package only: the C and C++ scaffolds are executables with their own
/// `main`, which is not a node a generated entry can boot.
pub fn grow_workspace(ws: &Path, from: &Path, force: bool) -> Result<PathBuf> {
    let system = from.join("system.toml");
    let Ok(text) = fs::read_to_string(&system) else {
        bail!(
            "nros new --workspace --from {}: there is no system.toml there. `--from` takes a \
             package made by `nros new <name> --board <board>`.",
            from.display()
        );
    };
    if !from.join("package.xml").is_file() {
        bail!(
            "nros new --workspace --from {}: there is no package.xml there, so it is not a \
             package.",
            from.display()
        );
    }
    let has_image = toml::from_str::<toml::Table>(&text)
        .ok()
        .and_then(|d| {
            d.get("image")
                .and_then(|i| i.as_table())
                .map(|t| !t.is_empty())
        })
        .unwrap_or(false);
    if !has_image {
        bail!(
            "nros new --workspace --from {}: its system.toml declares no [image.<id>], so there \
             is no board to carry into the workspace.",
            from.display()
        );
    }
    let Ok(manifest) = fs::read_to_string(from.join("Cargo.toml")) else {
        bail!(
            "nros new --workspace --from {}: there is no Cargo.toml there. Growing a C or C++ \
             package is not supported yet: its scaffold is an executable with its own `main`, \
             not a node a workspace's generated entry can boot. Start the workspace with \
             `nros new <ws> --workspace --lang <c|cpp>` and copy the node in.",
            from.display()
        );
    };
    let from_abs = from.canonicalize()?;
    let Some(pkg) = from_abs
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
    else {
        bail!(
            "nros new --workspace --from {}: invalid path",
            from.display()
        );
    };
    // A workspace inside the package would copy into itself.
    let ws_abs = if ws.is_absolute() {
        ws.to_path_buf()
    } else {
        std::env::current_dir()?.join(ws)
    };
    if ws_abs.starts_with(&from_abs) {
        bail!(
            "nros new --workspace --from {}: the workspace {} would sit inside the package; \
             make it beside the package instead (`cd ..` first).",
            from.display(),
            ws.display()
        );
    }
    if ws.exists() {
        if !force {
            bail!(
                "Directory '{}' already exists (use --force to overwrite)",
                ws.display()
            );
        }
        fs::remove_dir_all(ws)?;
    }

    let node = ws.join("src").join(&pkg);
    let bringup = ws.join("src").join(format!("{pkg}_bringup"));
    fs::create_dir_all(&node)?;
    fs::create_dir_all(&bringup)?;
    fs::write(
        ws.join(".colcon_workspace"),
        crate::workspace_scaffold::WORKSPACE_MARKER,
    )?;
    fs::write(ws.join(".gitignore"), "/target/\n/build/\n/Cargo.lock\n")?;

    copy_package(from, &node)?;
    fs::write(node.join("Cargo.toml"), node_manifest(&manifest, &pkg))?;

    fs::write(bringup.join("system.toml"), &text)?;
    fs::create_dir_all(bringup.join("launch"))?;
    fs::write(
        bringup.join("launch/system.launch.xml"),
        bringup_launch(&text, &pkg)?,
    )?;
    fs::write(bringup.join("package.xml"), bringup_package_xml(&pkg))?;
    fs::write(bringup.join(".gitignore"), "/generated/\n/build/\n")?;
    Ok(bringup)
}

/// The scaffolded manifest as a node package's: the same file less the
/// `[[bin]]` table, the generated selection dependency (`<pkg>_nros_selection`,
/// written by sync only for a package that is its own image) and the
/// `# nros-managed` message rows, which sync rewrites for the workspace's own
/// `generated/` on the next `nros sync`. Comments and every other line stay.
fn node_manifest(manifest: &str, pkg: &str) -> String {
    let selection = format!("{}_nros_selection", pkg.replace('-', "_"));
    let mut out = Vec::new();
    let mut in_bin = false;
    // Comments and blank lines after a `[[bin]]` table's keys belong to the
    // table that follows, so they are held and kept when it starts.
    let mut held: Vec<&str> = Vec::new();
    for line in manifest.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            in_bin = t.starts_with("[[bin]]");
            if !in_bin {
                out.append(&mut held);
            }
            held.clear();
        }
        if in_bin {
            if t.is_empty() || t.starts_with('#') {
                held.push(line);
            } else {
                held.clear();
            }
            continue;
        }
        if t.starts_with(&selection) || line.trim_end().ends_with("# nros-managed") {
            continue;
        }
        out.push(line);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// The bringup's launch file: one `<node>` per `[[component]]` row, the file
/// the single package's sync synthesised under `build/` (a workspace reads an
/// authored one, since a second node is an edit to it). `exec` is the node
/// name, the workspace templates' spelling for a library node.
fn bringup_launch(system_toml: &str, pkg: &str) -> Result<String> {
    let doc: toml::Table = toml::from_str(system_toml)?;
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
    };
    let mut xml = String::from(
        "<?xml version=\"1.0\"?>\n\
         <!-- What this workspace runs: one <node> per node. `nros new node <name>`\n     \
         adds one, beside the [[component]] row it writes in system.toml. -->\n<launch>\n",
    );
    let rows = doc
        .get("component")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    for row in &rows {
        let p = row.get("pkg").and_then(|v| v.as_str()).unwrap_or(pkg);
        let Some(name) = row.get("name").and_then(|v| v.as_str()) else {
            bail!("a `[[component]]` row in system.toml has no `name`");
        };
        xml.push_str(&format!(
            "  <node pkg=\"{}\" exec=\"{}\" name=\"{}\"/>\n",
            esc(p),
            esc(name),
            esc(name)
        ));
    }
    xml.push_str("</launch>\n");
    Ok(xml)
}

/// The bringup's `package.xml`: it builds nothing, and runs the node.
fn bringup_package_xml(pkg: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{pkg}_bringup</name>
  <version>0.1.0</version>
  <description>What {pkg} is deployed as: the images in system.toml.</description>
  <maintainer email="you@example.com">Your Name</maintainer>
  <license>Apache-2.0</license>
  <exec_depend>{pkg}</exec_depend>
  <export>
    <build_type>nros_cargo</build_type>
  </export>
</package>
"#
    )
}

/// Copy `from` into `to`, skipping [`NOT_CARRIED`] and [`ENTRY_HALF`].
fn copy_package(from: &Path, to: &Path) -> Result<()> {
    copy_filtered(from, to, Path::new(""))
}

fn copy_filtered(from: &Path, to: &Path, rel: &Path) -> Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let here = rel.join(&name);
        if rel.as_os_str().is_empty() && NOT_CARRIED.iter().any(|n| name == *n) {
            continue;
        }
        if ENTRY_HALF.iter().any(|e| Path::new(e) == here) {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.is_dir() {
            fs::create_dir_all(to.join(&name))?;
            copy_filtered(&entry.path(), &to.join(&name), &here)?;
        } else if meta.is_file() {
            fs::copy(entry.path(), to.join(&name))?;
        }
        // A symlink inside a scaffolded package is not something `nros new`
        // writes; it is skipped rather than followed out of the package.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readme_names_no_internal_id_and_leaves_no_token() {
        let r = render_readme(&ReadmeFacts {
            name: "sensor_node",
            role: Role::Talker,
            image_id: "mps2-an385-baremetal",
            board: "mps2-an385-baremetal",
            lang: Language::Rust,
            platform: "baremetal",
        });
        assert!(!r.contains('@'), "an unreplaced token:\n{r}");
        assert!(r.contains("board = \"mps2-an385-baremetal\""), "{r}");
        assert!(r.contains("nros build"), "{r}");
        assert!(!r.contains("cd sensor_node"), "{r}");
        for word in [
            "phase-",
            "Phase ",
            "issue ",
            "RFC-",
            "bringup",
            "launch.xml",
        ] {
            assert!(!r.contains(word), "`{word}` in the README:\n{r}");
        }
    }

    #[test]
    fn sole_image_reads_one_and_refuses_two() {
        assert_eq!(
            sole_image("[image.a]\nboard = \"linux\"\n"),
            Some(("a".into(), "linux".into()))
        );
        assert_eq!(
            sole_image("[image.a]\nboard = \"x\"\n[image.b]\nboard = \"y\"\n"),
            None
        );
    }

    #[test]
    fn growing_carries_the_node_unchanged_and_nothing_sync_wrote() {
        let t = tempfile::tempdir().unwrap();
        let pkg = t.path().join("sensor_node");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::create_dir_all(pkg.join("build/nros")).unwrap();
        fs::create_dir_all(pkg.join("generated/std_msgs")).unwrap();
        fs::create_dir_all(pkg.join(".cargo")).unwrap();
        let system = "[system]\nname = \"sensor_node\"\n\n[[component]]\npkg = \"sensor_node\"\n\
                      class = \"sensor_node::Talker\"\nname = \"sensor_node\"\n\n\
                      [image.linux]\nboard = \"linux\"\n";
        fs::write(pkg.join("system.toml"), system).unwrap();
        fs::write(pkg.join("package.xml"), "<package/>").unwrap();
        fs::write(pkg.join("README.md"), "# sensor_node").unwrap();
        fs::write(pkg.join("src/lib.rs"), "// node").unwrap();
        fs::write(pkg.join("src/main.rs"), "nros::main!();").unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname = \"sensor_node\"\n\n[workspace]\n\n[[bin]]\nname = \"sensor_node\"\n\
             path = \"src/main.rs\"\n\n# the node\n[lib]\npath = \"src/lib.rs\"\n\n\
             [dependencies]\nnros = { version = \"*\" }\n\
             sensor_node_nros_selection = { path = \"build/nros/selection\" }\n\
             std_msgs = { path = \"generated/std_msgs\" }  # nros-managed\n",
        )
        .unwrap();
        fs::write(pkg.join(".cargo/config.toml"), "include = []").unwrap();

        let ws = t.path().join("my_ws");
        let bringup = grow_workspace(&ws, &pkg, false).unwrap();
        let node = ws.join("src/sensor_node");
        assert_eq!(bringup, ws.join("src/sensor_node_bringup"));
        assert!(ws.join(".colcon_workspace").is_file());
        // The deployment moves to the bringup byte for byte; its component row
        // already names the node package.
        assert_eq!(
            fs::read_to_string(bringup.join("system.toml")).unwrap(),
            system
        );
        assert!(
            fs::read_to_string(bringup.join("package.xml"))
                .unwrap()
                .contains("<exec_depend>sensor_node</exec_depend>")
        );
        let launch = fs::read_to_string(bringup.join("launch/system.launch.xml")).unwrap();
        assert!(
            launch.contains(r#"<node pkg="sensor_node" exec="sensor_node" name="sensor_node"/>"#),
            "{launch}"
        );
        // The node moves unchanged; the entry half stays behind.
        assert_eq!(
            fs::read_to_string(node.join("src/lib.rs")).unwrap(),
            "// node"
        );
        assert_eq!(
            fs::read_to_string(node.join("package.xml")).unwrap(),
            "<package/>"
        );
        for gone in [
            "build",
            "generated",
            ".cargo",
            "system.toml",
            "README.md",
            "src/main.rs",
        ] {
            assert!(!node.join(gone).exists(), "{gone} was carried");
        }
        let manifest = fs::read_to_string(node.join("Cargo.toml")).unwrap();
        for gone in ["[[bin]]", "src/main.rs", "nros_selection", "# nros-managed"] {
            assert!(!manifest.contains(gone), "`{gone}` kept:\n{manifest}");
        }
        for kept in ["[lib]", "# the node", "nros = { version"] {
            assert!(manifest.contains(kept), "`{kept}` lost:\n{manifest}");
        }
        manifest
            .parse::<toml::Table>()
            .expect("the node manifest parses");
        // No root build file: the workspace is the marker and `src/`.
        assert!(!ws.join("Cargo.toml").exists() && !ws.join("CMakeLists.txt").exists());
        // The original stays.
        assert!(pkg.join("system.toml").is_file());
        // A second run refuses rather than overwrite.
        assert!(grow_workspace(&ws, &pkg, false).is_err());
        // A workspace inside the package is refused.
        assert!(grow_workspace(&pkg.join("ws"), &pkg, false).is_err());
    }

    #[test]
    fn growing_refuses_a_directory_that_is_not_a_scaffolded_package() {
        let t = tempfile::tempdir().unwrap();
        let e = grow_workspace(&t.path().join("ws"), t.path(), false)
            .unwrap_err()
            .to_string();
        assert!(e.contains("no system.toml"), "{e}");
        fs::write(t.path().join("system.toml"), "[system]\nname = \"x\"\n").unwrap();
        fs::write(t.path().join("package.xml"), "<package/>").unwrap();
        let e = grow_workspace(&t.path().join("ws"), t.path(), false)
            .unwrap_err()
            .to_string();
        assert!(e.contains("no [image.<id>]"), "{e}");
        // A C/C++ package (no Cargo.toml) is refused, naming why.
        fs::write(
            t.path().join("system.toml"),
            "[system]\nname = \"x\"\n[image.linux]\nboard = \"linux\"\n",
        )
        .unwrap();
        let e = grow_workspace(&t.path().join("ws"), t.path(), false)
            .unwrap_err()
            .to_string();
        assert!(e.contains("C or C++"), "{e}");
    }
}

//! `nros board new <name>` — scaffold a board package.
//!
//! phase-375 W2/W9. Onboarding a board used to be a scavenger hunt, and the
//! phase's own measurement is why this exists: `s32z270` landed RED on five
//! gates (weak symbols, board tiers, leaf lock, provider announcements, matrix
//! orphan) and `freertos-posix` on two plus a lane-table cascade. Each gate was
//! correct. The cost was discovering them serially, on main, by whoever noticed.
//!
//! So the scaffold's acceptance is not "it writes some files" — it is that a
//! board created by it is green on `just check fast` before its first commit.
//!
//! W2 was written before there WAS one board shape to scaffold; W6–W8 made one
//! (a `package.xml` announcement beside an `nros-board.toml`), which is why
//! this lands after them rather than before.
//!
//! `--out-of-tree <dir>` writes the same package somewhere else and prints the
//! one line that makes it resolvable. That is the whole difference between our
//! board and a user's, and keeping it to one flag is the point of RFC-0064 R5
//! D1.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail};

#[derive(Debug, ClapArgs)]
pub struct NewArgs {
    /// Board key, kebab-case. Becomes the directory name, the canonical
    /// `names` entry and the announcement.
    pub name: String,
    /// Platform this board runs on (`zephyr`, `freertos`, `nuttx`, `threadx`,
    /// `bare-metal`, `posix`, `esp32`).
    #[arg(long)]
    pub platform: String,
    /// Zephyr board id (`<board>/<soc>/<variant>`). Required for, and only
    /// meaningful on, `--platform zephyr`.
    #[arg(long)]
    pub west_board: Option<String>,
    /// Write the package outside the nano-ros tree. The board is then reached
    /// through `$NROS_EXTRA_BOARD_PATH`, which this prints.
    #[arg(long)]
    pub out_of_tree: Option<PathBuf>,
    /// nano-ros checkout to write the in-tree board into (auto-detected from
    /// cwd if omitted). Not read with `--out-of-tree`, which needs no checkout.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print what would be written and write nothing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Board key → the package name a `package.xml` declares.
fn package_name(board: &str) -> String {
    format!("nros_board_{}", board.replace(['-', '.'], "_"))
}

pub fn run(args: NewArgs) -> Result<()> {
    if args.name.is_empty()
        || !args
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!(
            "board key `{}` must be kebab-case ([a-z0-9-]). It becomes a \
             directory name, a cargo feature and a cmake value, and a key that \
             is legal in one of those and not the others fails late.",
            args.name
        );
    }
    if args.platform == "zephyr" && args.west_board.is_none() {
        bail!(
            "--platform zephyr needs --west-board <board>/<soc>/<variant>.\n  \
             It is the one irreducible fact about a Zephyr board: the per-board \
             conf and overlay filenames derive from it, and so does the \
             Rust-support Kconfig symbol."
        );
    }
    if args.platform != "zephyr" && args.west_board.is_some() {
        bail!("--west-board is only meaningful with --platform zephyr");
    }

    // Issue 2291 -- only the in-tree arm writes into a checkout, so only it
    // asks for one. The out-of-tree road is the installed-track user's, who
    // has no checkout to find. The in-tree arm keeps the checkout walk and not
    // `board::catalog_root`: that ladder also answers with an installed SDK
    // root, and a scaffold must never write into an install prefix.
    let pkg_dir = format!("nros-board-{}", args.name);
    let (dir, root) = match &args.out_of_tree {
        Some(base) => (base.join(&pkg_dir), None),
        None => {
            let root = match &args.workspace {
                Some(p) => p.clone(),
                None => crate::cmd::board::find_workspace_root()?,
            };
            (
                root.join("packages").join("boards").join(&pkg_dir),
                Some(root),
            )
        }
    };
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }

    let descriptor = render_descriptor(&args);
    let package_xml = render_package_xml(&args);

    if args.dry_run {
        println!(
            "would write {}/nros-board.toml:\n{descriptor}",
            dir.display()
        );
        println!("would write {}/package.xml:\n{package_xml}", dir.display());
        print_next_steps(&args, &dir);
        return Ok(());
    }

    std::fs::create_dir_all(&dir).wrap_err_with(|| format!("create {}", dir.display()))?;
    std::fs::write(dir.join("nros-board.toml"), &descriptor)?;
    std::fs::write(dir.join("package.xml"), &package_xml)?;
    if args.platform == "zephyr" {
        // An empty `prj.conf` is written on purpose: the projection picks it up
        // present-if-exists, so an absent one is indistinguishable from a board
        // whose author has not got to it yet.
        std::fs::write(
            dir.join("prj.conf"),
            "# Base Kconfig for this board, layered ahead of the consumer's own\n\
             # prj.conf by `nano_ros_use_board()`.\n",
        )?;
    }

    if let Some(root) = &root {
        append_registry_row(root, &args)?;
    }

    println!("wrote {}", dir.display());
    print_next_steps(&args, &dir);
    Ok(())
}

fn render_descriptor(args: &NewArgs) -> String {
    let mut out = format!(
        // See RFC-0064 R5 D1.
        "# Board descriptor for `{name}`.\n\
         #\n\
         # A board is a PACKAGE: this file says what the board\n\
         # lowers to, and the `package.xml` beside it announces that the board\n\
         # exists. Both are required — a descriptor with no announcement is\n\
         # refused, because it would resolve here and be invisible to every\n\
         # consumer that reaches providers through `provider_scan`.\n\
         \n\
         [[board]]\n\
         names = [\"{name}\"]\n\
         platform = \"{platform}\"\n\
         toolchain = \"stable\"\n\
         platform_feature = \"platform-{platform}\"\n",
        name = args.name,
        platform = args.platform,
    );
    out.push_str(
        "\
         # `hosted-main` when the host OS owns `main`, `zephyr-staticlib` on\n\
         # Zephyr, `board-run` when the board crate's `run()` is the entry.\n\
         # NOT derivable from `platform`: freertos carries both hosted and bare\n\
         # boards.\n",
    );
    out.push_str(&format!(
        "entry_kind = \"{}\"\n",
        if args.platform == "zephyr" {
            "zephyr-staticlib"
        } else if args.platform == "posix" {
            "hosted-main"
        } else {
            "board-run"
        }
    ));
    out.push_str(
        // See phase-351 W4.
        "# An EMPTY list is a statement, not an omission: a deploy naming a\n\
         # netstack this board does not support is refused.\n\
         supported_netstacks = []\n",
    );
    if let Some(wb) = &args.west_board {
        out.push_str(&format!(
            "\n[board.zephyr]\n\
             west_board = \"{wb}\"\n\
             # Derived, so absent on purpose: prj.conf and\n\
             # `boards/<west_board with / and @ as _>.{{conf,overlay}}`,\n\
             # present-if-exists. Zephyr's own board definition selects the\n\
             # emulator `west build -t run` drives.\n"
        ));
    }
    // phase-528 W2 (RFC-0118 D4) — how the board runs is stated ONCE, in
    // `[board.run]`; a host board says so, any other gets the shape to fill.
    if args.platform == "posix" {
        out.push_str(
            // See RFC-0118 D4.
            "\n# How this board runs: a host process.\n\
             [board.run]\n\
             host = true\n\
             \n\
             [board.console]\n\
             kind = \"stdio\"\n",
        );
    } else {
        out.push_str(
            // See RFC-0118 D4.
            "\n# How this board runs, when it has an emulator or a\n\
             # twin: `emulator = \"qemu:<machine>\" | \"fvp:<model>\" |\n\
             # \"renode:<platform>\"`. The support registry reads\n\
             # it from here. A QEMU board adds the line's facts, which `nros build`\n\
             # prints and the generated cargo settings use as `runner`:\n\
             #\n\
             # [board.run]\n\
             # emulator = \"qemu:<machine>\"\n\
             #\n\
             # [board.run.qemu]\n\
             # system = \"arm\"            # qemu-system-<system>\n\
             # cpu = \"cortex-m3\"\n\
             # icount = \"shift=auto\"\n\
             # semihosting = true\n\
             # nic_model = \"lan9118\"\n\
             #\n\
             # [board.console]\n\
             # kind = \"semihosting\"      # stdio | serial | rtt | semihosting\n",
        );
    }
    out.push_str(
        "\n[board.capabilities]\n\
         heap = true\n\
         atomics = true\n\
         threads = true\n",
    );
    out
}

fn render_package_xml(args: &NewArgs) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n\
         <package format=\"3\">\n  \
         <name>{pkg}</name>\n  \
         <version>0.0.0</version>\n  \
         <description>\n    \
         Board support for `{name}` ({platform}).\n  \
         </description>\n  \
         <maintainer email=\"dev@example.com\">Developer</maintainer>\n  \
         <license>Apache-2.0</license>\n\
         \n  \
         <export>\n    \
         <build_type>{build_type}</build_type>\n    \
         <!--\n      \
         Provision, NOT consumption: this package IS a board. Must equal the\n      \
         descriptor's `names`, canonical first — `check-provider-announcements`\n      \
         compares them, because two spellings of one fact drift.\n    \
         -->\n    \
         <nano_ros_provides kind=\"board\" name=\"{name}\"/>\n  \
         </export>\n\
         </package>\n",
        pkg = package_name(&args.name),
        name = args.name,
        platform = args.platform,
        build_type = if args.platform == "zephyr" {
            "nros_cmake"
        } else {
            "nros_cargo"
        },
    )
}

/// Append the `board-support.toml` row, at `runs = "none"` (RFC-0114 D10).
///
/// A new board boots nowhere until a cell says otherwise, so its derived
/// status word is "Not supported" (or "Builds" once a fixture builds it), and
/// it claims no breadth yet. That is the honest start: a row that claims a run
/// needs a named maintainer (phase-375 W1, re-keyed by RFC-0114: 2 for a
/// Verified row, 1 for Boots or Builds), and one that claimed a run nothing
/// shows would be exactly the unearned claim the registry gate refuses.
fn append_registry_row(root: &Path, args: &NewArgs) -> Result<()> {
    let path = root.join("packages/boards/board-support.toml");
    let mut body =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    // Trailing blank line, not just a leading one: the row is inserted BEFORE
    // an existing section, so without it the next `[[board]]` ends up glued to
    // this row's last key. Valid TOML, unreadable diff.
    let row = format!(
        "[[board]]\ncrate = \"nros-board-{}\"\nruns = \"none\"\nmaintainers = []\n\n",
        args.name
    );
    // Before the `# ===== infra` section, which is not boards.
    match body.find("# ===== infra") {
        Some(i) => body.insert_str(i, &row),
        None => body.push_str(&row),
    }
    std::fs::write(&path, body)?;
    Ok(())
}

fn print_next_steps(args: &NewArgs, dir: &Path) {
    println!("\nnext:");
    for line in next_steps(args, dir) {
        println!("  {line}");
    }
}

/// The scaffold's next-step lines. Every one on the out-of-tree road is a
/// user's step: no `just` recipe, no `nros dev` verb (issue 2294).
fn next_steps(args: &NewArgs, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(base) = &args.out_of_tree {
        out.push(format!(
            "export NROS_EXTRA_BOARD_PATH={}   # how this board is reached",
            base.display()
        ));
    } else {
        out.push("packages/boards/board-support.toml  # row added at runs = \"none\"".into());
    }
    if args.platform == "zephyr" {
        let flat: String = args
            .west_board
            .as_deref()
            .unwrap_or("")
            .chars()
            .map(|c| if matches!(c, '/' | '@' | '.') { '_' } else { c })
            .collect();
        out.push(format!(
            "{}/boards/{flat}.conf      # optional, picked up if present",
            dir.display()
        ));
        out.push(format!(
            "{}/boards/{flat}.overlay   # optional, picked up if present",
            dir.display()
        ));
        out.push(format!(
            "nano_ros_use_board({})  # in the consumer's CMakeLists",
            args.name
        ));
    }
    out.push(format!(
        "nros check {}   # resolves the board, its announcement, network and projection",
        args.name
    ));
    // The checkout's own acceptance, and only the checkout has the recipe. The
    // out-of-tree road is a user's, who has no `just` recipes to run (issue
    // 2292 routed `nros new board` here too).
    if args.out_of_tree.is_none() {
        out.push(
            "just check fast   # the scaffold's acceptance: green before the first commit".into(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(platform: &str, west_board: Option<&str>, out_of_tree: Option<&Path>) -> NewArgs {
        NewArgs {
            name: "acme".into(),
            platform: platform.into(),
            west_board: west_board.map(Into::into),
            out_of_tree: out_of_tree.map(Path::to_path_buf),
            workspace: None,
            dry_run: false,
        }
    }

    /// Issue 2294 — the out-of-tree road names `nros check <key>` and no
    /// contributor step; the in-tree arm keeps the checkout's `just check
    /// fast` after it.
    #[test]
    fn the_next_steps_name_the_user_check() {
        let dir = Path::new("/x/nros-board-acme");
        for (platform, wb) in [("freertos", None), ("zephyr", Some("acme/soc"))] {
            let user = next_steps(&args(platform, wb, Some(Path::new("/x"))), dir);
            assert!(
                user.iter().any(|l| l.starts_with("nros check acme ")),
                "{user:?}"
            );
            assert!(
                user.iter()
                    .all(|l| !l.contains("just ") && !l.contains("nros dev")),
                "{user:?}"
            );
            let tree = next_steps(&args(platform, wb, None), dir);
            let check = tree.iter().position(|l| l.starts_with("nros check acme "));
            let fast = tree.iter().position(|l| l.starts_with("just check fast"));
            assert!(check.is_some() && check < fast, "{tree:?}");
        }
    }

    /// The scaffold's own output passes `nros check`'s board steps: what it
    /// writes is a board the check accepts, Zephyr projection included.
    #[test]
    fn a_scaffolded_board_passes_the_board_check() {
        use crate::orchestration::board_descriptor::BoardCatalog;
        for (platform, wb) in [
            ("freertos", None),
            ("posix", None),
            ("zephyr", Some("acme_board/acme_soc")),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let sdk = tmp.path().join("sdk");
            std::fs::create_dir_all(sdk.join("packages/boards")).unwrap();
            let base = tmp.path().join("boards");
            let a = args(platform, wb, Some(&base));
            let dir = base.join("nros-board-acme");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("nros-board.toml"), render_descriptor(&a)).unwrap();
            std::fs::write(dir.join("package.xml"), render_package_xml(&a)).unwrap();
            let catalog = BoardCatalog::load_with_extra(&sdk, std::slice::from_ref(&base)).unwrap();
            crate::cmd::board::board_check_in(&catalog, &sdk, "acme")
                .unwrap_or_else(|e| panic!("{platform}: {e:#}"));

            // An announcement that drifts from the descriptor fails the check.
            let xml = render_package_xml(&a).replace(
                "name=\"acme\"/>",
                "name=\"acme\"/>\n    <nano_ros_provides kind=\"board\" name=\"other\"/>",
            );
            std::fs::write(dir.join("package.xml"), xml).unwrap();
            let catalog = BoardCatalog::load_with_extra(&sdk, std::slice::from_ref(&base)).unwrap();
            let err = crate::cmd::board::board_check_in(&catalog, &sdk, "acme")
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("announces boards") && err.contains("other"),
                "{err}"
            );
        }
    }
}

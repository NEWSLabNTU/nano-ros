//! The CMake `-D` names `nros build` passes itself (RFC-0117 D9, phase-568 W1).
//!
//! A user argument naming one of these (`--cmake-args=-D<name>=…`, or the
//! image's `[image.<id>.build] cmake_args`) would fight the value the build
//! computes, so D9 refuses it, naming the setting that owns it instead.
//!
//! The table is not trusted as written. `tests::every_emitted_define_is_owned`
//! harvests every `-D` the `nros` binary's own source emits, across every
//! crate compiled into it (the CLI's source closure, the same set its
//! freshness stamp hashes: `packages/cli` plus `cli-source-dirs.txt`), and
//! fails on any name missing here. It also fails on any `-D` whose name it
//! cannot READ (a computed name), so a new road, function or crate is covered
//! the moment it emits, with no fixture to forget. A list written by hand would
//! miss the unprefixed names, like `CONF_FILE` and `CMAKE_TOOLCHAIN_FILE`.
//!
//! A site whose name the harvest cannot read declares it with an
//! `// owned-define: <NAME>` comment on one of the three lines above it. A
//! test checks the declared name against the value the site formats from.
//! `// owned-define: user` marks the one other kind of site: re-emitting a
//! user's own argument after `build_args::parse_cmake` validated it, whose
//! name no table could list.

/// How a table row matches a `-D` name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// The name itself.
    Exact,
    /// Every name starting with this.
    Prefix,
}

/// One row: a name (or family) and why the build owns it.
#[derive(Clone, Copy, Debug)]
pub struct OwnedDefine {
    pub name: &'static str,
    pub matches: Match,
    pub why: &'static str,
}

const fn exact(name: &'static str, why: &'static str) -> OwnedDefine {
    OwnedDefine {
        name,
        matches: Match::Exact,
        why,
    }
}

/// Every `-D` name `nros build` passes. Sorted by name; the harvest test
/// keeps it complete.
pub const OWNED: &[OwnedDefine] = &[
    exact("APPLICATION_CONFIG_DIR", "the west road's app config dir"),
    exact(
        "ARMFVP_EXTRA_FLAGS",
        "the FVP model parameters for an fvp: board's run",
    ),
    exact(
        "CMAKE_BUILD_TYPE",
        "the image's profile (`[image.<id>] profile`, `--profile=`)",
    ),
    exact("CMAKE_PREFIX_PATH", "where the probe finds nano-ros"),
    exact("CMAKE_PROJECT_INCLUDE", "the census probe's host shim"),
    exact("CMAKE_TOOLCHAIN_FILE", "the board's toolchain"),
    exact("EXTRA_CONF_FILE", "the image's `conf = [..]` fragments"),
    exact("EXTRA_DTC_OVERLAY_FILE", "the image's devicetree overlays"),
    exact("FILE_SUFFIX", "the west road's build-type suffix"),
    exact(
        "NROS_BOARD_HEAP_BUDGET_BYTES",
        "the board heap, resolved at stage 4",
    ),
    exact("NROS_BOARD_MODULE", "the board package's CMake module"),
    exact("NROS_BOARD_RX_FLOOR_US", "the board's receive timing"),
    exact("NROS_BOARD_RX_FRAME_COST_US", "the board's receive timing"),
    exact("NROS_BOARD_SNIPPETS", "the board's Zephyr snippets"),
    exact("NROS_BRINGUP_DIR", "the bringup this image is built from"),
    exact("NROS_CARGO_PROFILE", "the image's profile"),
    exact("NROS_CONFIGURED_IMAGE", "which image this configure builds"),
    exact(
        "NROS_CONFIGURED_PROJECT",
        "which project this configure builds",
    ),
    exact("NROS_IMAGE", "the image id"),
    exact("NROS_IMAGE_LANGUAGE", "the image's language"),
    exact("NROS_IMAGE_NET", "the image's resolved network"),
    exact("NROS_IMAGE_STACK_FLOOR_IMAGE", "the stack-floor raise"),
    exact("NROS_IMAGE_STACK_FLOOR_MODE", "the stack-floor raise"),
    exact("NROS_IMAGE_SYSTEM_DIR", "the image's system.toml dir"),
    exact(
        "NROS_KNOB_CATALOG",
        "the merged configuration catalog `nros configure` wrote",
    ),
    exact("NROS_LEAF_BOARD", "the census probe's board"),
    exact("NROS_REPO_DIR", "the nano-ros checkout"),
    exact("NROS_RMW", "the RMW, derived from `[system] rmw`"),
    exact("NROS_SHARED_CARGO_BUILD_DIR", "the shared cargo target"),
    exact("NROS_SHARED_CARGO_ROOT", "the shared cargo target"),
    exact(
        "NROS_SHARED_IMAGE_CRATE_ROOT",
        "the shared image roots of the metadata probe",
    ),
    exact(
        "NROS_WEST_CONFIGURE_SHA256",
        "the west configure's input digest",
    ),
    exact("NROS_WORKSPACE_ROOT", "the workspace root"),
    exact("NROS_WS_PREAMBLE", "the workspace's cmake preamble"),
    exact("ZEPHYR_EXTRA_MODULES", "the nano-ros Zephyr module"),
    exact("Zephyr_DIR", "the census probe's Zephyr stub"),
    exact(
        "_NANO_ROS_CODEGEN_TOOL",
        "the codegen binary this build uses",
    ),
    exact("nano_ros_DIR", "where find_package(nano_ros) looks"),
    exact("nano_ros_ROOT", "the nano-ros checkout"),
];

/// Names a user sets and nano-ros READS, so they are deliberately NOT owned.
/// Refusing them would break the feature they exist for.
pub const USER_SET: &[OwnedDefine] = &[OwnedDefine {
    name: "NROS_OVERRIDE_",
    matches: Match::Prefix,
    why: "`-DNROS_OVERRIDE_<ID>=<v>`, a one-build override of a Kconfig knob that \
          `leaf_kconfig` reads; the build never emits it",
}];

/// `-D` flags the harvest finds that are not CMake arguments at all: C
/// preprocessor defines a build helper passes straight to a compiler. They are
/// not configure input, so D9 has nothing to refuse, and they are listed only
/// so the harvest can tell them from a missing row.
pub const COMPILER_DEFINES: &[OwnedDefine] = &[
    exact(
        "__KERNEL__",
        "nros-board-common's NuttX helpers, to the cross compiler (linker-script cpp and the builtins stub)",
    ),
    exact(
        "__NuttX__",
        "nros-board-common's NuttX helpers, to the cross compiler (linker-script cpp and the builtins stub)",
    ),
];

fn row_matches(row: &OwnedDefine, name: &str) -> bool {
    match row.matches {
        Match::Exact => row.name == name,
        Match::Prefix => name.starts_with(row.name),
    }
}

/// The row owning `name`, if `nros build` passes it.
pub fn owned(name: &str) -> Option<&'static OwnedDefine> {
    if USER_SET.iter().any(|r| row_matches(r, name)) {
        return None;
    }
    OWNED.iter().find(|r| row_matches(r, name))
}

/// Test helper: every `-D` in `args` is owned (the runtime cross-check the
/// harvest is backed by, run on the pure argument helpers).
#[cfg(test)]
pub(crate) fn assert_all_owned(args: &[String]) {
    for a in args {
        if let Some(def) = a.strip_prefix("-D") {
            let name = def.split(['=', ':']).next().unwrap_or(def);
            assert!(
                owned(name).is_some(),
                "`{a}` passes a -D OWNED does not name"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::{Path, PathBuf},
    };

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("nros-cli-core sits three levels under the repo root")
            .to_path_buf()
    }

    /// A `-D` the harvest found.
    #[derive(Debug, PartialEq, Eq)]
    enum Found {
        Name(String),
        /// A name the harvest cannot read, with the offending text.
        Unreadable(String),
    }

    /// Code with comment lines and `#[cfg(test)] mod … { … }` blocks removed,
    /// line numbers kept (removed lines become empty). Test modules are
    /// rustfmt-shaped: the attribute and `mod` at column 0, closed by a `}` at
    /// column 0.
    fn non_test_code(src: &str) -> Vec<String> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::with_capacity(lines.len());
        let mut i = 0;
        while i < lines.len() {
            if lines[i] == "#[cfg(test)]"
                && lines
                    .get(i + 1)
                    .is_some_and(|l| l.starts_with("mod ") && l.ends_with('{'))
            {
                while i < lines.len() && lines[i] != "}" {
                    out.push(String::new());
                    i += 1;
                }
                out.push(String::new());
                i += 1;
                continue;
            }
            out.push(lines[i].to_string());
            i += 1;
        }
        out
    }

    /// Every `-D` emitted in `src`, with its line. `"-D<NAME>…"` is read
    /// directly, `"-D{<IDENT>}…"` through the `const <IDENT>: &str` it names,
    /// and anything else through an `// owned-define: <NAME>` marker above it.
    /// A parse (`strip_prefix("-D")`, `starts_with("-D")`) is not an emission.
    fn harvest(src: &str, consts: &BTreeMap<String, String>) -> Vec<(usize, Found)> {
        let lines = non_test_code(src);
        let mut found = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            let mut rest = line.as_str();
            while let Some(at) = rest.find("\"-D") {
                let after = &rest[at + 3..];
                let before = &rest[..at];
                rest = after;
                if before.ends_with("strip_prefix(") || before.ends_with("starts_with(") {
                    continue;
                }
                let ident: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let item = if !ident.is_empty() {
                    Found::Name(ident)
                } else if let Some(inner) = after.strip_prefix('{') {
                    let id: String = inner
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    match (inner[id.len()..].starts_with('}'), consts.get(&id)) {
                        (true, Some(v)) if !id.is_empty() => Found::Name(v.clone()),
                        _ => marker(&lines, n)
                            .map(Found::Name)
                            .unwrap_or_else(|| Found::Unreadable(code.to_string())),
                    }
                } else {
                    marker(&lines, n)
                        .map(Found::Name)
                        .unwrap_or_else(|| Found::Unreadable(code.to_string()))
                };
                found.push((n + 1, item));
            }
        }
        found
    }

    /// An `// owned-define: <NAME>` comment on one of the three lines above `n`.
    fn marker(lines: &[String], n: usize) -> Option<String> {
        (n.saturating_sub(3)..n).rev().find_map(|i| {
            lines[i]
                .trim()
                .strip_prefix("// owned-define:")
                .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        })
    }

    /// `const <NAME>: &str = "<value>";` across the harvested code. A name
    /// defined twice with different values resolves to nothing, so the site
    /// falls to the marker rule rather than to the wrong value.
    fn string_consts(sources: &[(PathBuf, String)]) -> BTreeMap<String, String> {
        let mut seen: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (_, src) in sources {
            for line in src.lines() {
                let l = line.trim_start();
                let l = l.strip_prefix("pub ").unwrap_or(l);
                let l = l.strip_prefix("pub(crate) ").unwrap_or(l);
                let Some(l) = l.strip_prefix("const ") else {
                    continue;
                };
                let Some((name, rest)) = l.split_once(": &str = \"") else {
                    continue;
                };
                let Some(value) = rest.strip_suffix("\";") else {
                    continue;
                };
                seen.entry(name.trim().to_string())
                    .or_default()
                    .insert(value.to_string());
            }
        }
        seen.into_iter()
            .filter(|(_, v)| v.len() == 1)
            .map(|(k, v)| (k, v.into_iter().next().unwrap()))
            .collect()
    }

    /// The `nros` binary's source closure: `packages/cli` plus every dir
    /// `cli-source-dirs.txt` names, which is generated from cargo's own
    /// resolution and gated by `check-cli-source-dirs`. Rust sources only,
    /// minus vendored code, CLI test fixtures, integration tests and build
    /// scripts (none of which is in the binary).
    fn closure(root: &Path) -> (Vec<String>, Vec<(PathBuf, String)>) {
        let dirs = crate::source_stamp::cli_source_dirs(root);
        let mut files = Vec::new();
        for d in &dirs {
            walk(&root.join(d), &mut files);
        }
        files.sort();
        let sources = files
            .into_iter()
            .filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s)))
            .collect();
        (dirs, sources)
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if matches!(
                    name.as_str(),
                    "target" | "third-party" | "testing_workspaces" | "tests" | "build" | ".git"
                ) {
                    continue;
                }
                walk(&p, out);
            } else if name.ends_with(".rs")
                && !(name == "build.rs" && dir.join("Cargo.toml").is_file())
            {
                // A crate's build script runs at compile time and is not in the
                // binary; `src/cmd/build.rs` is.
                out.push(p);
            }
        }
    }

    #[test]
    fn every_emitted_define_is_owned() {
        let root = repo_root();
        let (dirs, sources) = closure(&root);
        assert!(
            sources.len() > 100,
            "harvested only {} file(s) from {dirs:?}: the closure is wrong",
            sources.len()
        );
        let consts = string_consts(&sources);
        let mut problems = Vec::new();
        let mut names = BTreeSet::new();
        for (path, src) in &sources {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            for (line, f) in harvest(src, &consts) {
                match f {
                    Found::Name(n) if n == "user" => {
                        // Allowed only where user arguments are re-emitted.
                        if !rel.ends_with("builder/build_args.rs") {
                            problems.push(format!(
                                "{rel}:{line}: `// owned-define: user` outside build_args.rs, \
                                 where only validated user arguments are re-emitted"
                            ));
                        }
                    }
                    Found::Name(n) => {
                        let accounted = USER_SET
                            .iter()
                            .chain(COMPILER_DEFINES)
                            .any(|r| row_matches(r, &n));
                        if !accounted && owned(&n).is_none() {
                            problems.push(format!("{rel}:{line}: `-D{n}` is not in OWNED"));
                        }
                        names.insert(n);
                    }
                    Found::Unreadable(code) => problems.push(format!(
                        "{rel}:{line}: a `-D` whose name cannot be read: `{code}` \
                         (format from a `const NAME: &str`, or put \
                         `// owned-define: <NAME>` on a line above it)"
                    )),
                }
            }
        }
        assert!(
            problems.is_empty(),
            "harvested {} file(s) from the `nros` binary's source closure {dirs:?}:\n  {}",
            sources.len(),
            problems.join("\n  ")
        );
        // A row nothing emits is stale.
        let stale: Vec<_> = OWNED
            .iter()
            .filter(|r| r.matches == Match::Exact && !names.contains(r.name))
            .map(|r| r.name)
            .collect();
        assert!(
            stale.is_empty(),
            "OWNED rows no `-D` in {dirs:?} emits: {stale:?}"
        );
    }

    #[test]
    fn the_harvest_fails_closed() {
        let consts: BTreeMap<String, String> =
            [("DIGEST".to_string(), "NROS_DIGEST".to_string())].into();
        let src = [
            "fn a() {",
            "    // \"-DIN_A_COMMENT=1\"",
            "    let x = format!(\"-DNROS_IMAGE={id}\");",
            "    let y = format!(\"-D{DIGEST}={d}\");",
            "    let z = format!(\"-D{}={}\", path::ENV, v);",
            "    // owned-define: NROS_IMAGE_NET",
            "    let w = format!(",
            "        \"-D{}={}\",",
            "        other::ENV, v);",
            "    if let Some(d) = a.strip_prefix(\"-D\") {}",
            "}",
            "",
            "#[cfg(test)]",
            "mod tests {",
            "    fn t() { let _ = \"-DIN_A_TEST=1\"; }",
            "}",
        ]
        .join("\n");
        let got = harvest(&src, &consts);
        assert_eq!(
            got,
            vec![
                (3, Found::Name("NROS_IMAGE".into())),
                (4, Found::Name("NROS_DIGEST".into())),
                (
                    5,
                    Found::Unreadable("let z = format!(\"-D{}={}\", path::ENV, v);".into())
                ),
                (8, Found::Name("NROS_IMAGE_NET".into())),
            ]
        );
    }

    #[test]
    fn an_unowned_name_is_refused_and_a_user_set_one_is_not_owned() {
        assert!(owned("CMAKE_TOOLCHAIN_FILE").is_some());
        assert!(owned("NROS_IMAGE").is_some());
        assert!(owned("MY_OPTION").is_none());
        assert!(owned("NROS_OVERRIDE_EXECUTOR_MAX_CBS").is_none());
    }

    #[test]
    fn the_public_argument_helpers_pass_only_owned_names() {
        let p = Path::new("/x");
        let mut args = vec![
            crate::builder::nano_ros_root_arg(p),
            crate::builder::codegen_tool_arg_for(p),
            crate::builder::workspace_root_arg(p),
            crate::builder::zephyr::module_arg(p),
        ];
        args.extend(crate::builder::zephyr::west_args(
            &crate::builder::zephyr::ZephyrOverlays {
                application_config_dir: Some(p.into()),
                extra_conf_file: vec![p.into()],
                extra_dtc_overlay_file: vec![p.into()],
                build_type: Some("debug".into()),
                ..Default::default()
            },
        ));
        assert!(args.len() >= 8, "{args:?}");
        assert_all_owned(&args);
    }

    #[test]
    fn the_image_net_marker_names_the_value_it_formats() {
        assert_eq!(nros_board_common::image_net::ENV, "NROS_IMAGE_NET");
    }
}

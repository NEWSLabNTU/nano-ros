//! RFC-0120 D4 (phase-530 W2) -- where a standalone leaf's entity counts
//! come from. ONE rule, asked by every reader of a leaf's entities: the
//! descriptor's composer, the leaf reader, the Kconfig rendering, `nros build`'s
//! leaf road, `take --leaf`, and (through `nros dev image entity-census
//! source`) the cmake and Zephyr hooks, which used to grep `system.toml` for
//! one word. Always for the image the build SELECTED ([`read`]), never the
//! leaf's default one.
//!
//! * a component that states `entities = [...]` -> **declared** (it wins, and
//!   needs no census);
//! * `entities = "census"` -> the **census**, as since issue 1556;
//! * `entities = "defaults"` on a component, or `[system] entities =
//!   "defaults"` -> the crate **defaults**, written down;
//! * nothing stated, on a C/C++ leaf (a `CMakeLists.txt` and no `Cargo.toml`)
//!   for a CROSS image -> the **census, by default**. That holds for a leaf
//!   with no `[[component]]` too when its `CMakeLists.txt` builds a program
//!   (Ruling R29); one with no component and no program has nothing a census
//!   could run, so it is [`EntitySource::NoProgram`] and every reader refuses
//!   it. A leaf is sized tight or says it is loose; nothing is silently loose.
//!
//! What stays [`EntitySource::Unstated`]: a Rust leaf (the metadata probe
//! counts it, RFC-0120 D4's other half) and a HOST image -- the census
//! producer's own platform, sized at the hosted defaults as before, the same
//! line `build::census_take_is_ours` draws for a workspace.

use std::path::Path;

use nros_orchestration_ir::leaf_system::LeafSystem;

/// Where a leaf's entity counts come from. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitySource {
    /// Some component states `entities = [...]`.
    Declared,
    /// The leaf's census. `explicit` when `entities = "census"` is written,
    /// `false` when it is the C/C++ default.
    Census { explicit: bool },
    /// `entities = "defaults"`, on a component or on `[system]`.
    Defaults,
    /// A cross C/C++ leaf with no `[[component]]`, no program in its
    /// `CMakeLists.txt` and nothing stated: refused (Ruling R29).
    NoProgram,
    /// Nothing stated and no default applies: a Rust leaf or a host image.
    Unstated,
}

impl EntitySource {
    /// The word `entity-census source` prints, which the cmake hooks match.
    pub fn word(self) -> &'static str {
        match self {
            EntitySource::Declared => "declared",
            EntitySource::Census { .. } => "census",
            EntitySource::Defaults => "defaults",
            EntitySource::NoProgram => "no-program",
            EntitySource::Unstated => "unstated",
        }
    }

    pub fn is_census(self) -> bool {
        matches!(self, EntitySource::Census { .. })
    }

    /// The refusal a reader stops with, naming the fixes; `None` for every
    /// source a reader can answer from.
    pub fn refusal(self, dir: &Path) -> Option<String> {
        (self == EntitySource::NoProgram).then(|| {
            format!(
                "{}: its image is built for a board and its system.toml states no entities, so \
                 it would be counted by its census -- but it has no `[[component]]` and its \
                 CMakeLists.txt builds no program, so there is nothing a census could run. \
                 (Looked for: {PROGRAM_SHAPES}.) Either add the `[[component]]` with \
                 `entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"]`, or write \
                 `[system] entities = \"defaults\"` (the crate defaults, written down).",
                dir.display()
            )
        })
    }
}

/// Is `dir` a C/C++ leaf: a `CMakeLists.txt` and no `Cargo.toml`? A Rust
/// Zephyr leaf carries both, and the probe counts it.
pub fn is_cmake_leaf(dir: &Path) -> bool {
    dir.join("CMakeLists.txt").is_file() && !dir.join("Cargo.toml").is_file()
}

/// Does `dir`'s `CMakeLists.txt` build a program a census could run? Any of:
///
/// - `nano_ros_add_executable(` or a plain `add_executable(`;
/// - `nros_components_register_node(... EXECUTABLE ...)` -- a component with
///   its own `main` (without `EXECUTABLE` it is a plugin, and runs nowhere);
/// - an `add_subdirectory(` of the leaf's own sources (a ported ament
///   workspace, whose packages add their executables and which the host
///   build's program manifest names, issue 2043) -- but NOT the nano-ros tree
///   itself (`add_subdirectory("${NROS_REPO_DIR}" nano_ros)`, the host
///   template's line), which adds the runtime and no program.
///
/// Comments do not count; command names match case-insensitively, as CMake's
/// do, and a call may span lines.
pub fn has_program(dir: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(dir.join("CMakeLists.txt")) else {
        return false;
    };
    cmake_calls(&raw)
        .iter()
        .any(|(name, args)| match name.as_str() {
            "nano_ros_add_executable" | "add_executable" => true,
            "nros_components_register_node" => args.iter().any(|a| a == "EXECUTABLE"),
            "add_subdirectory" => !adds_the_nano_ros_tree(args),
            _ => false,
        })
}

/// What [`has_program`] looks for, for the refusal that found none.
pub const PROGRAM_SHAPES: &str = "`nano_ros_add_executable(`, `add_executable(`, \
     `nros_components_register_node(... EXECUTABLE ...)`, or an `add_subdirectory(` of the \
     leaf's own sources (not `add_subdirectory(\"${NROS_REPO_DIR}\" nano_ros)`, which adds the \
     nano-ros runtime and no program)";

fn adds_the_nano_ros_tree(args: &[String]) -> bool {
    args.first()
        .is_some_and(|src| src.contains("${NROS_REPO_DIR}"))
        || args.get(1).is_some_and(|bin| bin == "nano_ros")
}

/// Every command call in a CMake listing, as (lower-cased name, arguments
/// with their quotes stripped). Enough of CMake's grammar for
/// [`has_program`]: `#` comments, quoted arguments, nested parentheses.
fn cmake_calls(raw: &str) -> Vec<(String, Vec<String>)> {
    let code: String = raw
        .lines()
        .map(|l| {
            // A `#` inside a quoted argument is not a comment.
            let mut quoted = false;
            let end = l
                .char_indices()
                .find(|&(_, c)| {
                    if c == '"' {
                        quoted = !quoted;
                    }
                    c == '#' && !quoted
                })
                .map_or(l.len(), |(i, _)| i);
            &l[..end]
        })
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = code.as_bytes();
    let mut calls = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !(c.is_ascii_alphabetic() || c == b'_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let name = code[start..i].to_ascii_lowercase();
        let mut j = i;
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            continue;
        }
        // Collect to the matching `)`, splitting on whitespace outside quotes.
        let (mut depth, mut quoted, mut args, mut cur) = (0usize, false, Vec::new(), String::new());
        let mut k = j + 1;
        while k < bytes.len() {
            let ch = code[k..].chars().next().unwrap_or(' ');
            match ch {
                '"' => quoted = !quoted,
                '(' if !quoted => {
                    depth += 1;
                    cur.push(ch);
                }
                ')' if !quoted && depth == 0 => break,
                ')' if !quoted => {
                    depth -= 1;
                    cur.push(ch);
                }
                c if c.is_whitespace() && !quoted => {
                    if !cur.is_empty() {
                        args.push(std::mem::take(&mut cur));
                    }
                }
                c => cur.push(c),
            }
            k += ch.len_utf8();
        }
        if !cur.is_empty() {
            args.push(cur);
        }
        calls.push((name, args));
        i = k + 1;
    }
    calls
}

/// [`entity_source`] with the host question answered by the caller:
/// `is_host(board)` is `Some(true)` for a board whose platform is the host's,
/// `Some(false)` for a cross board, `None` when the board resolves to nothing
/// (counted as cross: an unknown board is no reason to be loose).
pub fn entity_source_with(
    dir: &Path,
    system: &LeafSystem,
    is_host: &dyn Fn(&str) -> Option<bool>,
) -> EntitySource {
    if system.entities_from_census() {
        return EntitySource::Census { explicit: true };
    }
    if system.components.iter().any(|c| c.entities.is_some()) {
        return EntitySource::Declared;
    }
    if system.entities_defaults() {
        return EntitySource::Defaults;
    }
    if !is_cmake_leaf(dir) {
        return EntitySource::Unstated;
    }
    if system.board.as_deref().and_then(is_host).unwrap_or(false) {
        return EntitySource::Unstated;
    }
    if system.components.is_empty() && !has_program(dir) {
        return EntitySource::NoProgram;
    }
    EntitySource::Census { explicit: false }
}

/// [`entity_source_with`] for the leaf at `dir`, the host question answered
/// from the board catalog of the nano-ros checkout `dir` resolves.
pub fn entity_source(dir: &Path, system: &LeafSystem) -> EntitySource {
    use crate::orchestration::board_descriptor::PlatformKind;
    // Only a leaf the default could reach pays for the catalog.
    if system.states_entities() || !is_cmake_leaf(dir) {
        return entity_source_with(dir, system, &|_| None);
    }
    let catalog = crate::orchestration::nano_ros_root::resolve(None, dir)
        .and_then(|root| crate::cmd::leaf_system::load_catalog(&root, dir));
    let is_host = |board: &str| -> Option<bool> {
        let catalog = catalog.as_ref()?;
        crate::cmd::board_facts::resolve_board(catalog, board)
            .map(|d| d.platform == PlatformKind::Posix)
    };
    entity_source_with(dir, system, &is_host)
}

/// The leaf at `dir`, read for the SELECTED image (`None`: the leaf's own
/// rule -- one image, or `[system] default_images`), and classified. The ONE
/// image-aware read every leaf reader goes through. `Ok(None)` when `dir`
/// declares no deployment.
pub fn read(dir: &Path, image: Option<&str>) -> eyre::Result<Option<(LeafSystem, EntitySource)>> {
    Ok(read_system(dir, image)?.map(|s| {
        let source = entity_source(dir, &s);
        (s, source)
    }))
}

/// [`read`] with the host question injected (tests).
pub fn read_with(
    dir: &Path,
    image: Option<&str>,
    is_host: &dyn Fn(&str) -> Option<bool>,
) -> eyre::Result<Option<(LeafSystem, EntitySource)>> {
    Ok(read_system(dir, image)?.map(|s| {
        let source = entity_source_with(dir, &s, is_host);
        (s, source)
    }))
}

fn read_system(dir: &Path, image: Option<&str>) -> eyre::Result<Option<LeafSystem>> {
    match image {
        Some(id) => nros_orchestration_ir::leaf_system::read_image(dir, id),
        None => nros_orchestration_ir::leaf_system::read(dir),
    }
    .map_err(|e| eyre::eyre!(e))
}

/// [`read`], the classification alone.
pub fn entity_source_of(dir: &Path, image: Option<&str>) -> eyre::Result<Option<EntitySource>> {
    Ok(read(dir, image)?.map(|(_, source)| source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (name, body) in files {
            std::fs::write(d.path().join(name), body).unwrap();
        }
        d
    }

    const HEAD: &str = "[system]\nname = \"t\"\n[image.a]\nboard = \"zephyr\"\n";

    fn classify(files: &[(&str, &str)], host: Option<bool>) -> EntitySource {
        let d = leaf(files);
        let s = nros_orchestration_ir::leaf_system::read(d.path())
            .unwrap()
            .unwrap();
        entity_source_with(d.path(), &s, &|_| host)
    }

    fn system(component: &str) -> String {
        format!("{HEAD}[[component]]\nname = \"n\"\n{component}")
    }

    #[test]
    fn a_cpp_cross_leaf_that_states_nothing_is_counted_by_its_census() {
        let s = system("");
        assert_eq!(
            classify(&[("CMakeLists.txt", ""), ("system.toml", &s)], Some(false)),
            EntitySource::Census { explicit: false }
        );
        // An unknown board is no reason to be loose.
        assert_eq!(
            classify(&[("CMakeLists.txt", ""), ("system.toml", &s)], None),
            EntitySource::Census { explicit: false }
        );
    }

    #[test]
    fn what_a_leaf_writes_wins_over_the_default() {
        let cm = ("CMakeLists.txt", "");
        let declared = system("entities = [\"timer\"]\n");
        assert_eq!(
            classify(&[cm, ("system.toml", &declared)], Some(false)),
            EntitySource::Declared
        );
        let census = system("entities = \"census\"\n");
        // Written, so it holds even for a host board, as it did before.
        assert_eq!(
            classify(&[cm, ("system.toml", &census)], Some(true)),
            EntitySource::Census { explicit: true }
        );
        let defaults = system("entities = \"defaults\"\n");
        assert_eq!(
            classify(&[cm, ("system.toml", &defaults)], Some(false)),
            EntitySource::Defaults
        );
    }

    #[test]
    fn rust_and_host_leaves_keep_no_default() {
        let s = system("");
        // A Rust leaf: the probe counts it.
        assert_eq!(
            classify(
                &[
                    ("CMakeLists.txt", ""),
                    ("Cargo.toml", "[package]\nname = \"t\"\n"),
                    ("system.toml", &s)
                ],
                Some(false)
            ),
            EntitySource::Unstated
        );
        // A host image: the census producer's own platform.
        assert_eq!(
            classify(&[("CMakeLists.txt", ""), ("system.toml", &s)], Some(true)),
            EntitySource::Unstated
        );
    }

    /// Ruling R29 -- a component-less cross C/C++ leaf that builds a program
    /// (a port naming its executable in CMake, or a ported workspace adding
    /// its packages) is counted by its census like any other.
    #[test]
    fn component_less_cross_leaf_with_a_program_takes_the_census() {
        let exe = "project(p C)\nnano_ros_add_executable(minimal_publisher src/main.c ROS2_MAIN)\n";
        assert_eq!(
            classify(
                &[("CMakeLists.txt", exe), ("system.toml", HEAD)],
                Some(false)
            ),
            EntitySource::Census { explicit: false }
        );
        let ws = "project(p C)\nadd_subdirectory(../src/nodes nodes)\n";
        assert_eq!(
            classify(
                &[("CMakeLists.txt", ws), ("system.toml", HEAD)],
                Some(false)
            ),
            EntitySource::Census { explicit: false }
        );
        // A host image keeps the hosted defaults, component or not.
        assert_eq!(
            classify(
                &[("CMakeLists.txt", exe), ("system.toml", HEAD)],
                Some(true)
            ),
            EntitySource::Unstated
        );
    }

    /// Ruling R29 -- with no component and no program there is nothing a
    /// census could run, and nothing stated: refused, never silently loose.
    #[test]
    fn component_less_leaf_with_no_program_refuses() {
        let none = "project(p C)\n# nano_ros_add_executable(x) in a comment is not a program\n";
        let got = classify(
            &[("CMakeLists.txt", none), ("system.toml", HEAD)],
            Some(false),
        );
        assert_eq!(got, EntitySource::NoProgram);
        let why = got.refusal(Path::new("/l")).expect("a refusal");
        assert!(
            why.contains("[system] entities = \"defaults\"") && why.contains("[[component]]"),
            "{why}"
        );
    }

    /// Fix round 2 -- the host template's `add_subdirectory` of the nano-ros
    /// tree itself adds the runtime, not a program: a leaf with only that
    /// is refused by name, never sent to a census that finds 0 executables.
    #[test]
    fn adding_the_nano_ros_tree_is_not_a_program() {
        for cm in [
            "project(p C)\nadd_subdirectory(\"${NROS_REPO_DIR}\" nano_ros)\n",
            "project(p C)\nadd_subdirectory(${NROS_REPO_DIR})\n",
            "project(p C)\nadd_subdirectory(../../.. nano_ros)\n",
            "project(p C)\nADD_SUBDIRECTORY(\n  \"${NROS_REPO_DIR}\"\n  nano_ros\n)\n",
        ] {
            assert_eq!(
                classify(
                    &[("CMakeLists.txt", cm), ("system.toml", HEAD)],
                    Some(false)
                ),
                EntitySource::NoProgram,
                "{cm}"
            );
        }
        // The same tree beside a real program is still a program.
        let both = "add_subdirectory(\"${NROS_REPO_DIR}\" nano_ros)\nadd_subdirectory(src/nodes)\n";
        assert_eq!(
            classify(
                &[("CMakeLists.txt", both), ("system.toml", HEAD)],
                Some(false)
            ),
            EntitySource::Census { explicit: false }
        );
    }

    /// Fix round 2 -- a plain `add_executable`, and a component registered
    /// with its own EXECUTABLE, are programs too.
    #[test]
    fn plain_executables_and_registered_executables_are_programs() {
        for cm in [
            "project(p C)\nadd_executable(talker src/main.c)\n",
            "project(p C)\nnros_components_register_node(talker\n  PLUGIN p::Talker\n  EXECUTABLE talker_node)\n",
        ] {
            assert_eq!(
                classify(
                    &[("CMakeLists.txt", cm), ("system.toml", HEAD)],
                    Some(false)
                ),
                EntitySource::Census { explicit: false },
                "{cm}"
            );
        }
        // Registered with no EXECUTABLE: a plugin, nothing a census runs.
        let plugin = "nros_components_register_node(talker PLUGIN p::Talker)\n";
        assert_eq!(
            classify(
                &[("CMakeLists.txt", plugin), ("system.toml", HEAD)],
                Some(false)
            ),
            EntitySource::NoProgram
        );
    }

    /// Fix round 2 -- the refusal says what it looked for.
    #[test]
    fn the_no_program_refusal_names_what_it_looked_for() {
        let why = EntitySource::NoProgram.refusal(Path::new("/l")).unwrap();
        for looked in [
            "nano_ros_add_executable",
            "add_executable",
            "nros_components_register_node(... EXECUTABLE ...)",
            "NROS_REPO_DIR",
        ] {
            assert!(why.contains(looked), "{looked} missing from: {why}");
        }
    }

    /// Ruling R29 -- `[system] entities = "defaults"` opts a component-less
    /// leaf out.
    #[test]
    fn system_level_defaults_opts_out() {
        let exe = "nano_ros_add_executable(minimal_publisher src/main.c ROS2_MAIN)\n";
        let body = HEAD.replace("[system]\n", "[system]\nentities = \"defaults\"\n");
        assert_eq!(
            classify(
                &[("CMakeLists.txt", exe), ("system.toml", &body)],
                Some(false)
            ),
            EntitySource::Defaults
        );
    }

    /// M1 -- the SELECTED image decides, not the default one: a leaf whose
    /// default image is the host's and which also declares a cross image is
    /// census-sized when the cross image is built.
    #[test]
    fn cross_image_of_a_host_default_leaf_takes_the_census() {
        let d = leaf(&[
            ("CMakeLists.txt", ""),
            (
                "system.toml",
                "[system]\nname = \"t\"\ndefault_images = [\"linux\"]\n\
                 [[component]]\nname = \"n\"\n\
                 [image.linux]\nboard = \"linux\"\n[image.rtos]\nboard = \"zephyr\"\n",
            ),
        ]);
        let host = |b: &str| Some(b == "linux");
        let read = |image: Option<&str>| read_with(d.path(), image, &host).unwrap().unwrap().1;
        assert_eq!(read(None), EntitySource::Unstated);
        assert_eq!(read(Some("linux")), EntitySource::Unstated);
        assert_eq!(read(Some("rtos")), EntitySource::Census { explicit: false });
    }
}

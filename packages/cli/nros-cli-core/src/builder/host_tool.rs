//! The host build role (RFC-0119 D3, phase-527 W2).
//!
//! A `runner` or `command` package is a host tool. `nros build` compiles it
//! for the host triple into `<ws>/build/nros/host/<package>/`, whatever board
//! the images beside it target, and with a build directory of its own: a host
//! tool shares no cargo target directory with an image (issue 0616, one
//! target directory per workspace root), and no image root lists it.
//!
//! The place is fixed. `--build-base` does not move it, because the build is
//! not this directory's only reader: `nros <verb>` (command dispatch) and
//! `nros run` find the executable here, from the workspace alone, and a build
//! that could move it would leave them looking in the wrong place.
//!
//! This module holds the two facts both sides need, so neither restates them:
//! where a host tool builds ([`plan`]) and where its executable lands
//! ([`executable`]).

use std::path::{Path, PathBuf};

use super::{handoff::Handoff, plan::Driver};

/// Where host tools build, relative to the workspace root.
pub const HOST_BUILD_DIR: &str = "build/nros/host";

/// The cargo profile a host tool builds with. Release, as colcon's cargo
/// support installs a package: a runner pumps a console for as long as the
/// target runs, so it is built once and run many times.
pub const PROFILE: &str = "release";

/// The build directory of one host-tool package.
#[must_use]
pub fn build_dir(workspace: &Path, package: &str) -> PathBuf {
    workspace.join(HOST_BUILD_DIR).join(package)
}

/// The commands that build one host tool.
#[derive(Debug, Clone)]
pub struct HostBuild {
    /// `cargo` or `cmake`.
    pub driver: Driver,
    /// A step that must run first: a lock resolve for a cargo package with no
    /// lock of its own, the configure for a cmake package.
    pub configure: Option<Handoff>,
    /// The build.
    pub handoff: Handoff,
    /// [`build_dir`].
    pub build_dir: PathBuf,
}

/// What [`plan`] reads besides the package.
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    /// The host's rustc triple. Passed to cargo explicitly, so a
    /// `[build] target` in a `.cargo/config.toml` above the package (an
    /// embedded workspace's usual setting) cannot turn a host tool into a
    /// cross build.
    pub host_triple: &'a str,
    /// `--offline`: cargo gets `--frozen`, and the lock resolve `--offline`.
    pub offline: bool,
    /// Whether the package has a `Cargo.lock` of its own already.
    pub has_lock: bool,
    /// `--cmake-args`, for a cmake host tool.
    pub cmake_args: &'a [String],
    /// The `nros-extension` crate this nano-ros tree carries, for a cargo
    /// host tool that depends on it by registry name ([`EXTENSION_CRATE`]):
    /// both cargo commands get a `[patch.crates-io]` row pointing there, as
    /// an image's generated configuration patches the in-repo nros crates
    /// (phase-527 W7). `None` adds no row, so a package that does not name the
    /// crate never sees an unused patch.
    pub extension_crate: Option<&'a Path>,
}

/// The crate a runner or command package is written against (RFC-0119 D5).
/// Not on crates.io yet, so a package names it by registry name and `nros
/// build` patches it to the tree's copy.
pub const EXTENSION_CRATE: &str = "nros-extension";

/// Whether the cargo package at `package_dir` names [`EXTENSION_CRATE`] as a
/// dependency, read from its own `Cargo.toml` (any dependency table).
#[must_use]
pub fn depends_on_extension(package_dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(package_dir.join("Cargo.toml")) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return false;
    };
    let names = |t: Option<&toml::Value>| {
        t.and_then(toml::Value::as_table)
            .is_some_and(|t| t.contains_key(EXTENSION_CRATE))
    };
    ["dependencies", "build-dependencies", "dev-dependencies"]
        .iter()
        .any(|k| names(doc.get(*k)))
}

/// `--config` arguments carrying the patch row for [`EXTENSION_CRATE`].
fn extension_patch(path: Option<&Path>) -> Vec<String> {
    path.map(|p| {
        vec![
            "--config".to_string(),
            format!(
                "patch.crates-io.{EXTENSION_CRATE}.path={}",
                toml::Value::String(p.display().to_string())
            ),
        ]
    })
    .unwrap_or_default()
}

/// Plan the build of the host tool at `package_dir`, by the road its routing
/// names. `None` when the package carries no build file either road builds.
#[must_use]
pub fn plan(
    workspace: &Path,
    package: &str,
    package_dir: &Path,
    routing: crate::routing::Routing,
    o: Options<'_>,
) -> Option<HostBuild> {
    let dir = build_dir(workspace, package);
    if routing.cargo_member {
        let mut a = vec![
            "build".to_string(),
            format!("--{PROFILE}"),
            "--target".to_string(),
            o.host_triple.to_string(),
            "--target-dir".to_string(),
            dir.join("target").display().to_string(),
        ];
        if o.offline {
            a.push("--frozen".to_string());
        }
        a.extend(extension_patch(o.extension_crate));
        // A package with no lock of its own gets one resolved first, as a
        // package-mode cargo package does: the PATH shim's project-wide
        // `--locked` cannot create one.
        let configure = (!o.has_lock).then(|| {
            let mut u = vec!["update".to_string(), "--workspace".to_string()];
            if o.offline {
                u.push("--offline".to_string());
            }
            u.extend(extension_patch(o.extension_crate));
            Handoff::new("cargo", u).in_dir(package_dir)
        });
        return Some(HostBuild {
            driver: Driver::Cargo,
            configure,
            handoff: Handoff::new("cargo", a).in_dir(package_dir),
            build_dir: dir,
        });
    }
    if routing.cmake_subdir {
        // The package's own CMakeLists, not a generated root: a host tool
        // links nothing an image links, so it needs no nano-ros root and no
        // board toolchain file.
        let cmake = dir.join("cmake");
        let mut c = vec![
            "-S".to_string(),
            package_dir.display().to_string(),
            "-B".to_string(),
            cmake.display().to_string(),
            "-DCMAKE_BUILD_TYPE=Release".to_string(),
        ];
        c.extend(o.cmake_args.iter().cloned());
        return Some(HostBuild {
            driver: Driver::CMake,
            configure: Some(Handoff::new("cmake", c).in_dir(workspace)),
            handoff: Handoff::new(
                "cmake",
                vec!["--build".to_string(), cmake.display().to_string()],
            )
            .in_dir(workspace),
            build_dir: dir,
        });
    }
    None
}

/// Where each road puts the executable `bin` of `package`, cargo's first.
#[must_use]
pub fn candidates(workspace: &Path, package: &str, bin: &str, host_triple: &str) -> Vec<PathBuf> {
    let dir = build_dir(workspace, package);
    vec![
        dir.join("target").join(host_triple).join(PROFILE).join(bin),
        dir.join("cmake").join(bin),
    ]
}

/// A host tool that has not been built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotBuilt {
    /// The package that provides the tool.
    pub package: String,
    /// What the caller wanted it for, in the caller's words: `nros router`,
    /// `runner qemu`.
    pub wanted_for: String,
    /// Where it was looked for.
    pub looked: Vec<PathBuf>,
}

impl std::fmt::Display for NotBuilt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is provided by package `{}`, which is not built (looked for {}). \
             Build it with:  nros build --packages-select {}",
            self.wanted_for,
            self.package,
            self.looked
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.package
        )
    }
}

impl std::error::Error for NotBuilt {}

/// The built executable of a host tool, or a refusal naming the command that
/// builds it.
///
/// `wanted_for` names what the caller was doing (`nros router`) so the refusal
/// reads as an answer to it. The refusal is RFC-0119 D3's: an unbuilt command
/// package is never built implicitly by the verb that reaches it.
///
/// # Errors
///
/// [`NotBuilt`], when no road's output holds `bin`.
pub fn executable(
    workspace: &Path,
    package: &str,
    bin: &str,
    host_triple: &str,
    wanted_for: &str,
) -> Result<PathBuf, NotBuilt> {
    let looked = candidates(workspace, package, bin, host_triple);
    match looked.iter().find(|p| p.is_file()) {
        Some(p) => Ok(p.clone()),
        None => Err(NotBuilt {
            package: package.to_string(),
            wanted_for: wanted_for.to_string(),
            looked,
        }),
    }
}

/// The built executable of a resolved host tool, wherever its provider sits:
/// the one location ladder that runner lookup (`nros run`/`flash`/`monitor`)
/// and command dispatch (`nros <verb>`, phase-527 W4) share.
///
/// - A package of the workspace `ws`: [`executable`], which `nros build`
///   filled (RFC-0119 D3); an unbuilt one is refused naming the build.
/// - A nano-ros checkout's own host tool (a runner under `packages/runners/`, a
///   command package beside its domain): built beside the CLI by `just
///   setup-cli`, into the target directory of the cargo workspace it belongs
///   to ([`tree_target_dir`]).
/// - Otherwise a prebuilt package's own `bin/<bin>`: a released toolchain's
///   `share/nros/packages/<package>/` (phase-527 W6), or a user's overlay
///   package placed the same way.
///
/// `dir` is the provider's package directory and `wanted_for` names what the
/// caller wanted (`runner \`qemu\``, `` `nros router` ``), so every refusal
/// reads as an answer to it.
///
/// # Errors
///
/// A message naming where the executable was looked for and how to build it.
pub fn locate_built(
    ws: &Path,
    nano_ros_root: Option<&Path>,
    package: &str,
    dir: &Path,
    bin: &str,
    wanted_for: &str,
) -> Result<PathBuf, String> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let ws = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    let root = nano_ros_root.and_then(|p| p.canonicalize().ok());
    // A package of the nano-ros tree, reached from a directory that holds the
    // whole tree (the checkout's root, say): the checkout's own build answers,
    // not a workspace host build nobody ran.
    let in_tree = root
        .as_ref()
        .is_some_and(|r| dir.starts_with(r) && r.starts_with(&ws));
    // A package of this workspace: `nros build` built it for the host.
    if dir.starts_with(&ws) && !in_tree {
        let triple = crate::orchestration::metadata_build::host_triple();
        return executable(&ws, package, bin, &triple, wanted_for).map_err(|e| e.to_string());
    }
    // A nano-ros checkout's own host tool: built beside the CLI.
    if let Some(root) = root
        && let Some(target) = tree_target_dir(&root, &dir)
    {
        let exe = target.join("release").join(bin);
        return if exe.is_file() {
            Ok(exe)
        } else {
            Err(format!(
                "{wanted_for} (package `{package}`) is not built (looked for {}). Build it with \
                 `just setup-cli` in {}",
                exe.display(),
                root.display()
            ))
        };
    }
    // A prebuilt package ships its executable in its own `bin/`: a released
    // toolchain's `share/nros/packages/<package>/` (phase-527 W6), or a
    // user's overlay package placed the same way.
    let exe = dir.join("bin").join(bin);
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(format!(
            "{wanted_for} (package `{package}` at {}) has no built executable `bin/{bin}`",
            dir.display(),
        ))
    }
}

/// Where `just setup-cli` puts the binaries of the in-tree host tool at
/// `dir`: `<workspace root>/target`, the workspace root being the nearest
/// directory from `dir` up to the nano-ros `root` (exclusive) whose
/// `Cargo.toml` declares `[workspace]`. `packages/runners/` is one for every
/// runner; a command package is its own. `None` when `dir` is outside `root`
/// or no such manifest stands between them.
#[must_use]
pub fn tree_target_dir(root: &Path, dir: &Path) -> Option<PathBuf> {
    if !dir.starts_with(root) {
        return None;
    }
    dir.ancestors()
        .take_while(|d| *d != root)
        .find(|d| {
            std::fs::read_to_string(d.join("Cargo.toml"))
                .is_ok_and(|t| t.lines().any(|l| l.trim() == "[workspace]"))
        })
        .map(|d| d.join("target"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::Routing;

    const TRIPLE: &str = "x86_64-unknown-linux-gnu";

    fn opts(has_lock: bool) -> Options<'static> {
        Options {
            host_triple: TRIPLE,
            offline: false,
            has_lock,
            cmake_args: &[],
            extension_crate: None,
        }
    }

    #[test]
    fn a_tool_naming_nros_extension_gets_its_patch_and_only_then() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path();
        std::fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname = \"acme\"\n\n[dependencies]\nnros-extension = { version = \"*\" }\n",
        )
        .unwrap();
        assert!(depends_on_extension(pkg));
        let ext = Path::new("/sdk/packages/runners/nros-extension");
        let b = plan(
            Path::new("/ws"),
            "acme",
            pkg,
            Routing {
                cargo_member: true,
                cmake_subdir: false,
            },
            Options {
                extension_crate: Some(ext),
                ..opts(false)
            },
        )
        .unwrap();
        let want = "patch.crates-io.nros-extension.path=\"/sdk/packages/runners/nros-extension\"";
        assert!(
            b.handoff.display().contains(want),
            "{}",
            b.handoff.display()
        );
        assert!(b.configure.unwrap().display().contains(want));

        std::fs::write(pkg.join("Cargo.toml"), "[package]\nname = \"acme\"\n").unwrap();
        assert!(!depends_on_extension(pkg));
    }

    #[test]
    fn a_cargo_host_tool_builds_for_the_host_into_its_own_dir() {
        let ws = Path::new("/ws");
        let b = plan(
            ws,
            "acme_runner",
            Path::new("/ws/src/acme_runner"),
            Routing {
                cargo_member: true,
                cmake_subdir: false,
            },
            opts(true),
        )
        .expect("cargo road");
        assert_eq!(b.driver, Driver::Cargo);
        let shown = b.handoff.display();
        assert!(
            shown.contains(&format!("--target {TRIPLE}")),
            "the host triple is explicit: {shown}"
        );
        assert!(
            shown.contains("--target-dir /ws/build/nros/host/acme_runner/target"),
            "{shown}"
        );
        assert!(shown.contains("--release"), "{shown}");
        assert!(b.configure.is_none(), "a package with a lock resolves none");
        let b = plan(
            ws,
            "acme_runner",
            Path::new("/ws/src/acme_runner"),
            Routing {
                cargo_member: true,
                cmake_subdir: false,
            },
            opts(false),
        )
        .unwrap();
        assert!(
            b.configure
                .unwrap()
                .display()
                .contains("cargo update --workspace")
        );
    }

    #[test]
    fn a_cmake_host_tool_configures_its_own_cmakelists() {
        let b = plan(
            Path::new("/ws"),
            "acme_runner",
            Path::new("/ws/src/acme_runner"),
            Routing {
                cargo_member: false,
                cmake_subdir: true,
            },
            opts(true),
        )
        .expect("cmake road");
        assert_eq!(b.driver, Driver::CMake);
        let cfg = b.configure.unwrap().display();
        assert!(
            cfg.contains("-S /ws/src/acme_runner -B /ws/build/nros/host/acme_runner/cmake"),
            "{cfg}"
        );
        assert!(
            plan(
                Path::new("/ws"),
                "x",
                Path::new("/ws/src/x"),
                Routing::default(),
                opts(true)
            )
            .is_none()
        );
    }

    #[test]
    fn a_tree_package_reached_from_the_tree_root_is_the_checkout_s_build() {
        // The workspace is the checkout itself (`nros run` from its root): the
        // runner is the tree's, built by `just setup-cli`, and never a
        // workspace host build.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("packages/runners/nros_runner_x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            root.path().join("packages/runners/Cargo.toml"),
            "[workspace]\nmembers = [\"nros_runner_x\"]\n",
        )
        .unwrap();
        let locate = || {
            locate_built(
                root.path(),
                Some(root.path()),
                "nros_runner_x",
                &dir,
                "nros-runner-x-not-built",
                "runner `x`",
            )
        };
        let e = locate().expect_err("nothing built");
        // The checkout's own build, never `nros build`'s workspace one.
        assert!(e.contains("packages/runners/target/release"), "{e}");
        assert!(!e.contains("--packages-select"), "{e}");
        let exe = root
            .path()
            .join("packages/runners/target/release/nros-runner-x-not-built");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        assert_eq!(locate().unwrap(), exe.canonicalize().unwrap());
    }

    #[test]
    fn a_tree_host_tool_builds_into_its_own_workspace_target() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        let runner = r.join("packages/runners/nros_runner_x");
        let tool = r.join("packages/rmw/zenoh/nros_zenoh_tools");
        for d in [&runner, &tool] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(r.join("packages/runners/Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(runner.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::write(
            tool.join("Cargo.toml"),
            "[package]\nname = \"t\"\n\n[workspace]\n",
        )
        .unwrap();
        assert_eq!(
            tree_target_dir(r, &runner),
            Some(r.join("packages/runners/target"))
        );
        assert_eq!(tree_target_dir(r, &tool), Some(tool.join("target")));
        assert_eq!(tree_target_dir(r, Path::new("/elsewhere")), None);
    }

    #[test]
    fn an_unbuilt_tool_is_refused_naming_the_build_command() {
        let tmp = tempfile::tempdir().unwrap();
        let e = executable(tmp.path(), "acme_tools", "acme", TRIPLE, "`nros acme`")
            .expect_err("nothing built");
        let msg = e.to_string();
        assert!(
            msg.contains("nros build --packages-select acme_tools"),
            "{msg}"
        );
        assert!(
            msg.starts_with("`nros acme` is provided by package `acme_tools`"),
            "{msg}"
        );

        let built = candidates(tmp.path(), "acme_tools", "acme", TRIPLE)[0].clone();
        std::fs::create_dir_all(built.parent().unwrap()).unwrap();
        std::fs::write(&built, "").unwrap();
        assert_eq!(
            executable(tmp.path(), "acme_tools", "acme", TRIPLE, "`nros acme`").unwrap(),
            built
        );
    }
}

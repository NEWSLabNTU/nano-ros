//! Skip a cmake configure that cannot change anything (phase-489 W6).
//!
//! `cmake -S <src> -B <dir>` is not make-shaped: invoked by hand it ALWAYS
//! runs the whole configure and generate steps, whether or not anything moved.
//! `nros build` runs one per image, and a workspace's images share a build dir
//! per board/RMW, so a no-change re-run of a 20-row C workspace paid 20
//! configures (5-30 s each) to learn nothing. Measured: 20 of 20 rows printed
//! `-- Configuring done` on a re-run with an unchanged tree.
//!
//! The bash road (`nros_cmake_configure_if_needed`) already answered this: keep
//! a stamp of the arguments, configure when it differs or the dir is not
//! configured, and leave the rest to `cmake --build`, which re-runs the
//! configure itself when a registered input moved (`CMakeLists.txt`,
//! `CMAKE_CONFIGURE_DEPENDS` — the codegen tool's, issue 1018). This is that
//! rule for the three configure sites `nros build` plans (workspace, single
//! leaf, cmake member), reached through the one [`ConfigureStamp`].
//!
//! What the stamp must name is everything a configure READS that cmake's own
//! dependency tracking cannot see: the `-D` arguments (cmake never compares
//! them with the cache before a build) and the process environment the
//! generated project consults. Anything else is an input FILE, and an input
//! file is `cmake --build`'s job.
//!
//! `NROS_CONFIGURED_PROJECT` / `NROS_CONFIGURED_IMAGE` are left out on
//! purpose (phase-530 W4; the retired `NROS_RESOLVED_DIR` before them). They
//! name WHICH image's configured answer the configure seeds from, so they
//! differ for every image sharing the dir, while the answer itself is a file
//! the configure registers as a dependency — counting them re-ran the
//! configure for every row.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use super::handoff::Handoff;

/// File in the build dir holding the stamp of the last configure `nros build`
/// ran there.
pub const STAMP_NAME: &str = ".nros-build-configure.stamp";

/// Cache variables that are inert on a configured dir (see the module doc).
const SEED_ONLY: &[&str] = &["NROS_CONFIGURED_PROJECT", "NROS_CONFIGURED_IMAGE"];

/// Environment a configure may read. By PREFIX, so a new knob is covered
/// without anyone remembering to list it; the ambient noise that varies per
/// process (`PWD`, `SHLVL`, `MAKEFLAGS`, job-server fds, `_`) matches none.
const ENV_PREFIXES: &[&str] = &[
    "NROS_",
    "NANO_ROS_",
    "nano_ros_",
    "ZPICO_",
    "XRCE_",
    "CMAKE_",
    "AMENT_",
    "COLCON_",
    "ROS_",
    "RMW_",
    "CYCLONEDDS_",
    "FASTRTPS_",
    "FASTDDS_",
    "CARGO_",
    "RUSTFLAGS",
    "RUSTC",
    "CC",
    "CXX",
    "CFLAGS",
    "CXXFLAGS",
    "LDFLAGS",
    "FREERTOS_",
    "THREADX_",
    "NUTTX_",
    "ZEPHYR_",
    "LD_LIBRARY_PATH",
];

/// Env names that match a prefix and still vary run to run without meaning
/// anything to a configure (a per-process jobserver or tmp path).
const ENV_NOISE: &[&str] = &["CARGO_MAKEFLAGS", "CARGO_PKG_VERSION"];

/// What a `cmake -S -B` handoff configures, as far as skipping is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureStamp {
    build_dir: PathBuf,
    body: String,
    /// The handoff's own environment, which the configure ran with and a
    /// re-ask of the locate ladder must run with too.
    handoff_env: Vec<(OsString, OsString)>,
}

impl ConfigureStamp {
    /// The stamp for `cfg`, or `None` when `cfg` is not a plain
    /// `cmake -S <src> -B <dir> ...` configure (then it always runs).
    #[must_use]
    pub fn of(cfg: &Handoff) -> Option<Self> {
        Self::of_with_env(cfg, std::env::vars_os())
    }

    /// [`Self::of`] over an explicit environment, so a test names the
    /// environment it means instead of mutating the process's.
    pub fn of_with_env<I>(cfg: &Handoff, env: I) -> Option<Self>
    where
        I: IntoIterator<Item = (OsString, OsString)>,
    {
        if cfg.program != "cmake" {
            return None;
        }
        let args: Vec<String> = cfg
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if args.iter().any(|a| a == "--build") {
            return None;
        }
        let value_of = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
        };
        let (src, bdir) = (value_of("-S")?, value_of("-B")?);
        let base = cfg.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
        let build_dir = base.join(bdir);

        let mut body = String::new();
        body.push_str("nros-build-configure-stamp v1\n");
        body.push_str(&format!("base={}\n", abs(&base).display()));
        body.push_str(&format!("src={src}\nbuild={bdir}\n"));
        for a in &args {
            if let Some(def) = a.strip_prefix("-D") {
                let name = def.split(['=', ':']).next().unwrap_or("");
                if SEED_ONLY.contains(&name) {
                    continue;
                }
            }
            body.push_str(&format!("arg={a}\n"));
        }
        for (k, v) in &cfg.env {
            body.push_str(&format!(
                "handoff-env {}={}\n",
                k.to_string_lossy(),
                v.to_string_lossy()
            ));
        }
        let mut vars: Vec<(String, String)> = env
            .into_iter()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .filter(|(k, _)| {
                ENV_PREFIXES.iter().any(|p| k.starts_with(p)) && !ENV_NOISE.contains(&k.as_str())
            })
            .collect();
        vars.sort();
        for (k, v) in vars {
            body.push_str(&format!("env {k}={v}\n"));
        }
        Some(Self {
            build_dir,
            body,
            handoff_env: cfg.env.clone(),
        })
    }

    /// The build dir this stamp belongs to.
    #[must_use]
    pub fn build_dir(&self) -> &Path {
        &self.build_dir
    }

    fn path(&self) -> PathBuf {
        self.build_dir.join(STAMP_NAME)
    }

    /// True when the configure may be skipped: the dir holds a generated build
    /// system AND the last configure `nros build` ran there had this stamp.
    /// Everything else (a file the configure reads) is `cmake --build`'s check,
    /// except the located source trees: see [`locate_answers_current`].
    #[must_use]
    pub fn is_current(&self) -> bool {
        let d = &self.build_dir;
        d.join("CMakeCache.txt").is_file()
            && (d.join("build.ninja").is_file() || d.join("Makefile").is_file())
            && std::fs::read_to_string(self.path()).is_ok_and(|s| s == self.body)
            && locate_answers_current(d, &self.handoff_env)
    }

    /// Record that a configure with this stamp just succeeded.
    ///
    /// Written only on change, and a failure to write is not a build failure:
    /// the next run simply configures again.
    pub fn record(&self) {
        if std::fs::read_to_string(self.path()).is_ok_and(|s| s == self.body) {
            return;
        }
        let _ = std::fs::write(self.path(), &self.body);
    }

    /// Forget the stamp before a configure starts, so a configure that fails
    /// half-way cannot leave the previous run's stamp vouching for it.
    pub fn invalidate(&self) {
        let _ = std::fs::remove_file(self.path());
    }
}

/// The stamp `NanoRosLocate.cmake` writes beside the cache: the `nros` it
/// asked, the index and project it asked about, then the answers verbatim.
pub const LOCATE_STAMP_NAME: &str = "nros-locate.stamp";

/// Do the source trees this build dir's configure located still locate there?
///
/// The configure registers every input FILE the ladder read, so `cmake
/// --build` re-configures when one moves. Two inputs have no file: a row's
/// environment variable and a checkout's uncommitted edits (which move a
/// submodule row from the store rung to the local-edit rung). So this
/// re-asks the same ladder, with the same index, project and handoff
/// environment, and compares the answers. A dir whose configure asked no
/// ladder (no stamp) is current; a re-ask that cannot run is not, so the
/// error is toward a configure, never toward a stale tree.
#[must_use]
pub fn locate_answers_current(build_dir: &Path, handoff_env: &[(OsString, OsString)]) -> bool {
    // Issue 3019 -- the configure was skipped over a moved answer, and the
    // image was built from the tree located first.
    let Ok(stamp) = std::fs::read_to_string(build_dir.join(LOCATE_STAMP_NAME)) else {
        return true;
    };
    let Some((head, recorded)) = stamp.split_once("\n---\n") else {
        return false;
    };
    let field = |k: &str| {
        head.lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .map(str::to_string)
    };
    let (Some(nros), Some(index), Some(project)) =
        (field("nros"), field("index"), field("project"))
    else {
        return false;
    };
    let nros = if Path::new(&nros).is_file() {
        PathBuf::from(nros)
    } else {
        match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => return false,
        }
    };
    let mut cmd = std::process::Command::new(&nros);
    cmd.args(["locate", "--all", "--format", "cmake", "--index", &index]);
    if project.is_empty() {
        // The configure found no project; a re-ask must not find one either
        // from a working directory or a variable the configure did not see.
        cmd.current_dir("/").env_remove("NROS_WORKSPACE_ROOT");
    } else {
        cmd.args(["--project", &project]);
    }
    for (k, v) in handoff_env {
        cmd.env(k, v);
    }
    let Ok(out) = cmd.stderr(std::process::Stdio::null()).output() else {
        return false;
    };
    out.status.success() && answers(&String::from_utf8_lossy(&out.stdout)) == answers(recorded)
}

/// The located paths in a `nros locate --all --format cmake` output, without
/// the watch list and the project line (they say what was READ, not what was
/// located; a store directory appearing changes the first without moving
/// any answer).
fn answers(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.starts_with("set(NROS_LOCATE_"))
        .filter(|l| {
            !l.starts_with("set(NROS_LOCATE_INPUTS ") && !l.starts_with("set(NROS_LOCATE_PROJECT ")
        })
        .collect()
}

fn abs(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path, extra: &[&str]) -> Handoff {
        let mut a = vec!["-S", "src", "-B", "src/cmake", "-DNROS_RMW=zenoh"];
        a.extend(extra);
        Handoff::new("cmake", a).in_dir(dir)
    }

    fn stamp(dir: &Path, extra: &[&str], env: &[(&str, &str)]) -> ConfigureStamp {
        ConfigureStamp::of_with_env(
            &cfg(dir, extra),
            env.iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        )
        .expect("a cmake -S -B handoff")
    }

    /// A configured dir: a cache and a generated build system.
    fn configured(dir: &Path) {
        let b = dir.join("src/cmake");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("CMakeCache.txt"), "").unwrap();
        std::fs::write(b.join("build.ninja"), "").unwrap();
    }

    #[test]
    fn unconfigured_dir_is_never_current() {
        let t = tempfile::tempdir().unwrap();
        let s = stamp(t.path(), &[], &[]);
        assert!(!s.is_current(), "no build dir");
        configured(t.path());
        assert!(!s.is_current(), "configured but never stamped");
    }

    #[test]
    fn recorded_stamp_is_current_until_an_input_moves() {
        let t = tempfile::tempdir().unwrap();
        configured(t.path());
        let s = stamp(t.path(), &[], &[("NROS_X", "1")]);
        s.record();
        assert!(s.is_current());
        // Negative controls: each of these is a real configure input.
        assert!(
            !stamp(t.path(), &["-DNROS_RMW=xrce"], &[("NROS_X", "1")]).is_current(),
            "a changed -D"
        );
        assert!(
            !stamp(t.path(), &[], &[("NROS_X", "2")]).is_current(),
            "a changed NROS_* environment variable"
        );
        assert!(
            !stamp(t.path(), &[], &[]).is_current(),
            "a removed NROS_* environment variable"
        );
        assert!(
            !stamp(
                t.path(),
                &["-Dnano_ros_ROOT=/elsewhere"],
                &[("NROS_X", "1")]
            )
            .is_current(),
            "a moved SDK root"
        );
    }

    #[test]
    fn a_missing_generated_build_system_is_not_current() {
        let t = tempfile::tempdir().unwrap();
        configured(t.path());
        let s = stamp(t.path(), &[], &[]);
        s.record();
        std::fs::remove_file(t.path().join("src/cmake/build.ninja")).unwrap();
        assert!(!s.is_current(), "a failed configure leaves no build.ninja");
    }

    #[test]
    fn invalidate_forgets_the_stamp() {
        let t = tempfile::tempdir().unwrap();
        configured(t.path());
        let s = stamp(t.path(), &[], &[]);
        s.record();
        s.invalidate();
        assert!(!s.is_current());
    }

    #[test]
    fn the_per_image_identity_is_inert_but_nothing_else_is() {
        let t = tempfile::tempdir().unwrap();
        configured(t.path());
        let a = stamp(t.path(), &["-DNROS_CONFIGURED_IMAGE=b:img_a"], &[]);
        a.record();
        assert!(
            stamp(t.path(), &["-DNROS_CONFIGURED_IMAGE=b:img_b"], &[]).is_current(),
            "another image's answer dir seeds nothing on a configured dir"
        );
        assert!(
            !stamp(t.path(), &["-DNROS_BRINGUP_DIR=/ws/src/b"], &[]).is_current(),
            "a different bringup is a different configure"
        );
    }

    #[test]
    fn ambient_noise_does_not_count() {
        let t = tempfile::tempdir().unwrap();
        configured(t.path());
        let s = stamp(
            t.path(),
            &[],
            &[("PWD", "/a"), ("MAKEFLAGS", "-j4 --jobserver-auth=3,4")],
        );
        s.record();
        assert!(stamp(t.path(), &[], &[("PWD", "/b"), ("MAKEFLAGS", "-j8")]).is_current());
    }

    #[test]
    fn only_a_configure_has_a_stamp() {
        let build = Handoff::new("cmake", ["--build", "b"]);
        assert!(ConfigureStamp::of(&build).is_none());
        let cargo = Handoff::new("cargo", ["generate-lockfile"]);
        assert!(ConfigureStamp::of(&cargo).is_none());
    }

    /// Issue 3019 -- a configure that located a source tree is current only
    /// while the ladder still answers the same. A stand-in `nros` prints the
    /// answer a file holds, so the test moves the answer without a real tree.
    #[cfg(unix)]
    #[test]
    fn a_moved_locate_answer_makes_the_configure_stale() {
        let t = tempfile::tempdir().unwrap();
        let answer = t.path().join("answer");
        let fake = t.path().join("nros");
        crate::test_support::write_executable_stub(
            &fake,
            &format!(
                "#!/bin/sh\ncat {}\necho 'set(NROS_LOCATE_INPUTS \"/x\")'\n\
                 echo 'set(NROS_LOCATE_PROJECT \"\")'\n",
                answer.display()
            ),
        );
        let bdir = t.path().join("b");
        std::fs::create_dir_all(&bdir).unwrap();
        let a = "set(NROS_LOCATE_FREERTOS_KERNEL \"/store/k\")\n";
        std::fs::write(&answer, a).unwrap();
        // What the configure recorded: same answer, a DIFFERENT watch list (a
        // store dir appearing moves that line, never an answer).
        std::fs::write(
            bdir.join(LOCATE_STAMP_NAME),
            format!(
                "nros={}\nindex=/i\nproject=\n---\n{a}set(NROS_LOCATE_INPUTS \"/y\")\n",
                fake.display()
            ),
        )
        .unwrap();
        assert!(
            locate_answers_current(&bdir, &[]),
            "same answer, other inputs"
        );

        std::fs::write(&answer, "set(NROS_LOCATE_FREERTOS_KERNEL \"/co/k\")\n").unwrap();
        assert!(!locate_answers_current(&bdir, &[]), "moved answer");

        // No stamp: the configure asked no ladder, so there is nothing to move.
        assert!(locate_answers_current(&t.path().join("none"), &[]));
        // A stamp that cannot be re-asked is stale, never current.
        std::fs::write(bdir.join(LOCATE_STAMP_NAME), "garbage").unwrap();
        assert!(!locate_answers_current(&bdir, &[]));
    }
}

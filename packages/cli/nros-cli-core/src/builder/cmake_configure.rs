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
//! `NROS_RESOLVED_DIR` is left out on purpose. It names where stage 3.5 put
//! THIS image's answer, so it differs for every image sharing the dir, but the
//! configure only SEEDS the entity-inventory fragment from it when none exists
//! yet (`nros_resolved_seed_entity_inventory`) — on a configured dir it is
//! inert, and counting it re-ran the configure for every row.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use super::handoff::Handoff;

/// File in the build dir holding the stamp of the last configure `nros build`
/// ran there.
pub const STAMP_NAME: &str = ".nros-build-configure.stamp";

/// Cache variables that are inert on a configured dir (see the module doc).
const SEED_ONLY: &[&str] = &["NROS_RESOLVED_DIR"];

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
        Some(Self { build_dir, body })
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
    /// Everything else (a file the configure reads) is `cmake --build`'s check.
    #[must_use]
    pub fn is_current(&self) -> bool {
        let d = &self.build_dir;
        d.join("CMakeCache.txt").is_file()
            && (d.join("build.ninja").is_file() || d.join("Makefile").is_file())
            && std::fs::read_to_string(self.path()).is_ok_and(|s| s == self.body)
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
    fn the_per_image_resolved_dir_is_inert_but_nothing_else_is() {
        let t = tempfile::tempdir().unwrap();
        configured(t.path());
        let a = stamp(t.path(), &["-DNROS_RESOLVED_DIR=/ws/build/img_a"], &[]);
        a.record();
        assert!(
            stamp(t.path(), &["-DNROS_RESOLVED_DIR=/ws/build/img_b"], &[]).is_current(),
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
}

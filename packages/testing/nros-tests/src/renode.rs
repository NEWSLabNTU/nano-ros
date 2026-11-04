//! Renode twin launcher (phase-524 W4c, RFC-0115 D4).
//!
//! The one place a test starts Renode: boot a prebuilt ELF in the board's
//! Renode machine, with the image's zenoh UART on a host pty (which the test's
//! router listens on as `serial/<pty>`) and its console UART in a file.
//! [`crate::qemu`]'s sibling for the boards whose emulator twin is Renode.
//!
//! ## One spelling of each fact
//!
//! Nothing here restates a board fact or a Renode command:
//!
//! * where Renode is: `nros locate tool renode` (`$NROS_RENODE`, then the store
//!   at the pinned `[tool.renode]`), asked once by [`require_renode`];
//! * which machine, which UART carries the link, which the console: the twin
//!   probe track's facts, `scripts/probe/probe-track.py --twin <track>`, which
//!   reads the registry row and the board descriptor's `[board.run.renode]`;
//! * how Renode is driven (the `.resc` script, the pty terminal for the link, the
//!   monitor's `start`, the process group it runs in):
//!   `scripts/probe/twin/renode.sh`, the probe's emulator-kind file, whose four
//!   functions this module calls through `bash`.
//!
//! So the weekly probe track and this launcher boot the twin the same way, and
//! phase-524 W4b's `nros run` Renode runner may later replace both.
//!
//! ## Preconditions
//!
//! A missing Renode is [`crate::unmet!`] (issue 1758): a red naming
//! `nros setup --tool renode`, never a skip.

use crate::{TestError, TestResult, project_root};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Renode, through the one ladder (`nros locate tool renode`), or an unmet
/// precondition naming why it is nowhere.
pub fn require_renode() -> PathBuf {
    crate::require_nros_cli();
    let nros = crate::nros_cli_bin_path().expect("require_nros_cli passed");
    let out = Command::new(&nros)
        .args(["locate", "tool", "renode"])
        .current_dir(project_root())
        .output()
        .unwrap_or_else(|e| panic!("could not run `{} locate tool renode`: {e}", nros.display()));
    if !out.status.success() {
        crate::unmet!(
            "Renode not found (`nros locate tool renode`: {}) -- `nros setup --tool renode` \
             installs the pinned [tool.renode]",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    // The verb answers only with a file (its env rung refuses anything else,
    // and its store rung checks the program exists), so no second probe here.
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

/// A Renode twin booting one ELF, paused until [`RenodeTwin::start`]. Dropping
/// it stops Renode (its whole process group).
pub struct RenodeTwin {
    run: PathBuf,
    track: String,
    renode: PathBuf,
    monitor_port: u16,
    up: bool,
}

impl RenodeTwin {
    /// Start Renode PAUSED for `elf` (a Zephyr build's `zephyr/zephyr.elf`) in
    /// the machine of twin track `track` (a `[[probe]]` row of
    /// `board-support.toml` with `twin = <board>`), with the image's link on
    /// the pty [`RenodeTwin::link`]. Returns once that pty exists, so a router
    /// can open it before the image runs. `monitor_port` is Renode's monitor
    /// (one per concurrently running twin); `run` is a fresh scratch dir.
    pub fn up(
        renode: &Path,
        track: &str,
        elf: &Path,
        run: &Path,
        monitor_port: u16,
    ) -> TestResult<Self> {
        // `nros_twin_up <elf-dir>` reads `<elf-dir>/zephyr/zephyr.elf`.
        let elf_dir = elf
            .parent()
            .and_then(Path::parent)
            .filter(|_| elf.ends_with("zephyr/zephyr.elf"))
            .ok_or_else(|| {
                TestError::BuildFailed(format!(
                    "{} is not a Zephyr build's zephyr/zephyr.elf",
                    elf.display()
                ))
            })?;
        let _ = std::fs::remove_dir_all(run);
        std::fs::create_dir_all(run).map_err(TestError::ProcessStart)?;
        let twin = Self {
            run: run.to_path_buf(),
            track: track.to_string(),
            renode: renode.to_path_buf(),
            monitor_port,
            up: true,
        };
        twin.kind_fn(
            "nros_twin_up",
            &[&elf_dir.to_string_lossy(), &run.to_string_lossy()],
        )?;
        Ok(twin)
    }

    /// The host pty Renode connected to the image's zenoh UART.
    pub fn link(&self) -> PathBuf {
        self.run.join("link")
    }

    /// Run the simulation (the image boots now).
    pub fn start(&self) -> TestResult<()> {
        self.kind_fn("nros_twin_start", &[&self.run.to_string_lossy()])
    }

    /// What the image's console UART has printed so far.
    pub fn console(&self) -> String {
        std::fs::read(self.run.join("console.log"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    /// Renode's own log (its stdout and stderr), for a failure message.
    pub fn renode_log(&self) -> String {
        std::fs::read(self.run.join("renode.log"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    /// Poll the console until it holds `pattern` `count` times, or `budget`
    /// runs out. Returns the console either way; the caller asserts.
    pub fn wait_console(&self, pattern: &str, count: usize, budget: Duration) -> String {
        let deadline = Instant::now() + budget;
        loop {
            let text = self.console();
            if text.matches(pattern).count() >= count || Instant::now() >= deadline {
                return text;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Stop Renode. Idempotent; also run on drop.
    pub fn down(&mut self) {
        if self.up {
            self.up = false;
            let _ = self.kind_fn("nros_twin_down", &[&self.run.to_string_lossy()]);
        }
    }

    /// Call one of the emulator kind's four functions, with the track's board
    /// facts in the environment exactly as the probe harness assigns them.
    fn kind_fn(&self, func: &str, args: &[&str]) -> TestResult<()> {
        let script = r#"
            set -euo pipefail
            track="$1"; func="$2"; shift 2
            facts="$(python3 scripts/probe/probe-track.py --twin "$track")"
            eval "$facts"
            if [ "$NROS_TWIN_KIND" != renode ]; then
                echo "twin track $track is kind '$NROS_TWIN_KIND', not renode" >&2
                exit 2
            fi
            . "scripts/probe/twin/$NROS_TWIN_KIND.sh"
            "$func" "$@"
        "#;
        let out = Command::new("bash")
            .arg("-c")
            .arg(script)
            .arg("nros-renode-twin")
            .arg(&self.track)
            .arg(func)
            .args(args)
            .current_dir(project_root())
            .env("NROS_TWIN_RENODE", &self.renode)
            .env("NROS_TWIN_MONITOR_PORT", self.monitor_port.to_string())
            .output()
            .map_err(TestError::ProcessStart)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(TestError::ProcessFailed(format!(
                "{func} for twin track {} failed ({}):\n{}{}\n--- renode.log ---\n{}",
                self.track,
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
                self.renode_log()
            )))
        }
    }
}

impl Drop for RenodeTwin {
    fn drop(&mut self) {
        self.down();
    }
}

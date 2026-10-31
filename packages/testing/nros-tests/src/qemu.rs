//! QEMU process fixture for embedded testing
//!
//! Provides managed QEMU processes for testing ARM Cortex-M binaries.
//!
//! ## Launch convention — SANCTIONED hand-rolled builders (phase-295 W5.c)
//!
//! Every `QemuProcess::start_*` builder below assembles a `qemu-system-{arm,
//! riscv32,riscv64}` command line by hand. This is the **sanctioned form**
//! (RFC-0051 §4 / audit checklist E9 / the #222 E1-exception pattern), not a
//! bypass to fix, because the platforms this module launches — FreeRTOS,
//! NuttX, ThreadX, and pure bare-metal — have **no build-system runner
//! metadata** to interpret:
//!
//! - There is no Zephyr `runners.yaml` (that path is handled by the
//!   [`crate::zephyr::RunnersYaml`] interpreter) and no ESP-IDF
//!   `flasher_args.json` ([`crate::esp32::EspFlasherArgs`]) for these images.
//!   They are plain cross-compiled ELFs; the emulator machine/CPU/netdev plan
//!   lives only in each board crate's build glue and this file.
//! - So `qemu.rs` is itself "the interpreter" E9 refers to: the hand-rolled
//!   `-M/-cpu/-netdev` blocks here are the run convention, and consuming
//!   PREBUILT ELFs (never building at run time) keeps the E1 no-compile rule.
//!
//! FOLLOW-UP (phase doc W5.c): the per-machine argument blocks (the
//! `-M virt -cpu cortex-a7 …` specifics) can later move into each board
//! crate's `NROS_BOARD_RUNNER`-adjacent metadata so a board owns its launch
//! line (the phase-215 duty rule), leaving these functions as thin readers of
//! board-provided launch metadata. That relocation is deferred here to avoid
//! disturbing the many green QEMU lanes; the doc-comment route is the
//! sanctioned interim state.

use crate::{
    TestError, TestResult,
    process::{kill_process_group, set_new_process_group},
};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// Issue 1851 — the ONE ladder for a QEMU system binary (`qemu-system-arm`,
/// `qemu-system-riscv64`, `qemu-system-aarch64`, …), the harness's spelling of
/// [`nros_qemu_ladder::locate`] (phase-517 W14 — the CLI's printed run commands
/// call the same function):
///
///   1. `QEMU_SYSTEM_<ARCH>` (e.g. `QEMU_SYSTEM_ARM`) — developer override /
///      CI pin.
///   2. The project-local prefix `<project_root>/build/qemu/bin/` (made by
///      `just qemu setup-qemu` / `just linux setup`); a prefix that is not the
///      pinned version PANICS here (issue 0930 — a test must not run on an
///      emulator nobody chose), where the CLI skips it.
///   3. The `nros setup` store, at the PINNED `[tool.qemu]` version.
///   4. `$PATH` — harness-only; the CLI has no such rung. So a host that never
///      ran setup still produces the documented precondition report rather
///      than an exec error.
///
/// Its shell twin is `nros_qemu_bin` (`scripts/lib/qemu-bin.sh`); the two must
/// answer alike, which is why neither the tests nor the recipes may launch a
/// `qemu-system-*` by bare name (gate `check-qemu-one-ladder`).
pub fn qemu_system_path(exe: &str) -> std::ffi::OsString {
    qemu_system_path_in(exe, "qemu", true)
}

/// [`qemu_system_path`] with the store row and the project-prefix rung named.
/// The ESP32 binary is a different QEMU (Espressif's fork, `[tool.esp32-qemu]`)
/// that the project prefix never holds, so it skips rung 2.
pub(crate) fn qemu_system_path_in(
    exe: &str,
    tool: &str,
    project_prefix: bool,
) -> std::ffi::OsString {
    let pin = nros_build_paths::sdk_pinned_version(tool);
    let getenv = |k: &str| std::env::var_os(k);
    let ctx = nros_qemu_ladder::Ctx {
        getenv: &getenv,
        // phase-334 W2.b step 2 — the Rust mirror of `nros_build_dir`.
        project_prefix: project_prefix.then(|| crate::build_dir(crate::kind::QEMU, &[])),
        store_dir: pin
            .as_ref()
            .map(|v| crate::store_root().join("sdk").join(tool).join(v)),
        pin,
        // No index reader here: every target is taken as the store's, which for
        // a target it does not build ends at rung 4 either way.
        store_targets: None,
        stale_prefix: nros_qemu_ladder::StalePrefix::Refuse,
    };
    match nros_qemu_ladder::locate(exe, &ctx) {
        nros_qemu_ladder::QemuBin::Found { path, rung } => {
            if rung == "store" {
                // The store copy must also LOAD (`[tool.qemu] system`).
                if let Some(checked) = crate::nros_store_bin(tool, exe) {
                    return checked.into_os_string();
                }
            }
            path.into_os_string()
        }
        nros_qemu_ladder::QemuBin::RefusedPrefix { path, why } => panic!(
            "{} is not the QEMU the index pins: {why}. A test must not run on \
             an emulator nobody chose (issue 0930) -- re-provision it with \
             `just qemu setup-qemu`, or remove {} to use the store copy",
            path.display(),
            path.parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(&path)
                .display()
        ),
        nros_qemu_ladder::QemuBin::Missing { .. } | nros_qemu_ladder::QemuBin::Host { .. } => {
            std::ffi::OsString::from(exe)
        }
    }
}

/// A [`Command`] for `line` booting `kernel` — the binary through
/// [`qemu_system_path`], the arguments from [`nros_qemu_ladder::board_line`].
/// The CLI's printed run command comes from the same line (phase-517 W14).
pub fn board_line_cmd(line: &nros_qemu_ladder::Line, kernel: &Path) -> Command {
    let mut cmd = qemu_system_cmd(line.exe);
    cmd.args(&line.before_kernel)
        .arg(kernel)
        .args(&line.after_kernel);
    cmd
}

/// Issue 0930 — whether the project prefix holds the version `[tool.<tool>]`
/// pins ([`nros_qemu_ladder::prefix_matches_pin`] against this checkout's pin).
/// The ladder asks it itself; this spelling is for the unit test below.
#[cfg(test)]
pub(crate) fn project_prefix_matches_pin(prefix: &Path, tool: &str) -> Result<(), String> {
    nros_qemu_ladder::prefix_matches_pin(
        prefix,
        nros_build_paths::sdk_pinned_version(tool).as_deref(),
    )
}

/// The line [`QemuProcess::start_nuttx_virt`] /[`QemuProcess::start_nuttx_riscv`]
/// boot: user networking, or no NIC for a boot-only image.
pub fn nuttx_virt_line(board: nros_qemu_ladder::Board, networking: bool) -> nros_qemu_ladder::Line {
    let net = if networking {
        nros_qemu_ladder::Net::User { mac: None }
    } else {
        nros_qemu_ladder::Net::None
    };
    nros_qemu_ladder::board_line(board, net)
}

/// The line [`QemuProcess::start_riscv64_virt`] boots for `peer_index` (its MAC
/// is the firmware's preset for that peer).
pub fn threadx_riscv64_line(peer_index: u8) -> nros_qemu_ladder::Line {
    let mac = nros_qemu_ladder::threadx_riscv64_mac(peer_index);
    nros_qemu_ladder::board_line(
        nros_qemu_ladder::Board::ThreadxRiscv64,
        nros_qemu_ladder::Net::User { mac: Some(&mac) },
    )
}

/// A fresh [`Command`] for [`qemu_system_path`]`(exe)`.
pub fn qemu_system_cmd(exe: &str) -> Command {
    Command::new(qemu_system_path(exe))
}

/// Whether [`qemu_system_path`]`(exe)` runs (`--version` succeeds).
pub fn qemu_system_available(exe: &str) -> bool {
    qemu_system_cmd(exe)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Whether [`qemu_system_path`]`(exe)` runs and offers `machine`.
///
/// The question a precondition must ask is the one its BOOT needs: the binary
/// the boot helper will resolve, with the machine it will pass to `-M`.
pub fn qemu_system_has_machine(exe: &str, machine: &str) -> bool {
    let Ok(out) = qemu_system_cmd(exe).args(["-machine", "help"]).output() else {
        return false;
    };
    out.status.success()
        && String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.split_whitespace().next() == Some(machine))
}

/// Whether [`QemuProcess::start_nuttx_riscv`] can boot: its binary
/// (`qemu-system-riscv32` through [`qemu_system_path`]) with the `virt`
/// machine.
///
/// Issue 1884 — the NuttX riscv tests asked
/// [`crate::esp32::is_qemu_riscv32_available`] instead. That probe is for
/// ESP32-C3: it resolves the Espressif fork's store row and demands its
/// `esp32c3` machine, which a NuttX rv-virt image never uses. A stock
/// `qemu-system-riscv32` (apt `qemu-system-misc`, what the CI image installs)
/// boots the image and failed that probe, so the tier-2 `nuttx-riscv`
/// coordinate reported an unmet precondition on every host without the fork.
pub fn is_qemu_riscv32_virt_available() -> bool {
    qemu_system_has_machine("qemu-system-riscv32", "virt")
}

/// Whether the Zephyr `qemu_cortex_a53` SMP boot can run: its binary
/// (`qemu-system-aarch64` through [`qemu_system_path`]) with the `virt`
/// machine.
///
/// Issue 2113 — the `[tool.qemu]` store build targets arm and riscv64 only, so
/// this binary comes from the `qemu-system-aarch64` prereq (apt's
/// `qemu-system-arm` package ships it). Without one the boot used to die as a
/// bare ENOENT at spawn, naming no tool.
pub fn is_qemu_aarch64_virt_available() -> bool {
    qemu_system_has_machine("qemu-system-aarch64", "virt")
}

/// `qemu-system-arm` through [`qemu_system_path`].
pub fn qemu_system_arm_path() -> std::ffi::OsString {
    qemu_system_path("qemu-system-arm")
}

/// Convenience wrapper around [`qemu_system_arm_path`] that returns
/// a fresh [`Command`] preconfigured to invoke the resolved binary.
/// Prefer this over `Command::new("qemu-system-arm")` so future
/// `build/qemu/` patches propagate automatically (Phase 143).
pub fn qemu_system_arm_cmd() -> Command {
    Command::new(qemu_system_arm_path())
}

/// Best-effort discovery of the cargo workspace root by walking
/// Phase 88.16.A — flip a piped stdio fd to non-blocking so the
/// drain loop below never wedges.
#[cfg(unix)]
fn set_nonblocking<F: AsRawFd>(fd: &F) {
    let raw = fd.as_raw_fd();
    // SAFETY: `raw` is a valid OS fd we own; fcntl is async-signal-safe.
    unsafe {
        let flags = libc::fcntl(raw, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}

/// Phase 88.16.A — drain whatever bytes are currently buffered on
/// `src` into `dst`. Returns `true` if any bytes were consumed.
/// Treats `WouldBlock`, `0`, and read errors as "no progress".
fn drain_into<R: Read>(src: &mut R, buffer: &mut [u8], dst: &mut String) -> bool {
    match src.read(buffer) {
        Ok(0) => false,
        Ok(n) => {
            crate::capture::append(dst, &buffer[..n]);
            true
        }
        Err(_) => false,
    }
}

// phase-334 W2.b step 2 — the private `project_root()` walk-up that used to
// live here is GONE, not silenced. Its only caller was `qemu_system_arm_path`,
// and the root it recomputed is what `crate::build_dir` already derives. Two
// spellings of "where is the repo" is the R3 drift this step exists to remove;
// keeping it behind an `#[allow(dead_code)]` would have preserved the second
// spelling for the next caller to find.

/// Managed QEMU process for Cortex-M3 emulation
///
/// Starts QEMU with semihosting enabled and captures output.
/// Automatically kills the process on drop.
///
/// # Example
///
/// ```ignore
/// use nros_tests::fixtures::QemuProcess;
/// use std::path::Path;
///
/// let binary = Path::new("target/thumbv7m-none-eabi/release/my-test");
/// let mut qemu = QemuProcess::start_cortex_m3(binary).unwrap();
/// let output = qemu.wait_for_output(Duration::from_secs(30)).unwrap();
/// assert!(output.contains("[PASS]"));
/// ```
pub struct QemuProcess {
    handle: Child,
    /// The exact command this process was started with — see
    /// [`QemuProcess::command_line`].
    cmdline: String,
}

/// Render a spawned command as a line a human can paste into a shell.
///
/// Issue 0877 — "run it by hand with the harness's own QEMU arguments" is only
/// a valid experiment if the hand run uses the harness's own QEMU BINARY, and
/// nothing here ever said which one that is. [`qemu_system_arm_path`] prefers
/// `build/qemu/bin/qemu-system-arm` (our LAN9118 flow-control patch, issues
/// 0830 / 0917) over the `qemu-system-arm` a shell resolves from `$PATH`, so
/// copying the ARGUMENTS alone silently swaps the emulated NIC's RX flow
/// control for a different implementation — the one component a delivery
/// failure is most likely to be about. Recording the resolved program next to
/// its arguments makes the two runs comparable.
///
/// Quoting is the conservative kind: anything outside a known-safe set is
/// single-quoted, so the line is pasteable rather than merely readable.
pub(crate) fn render_command(cmd: &Command) -> String {
    fn quote(word: &str) -> String {
        let safe = |c: char| c.is_ascii_alphanumeric() || "_-./=:,@%+".contains(c);
        if !word.is_empty() && word.chars().all(safe) {
            word.to_string()
        } else {
            format!("'{}'", word.replace('\'', r"'\''"))
        }
    }
    let mut out = quote(&cmd.get_program().to_string_lossy());
    for arg in cmd.get_args() {
        out.push(' ');
        out.push_str(&quote(&arg.to_string_lossy()));
    }
    out
}

impl QemuProcess {
    /// Spawn `cmd`, recording the exact command line for failure messages.
    ///
    /// THE one spawn point for every `start_*` builder in this file, so a new
    /// builder cannot forget to record what it ran (issue 0877).
    fn spawn(mut cmd: Command) -> TestResult<Self> {
        let cmdline = render_command(&cmd);
        let handle = cmd.spawn()?;
        Ok(Self { handle, cmdline })
    }

    /// The command line this QEMU was started with, pasteable into a shell.
    ///
    /// Assertion messages that report "the guest did nothing" must carry this:
    /// the RTOS lanes run under `--failure-output never` (see the `just
    /// <platform> test` recipes), so anything printed with `eprintln!` is
    /// discarded and only the panic text survives. Issue 0877 is what that
    /// costs — a report whose central experiment was a hand run that could not
    /// have used the same emulator, with nothing in the failure output to say
    /// so.
    #[must_use]
    pub fn command_line(&self) -> &str {
        &self.cmdline
    }

    /// Start QEMU Cortex-M3 emulator with semihosting
    ///
    /// Uses the LM3S6965EVB machine which supports semihosting output.
    ///
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_cortex_m3(binary: &Path) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = qemu_system_arm_cmd();
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "lm3s6965evb",
            "-nographic",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Start QEMU with MPS2-AN385 machine (Cortex-M3 + LAN9118 Ethernet)
    ///
    /// Uses the MPS2-AN385 machine which has a LAN9118 Ethernet controller.
    ///
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_mps2_an385(binary: &Path) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = qemu_system_arm_cmd();
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-nographic",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Start QEMU with MPS2-AN385 machine + LAN9118 slirp networking
    ///
    /// Uses the MPS2-AN385 machine with a LAN9118 Ethernet controller in QEMU's
    /// user-mode (slirp) networking. Each QEMU instance gets its own fully
    /// isolated NAT stack — no TAP devices, no bridge, no sudo needed.
    ///
    /// The firmware connects to the host via slirp gateway `10.0.2.2`. The
    /// firmware's `config.toml` must use the `10.0.2.0/24` subnet with gateway
    /// `10.0.2.2` and a unique guest IP per instance.
    ///
    /// Uses `-icount shift=auto` to synchronize QEMU's virtual clock with
    /// wall-clock time. Without this, hardware timers (CMSDK Timer0) race ahead
    /// during WFI, causing zenoh-pico timeouts to expire before network I/O
    /// completes. See `docs/reference/qemu-icount.md`.
    pub fn start_mps2_an385_networked(binary: &Path) -> TestResult<Self> {
        Self::spawn(mps2_an385_networked_cmd(binary)?)
    }

    /// [`Self::start_mps2_an385_networked`] on the network the IMAGE states
    /// (RFC-0103 D10a, phase-497 W3): `nic` is the `-nic` value of its
    /// interface, from [`image_nic`] — QEMU's own user-mode network, a host
    /// bridge, or a TAP.
    pub fn start_mps2_an385_on(binary: &Path, nic: &str) -> TestResult<Self> {
        Self::spawn(mps2_an385_cmd_with_nic(binary, nic)?)
    }

    /// [`Self::start_mps2_an385_networked`] plus a QEMU human monitor on a
    /// unix socket, for a guest whose verdict is in its MEMORY rather than on
    /// its console (issue 1036: the boot report of an image with no console).
    /// Talk to it with [`qemu_monitor_command`]; `pmemsave` there is the
    /// QEMU spelling of the `savemem` a probe performs on a board.
    pub fn start_mps2_an385_networked_with_monitor(
        binary: &Path,
        monitor_socket: &Path,
    ) -> TestResult<Self> {
        let mut cmd = mps2_an385_networked_cmd(binary)?;
        cmd.arg("-monitor").arg(format!(
            "unix:{},server=on,wait=off",
            monitor_socket.display()
        ));
        Self::spawn(cmd)
    }

    // phase-484 W4b-2 (RFC-0103 D10) — `start_mps2_an385_networked` is
    // gone. It configured SLIRP to `192.0.3.0/24` (host `192.0.3.1`) because
    // the FreeRTOS firmware's lwIP was static on that plan; the FreeRTOS QEMU
    // boards now sit on QEMU's own SLIRP network like every other QEMU board,
    // so they boot with `start_mps2_an385_networked`, the one MPS2 launcher.

    /// Start QEMU `mps3-an536` (Cortex-R52) on a multicast-socket LAN.
    ///
    /// Issue 1624 — the FreeRTOS AN536 board's runner (its `nros-board.toml`)
    /// with a LAN9118 on a `-net socket,mcast` backend: no `sudo`, no tap, and
    /// unlike slirp it carries the SPDP multicast a CycloneDDS participant
    /// sends. A guest alone on the group still delivers to itself, which is
    /// what an in-image talker + listener needs.
    pub fn start_mps3_an536_mcast(binary: &Path, mcast_addr_port: &str) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = qemu_system_arm_cmd();
        cmd.args([
            "-machine",
            "mps3-an536",
            "-nographic",
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .args([
            "-nic",
            &format!("socket,model=lan9118,mcast={mcast_addr_port}"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Start QEMU `mps3-an536` (Cortex-R52) on user-mode networking (slirp),
    /// forwarding the host UDP ports a Cyclone guest receives on.
    ///
    /// phase-441 W6 — the shape a live ROS 2 peer on the HOST can reach with
    /// no `sudo` and no tap. Slirp carries no multicast, so the guest's
    /// Cyclone config must be unicast-only and name its peer; `hostfwd` is the
    /// clause list `CycloneSlirpPair::qemu_hostfwd` writes for the guest's two
    /// unicast ports. The guest's address is the board's QEMU network plan
    /// (`10.0.2.15`, gateway `10.0.2.2`).
    pub fn start_mps3_an536_slirp(binary: &Path, hostfwd: &str) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = qemu_system_arm_cmd();
        cmd.args([
            "-machine",
            "mps3-an536",
            "-nographic",
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .args(["-nic", &format!("user,model=lan9118,{hostfwd}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Start QEMU with MPS2-AN385 machine + external serial device
    ///
    /// Connects UART0 to the given serial device path (e.g., a socat PTY).
    /// Uses `-display none -monitor none` to avoid `-nographic`'s implicit
    /// `-serial mon:stdio` which would hijack UART0 for the monitor.
    /// Semihosting output goes to stdout via the ARM semihosting interface.
    ///
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    /// * `serial_path` - Host serial device path (e.g., `/tmp/serial-a`)
    pub fn start_mps2_an385_with_serial(binary: &Path, serial_path: &str) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let chardev_arg = format!("serial,id=ser0,path={}", serial_path);
        let mut cmd = qemu_system_arm_cmd();
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-display",
            "none",
            "-monitor",
            "none",
            // Synchronize virtual clock with wall-clock time. The firmware
            // uses CMSDK Timer0 (hardware timer) for zenoh-pico timeouts.
            // Without icount, WFI advances virtual time instantly, causing
            // timeouts to fire before serial I/O through zenohd completes.
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-chardev",
        ])
        .arg(&chardev_arg)
        .args(["-serial", "chardev:ser0", "-kernel"])
        .arg(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Wait for QEMU to produce output and exit
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    /// The combined stdout/stderr output
    pub fn wait_for_output(&mut self, timeout: Duration) -> TestResult<String> {
        let start = Instant::now();
        let mut output = String::new();

        // Phase 88.16.A — drain stderr alongside stdout so logging
        // records (which route through `hstderr()` on bare-metal
        // semihosting) reach the captured string. Examples that
        // emit via `log_info!` would otherwise look silent to the
        // harness.
        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed("No stdout".to_string()))?;
        let mut stderr = self.handle.stderr.take();

        // Set non-blocking mode so read() doesn't block indefinitely when the
        // QEMU process pauses output (e.g., listener waiting for messages).
        #[cfg(unix)]
        {
            set_nonblocking(&stdout);
            if let Some(ref s) = stderr {
                set_nonblocking(s);
            }
        }

        let mut buffer = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                kill_process_group(&mut self.handle);
                if output.is_empty() {
                    return Err(TestError::Timeout);
                }
                break;
            }

            match self.handle.try_wait() {
                Ok(Some(_status)) => {
                    let _ = stdout.read_to_string(&mut output);
                    if let Some(mut s) = stderr.take() {
                        let _ = s.read_to_string(&mut output);
                    }
                    break;
                }
                Ok(None) => {
                    let mut progressed = false;
                    progressed |= drain_into(&mut stdout, &mut buffer, &mut output);
                    if let Some(ref mut s) = stderr {
                        progressed |= drain_into(s, &mut buffer, &mut output);
                    }
                    if !progressed {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    // Check for test completion markers
                    if output.contains("All tests passed")
                        || output.contains("Benchmark complete")
                        || output.contains("TEST COMPLETE")
                        || output.contains("QEMU: Terminated")
                        || output.contains(crate::output::SERVICE_RESULT_PREFIX)
                        || output.contains(crate::output::ACTION_RESULT_PREFIX)
                    {
                        std::thread::sleep(Duration::from_millis(100));
                        kill_process_group(&mut self.handle);
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Wait until the output contains `pattern` — the STRICT wait. Kills the
    /// process on timeout.
    ///
    /// `Ok(output)` means the pattern appeared. `Err` means it did not, whether
    /// because the timeout elapsed or because QEMU exited first, and quotes the
    /// output collected so far.
    ///
    /// Issue 0471 was filed against [`crate::process::ManagedProcess`]; this is
    /// the same defect in the same shape, found by fixing the class rather than
    /// the reported site. Both types had TWO lenient returns — timeout with
    /// non-empty output, and process exit — and on an emulator target the exit
    /// path is the common one, so this copy mattered more.
    ///
    /// When a missing pattern is not itself the failure, use
    /// [`Self::collect_until`].
    pub fn wait_for_output_pattern(
        &mut self,
        pattern: &str,
        timeout: Duration,
    ) -> TestResult<String> {
        let out = self.collect_until_inner(pattern, timeout)?;
        if out.contains(pattern) {
            Ok(out)
        } else {
            Err(TestError::ProcessFailed(format!(
                "qemu did not print `{pattern}` within {timeout:?}. Output:\n{out}"
            )))
        }
    }

    /// Wait until EVERY pattern in `patterns` has appeared, then return the
    /// accumulated output. Errors naming the ones still missing at the
    /// deadline.
    ///
    /// issue 0636 — the wait to use when the rule is "each of these must
    /// report". [`Self::wait_for_output_pattern`] returns at the first match,
    /// which for a per-tier rule means the assertion runs against a log that
    /// was cut mid-boot; the failure then reads as a dropped dim rather than as
    /// a short read, which is the most expensive kind of wrong.
    /// Each GROUP must be satisfied by ANY of its alternatives — the shape a
    /// per-tier rule actually has, where a tier may report either its accept
    /// marker or its loud-failure marker and both count as "it reported".
    /// A single-element group degenerates to "this must appear".
    pub fn wait_for_output_each(
        &mut self,
        groups: &[Vec<String>],
        timeout: Duration,
    ) -> TestResult<String> {
        let satisfied = |o: &str, g: &Vec<String>| g.iter().any(|p| o.contains(p.as_str()));
        let out = self.collect_until_pred(|o| groups.iter().all(|g| satisfied(o, g)), timeout)?;
        let missing: Vec<String> = groups
            .iter()
            .filter(|g| !satisfied(&out, g))
            .map(|g| g.join(" | "))
            .collect();
        if missing.is_empty() {
            Ok(out)
        } else {
            Err(TestError::ProcessFailed(format!(
                "qemu did not print {} within {timeout:?}: {}. Output:\n{out}",
                if missing.len() == 1 { "this" } else { "these" },
                missing.join(", ")
            )))
        }
    }

    /// Collect output, stopping early once `pattern` appears — the LENIENT wait.
    ///
    /// Returns whatever QEMU printed, pattern or not. Correct for readiness
    /// waits and for tests that assert on the content themselves (and want the
    /// output in their own failure message). See
    /// [`crate::process::ManagedProcess::collect_until`].
    ///
    /// Returns `String`, not `TestResult`, to match `ManagedProcess` — callers
    /// use the value in their own assertion message. A harness-level failure
    /// (stdout already taken, or QEMU silent until the deadline) is rendered
    /// INTO the returned text rather than dropped, so the assertion that
    /// follows prints the reason instead of an empty diff.
    pub fn collect_until(&mut self, pattern: &str, timeout: Duration) -> String {
        match self.collect_until_inner(pattern, timeout) {
            Ok(out) => out,
            Err(e) => format!("<no output collected: {e}>"),
        }
    }

    fn collect_until_inner(&mut self, pattern: &str, timeout: Duration) -> TestResult<String> {
        self.collect_until_pred(|out| out.contains(pattern), timeout)
    }

    /// Collect output, stopping early once `pattern` has appeared `expected`
    /// times — the COUNTING sibling of [`Self::collect_until`], lenient in
    /// exactly the same way.
    ///
    /// This is the wait for a FREE-RUNNING producer, which is the one shape
    /// [`Self::wait_for_output`] cannot serve: that call is run-to-completion
    /// (it kills the guest at the deadline), so pointing it at a node with no
    /// terminal state silently makes the timeout the node's LIFETIME —
    /// issue 1013, where a 15 s window ended a 1 Hz talker after twelve
    /// publishes and the cell still reported PASS. A count says what such a
    /// test actually means: "keep it alive until it has produced this much".
    ///
    /// Returns whatever was printed, count reached or not, so the caller can
    /// assert on the count itself and put the real transcript in its own
    /// failure message.
    pub fn collect_until_count(
        &mut self,
        pattern: &str,
        expected: usize,
        timeout: Duration,
    ) -> String {
        match self.collect_until_pred(|out| out.matches(pattern).count() >= expected, timeout) {
            Ok(out) => out,
            Err(e) => format!("<no output collected: {e}>"),
        }
    }

    /// The general form: collect until `done` accepts the accumulated output,
    /// or the deadline passes.
    ///
    /// issue 0636 — [`Self::wait_for_output_pattern`] stops at the FIRST match
    /// and then the caller kills QEMU, so a test whose rule needs SEVERAL
    /// distinct markers only sees the later ones if they happened to land in
    /// the same buffer flush. That is a coin flip, not a wait: the multi-tier
    /// sched-dim cells assert one marker PER TIER, and the third tier's line
    /// went missing in 2 of 8 runs purely because the reader had already
    /// returned. Waiting on a predicate lets the caller say "all of them".
    fn collect_until_pred(
        &mut self,
        done: impl Fn(&str) -> bool,
        timeout: Duration,
    ) -> TestResult<String> {
        let start = Instant::now();
        let mut output = String::new();

        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed("No stdout".to_string()))?;
        let mut stderr = self.handle.stderr.take();

        #[cfg(unix)]
        {
            set_nonblocking(&stdout);
            if let Some(ref s) = stderr {
                set_nonblocking(s);
            }
        }

        let mut buffer = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                kill_process_group(&mut self.handle);
                if output.is_empty() {
                    return Err(TestError::Timeout);
                }
                break;
            }

            match self.handle.try_wait() {
                Ok(Some(_)) => {
                    let _ = stdout.read_to_string(&mut output);
                    if let Some(mut s) = stderr.take() {
                        let _ = s.read_to_string(&mut output);
                    }
                    break;
                }
                Ok(None) => {
                    let mut progressed = false;
                    progressed |= drain_into(&mut stdout, &mut buffer, &mut output);
                    if let Some(ref mut s) = stderr {
                        progressed |= drain_into(s, &mut buffer, &mut output);
                    }
                    if done(&output) {
                        // Put streams back so follow-up `wait_for_output`
                        // / `kill` calls see them.
                        self.handle.stdout = Some(stdout);
                        self.handle.stderr = stderr;
                        return Ok(output);
                    }
                    if !progressed {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Kill the QEMU process
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
    }

    /// Check if QEMU is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }

    /// Wait up to `timeout` for QEMU to exit on its own and return its status,
    /// or `None` if it is still running then (the caller decides whether that
    /// is a failure — most images spin forever).
    ///
    /// Issue 1770 — an image that ENDS does so through semihosting, and QEMU's
    /// exit status is then the image's verdict. That is only worth reading if
    /// it carries the app's status: the FreeRTOS C/C++ entry used the A64
    /// `SYS_EXIT` block form on A32 and exited 1 whatever the app did.
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.handle.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                _ => return None,
            }
        }
    }

    pub fn start_nuttx_virt(binary: &Path, networking: bool) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        // phase-517 W14 — the line is the ONE the CLI prints too (the
        // virtio-mmio NIC and `-icount` reasons live with it).
        let mut cmd = board_line_cmd(
            &nuttx_virt_line(nros_qemu_ladder::Board::NuttxArmVirt, networking),
            binary,
        );
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// #165 — boot a riscv32 rv-virt NuttX kernel image (the riscv sibling of
    /// [`Self::start_nuttx_virt`]). Command mirrors the canonical rv-virt export
    /// `scripts/nuttx/build-nuttx.sh` emits: `qemu-system-riscv32 -M virt -bios
    /// none` + a virtio-net-device on the MMIO transport (NuttX's rv-virt
    /// defconfig enables `CONFIG_DRIVERS_VIRTIO_MMIO`, so — as on arm-virt — the
    /// NIC must be `-device virtio-net-device`, not the PCI default `-nic user`).
    /// `-icount shift=auto` syncs the virtual clock to wall-clock so `sleep()` /
    /// timeouts run at real rates, matching `start_nuttx_virt`.
    pub fn start_nuttx_riscv(binary: &Path, networking: bool) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = board_line_cmd(
            &nuttx_virt_line(nros_qemu_ladder::Board::NuttxRvVirt, networking),
            binary,
        );
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Start QEMU with RISC-V 64-bit virt machine + virtio-net slirp networking
    ///
    /// Used for ThreadX QEMU RISC-V tests. The virt machine provides a virtio-net
    /// MMIO interface using QEMU's user-mode (slirp) networking. Each instance
    /// gets its own isolated NAT stack — no TAP devices, no bridge, no sudo needed.
    ///
    /// The `peer_index` selects the MAC address:
    /// - 0: MAC 52:54:00:12:34:56 (talker/server)
    /// - 1: MAC 52:54:00:12:34:57 (listener/client)
    ///
    /// MAC addresses use the QEMU OUI range (52:54:00) with the last byte
    /// derived from the peer index (0x56 + index). These must match the
    /// firmware's `Config::default()` / `Config::listener()` presets in
    /// `nros-board-threadx-qemu-riscv64`.
    /// Phase 97.4.threadx-riscv64 — sibling-mcast variant. Two
    /// QEMU instances share `-netdev socket,mcast=…` so SPDP /
    /// SEDP / pubsub frames cross between them on the host's
    /// loopback / primary iface (no `localaddr` — same lesson as
    /// the FreeRTOS slice).
    pub fn start_riscv64_virt_mcast(
        binary: &Path,
        mcast_addr_port: &str,
        mac: &str,
    ) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = qemu_system_cmd("qemu-system-riscv64");
        cmd.args([
            "-M",
            "virt",
            "-m",
            "256M",
            "-bios",
            "none",
            "-nographic",
            "-global",
            "virtio-mmio.force-legacy=false",
            "-kernel",
        ])
        .arg(binary)
        .args([
            "-netdev",
            &format!("socket,id=net0,mcast={mcast_addr_port}"),
            "-device",
            &format!("virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac={mac}"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    /// Phase 127.B.5 — QEMU 7.2+ `-netdev dgram,local.type=unix,…`
    /// peer pair for RV64 ThreadX. It avoids QEMU's broken cross-process
    /// `socket,mcast=` delivery by routing two QEMU processes at each
    /// other via an AF_UNIX SOCK_DGRAM pair, with no root, routes or IGMP.
    /// The caller picks two distinct per-pair paths and deletes stale files
    /// at them first (QEMU's bind fails on EADDRINUSE).
    pub fn start_riscv64_virt_dgram(
        binary: &Path,
        local_unix_path: &str,
        remote_unix_path: &str,
        mac: &str,
    ) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        // Issue 2170 — the board's machine line from the shared ladder, the
        // pair link as its `Net::Dgram`.
        let line = nros_qemu_ladder::board_line(
            nros_qemu_ladder::Board::ThreadxRiscv64,
            nros_qemu_ladder::Net::Dgram {
                local: local_unix_path,
                remote: remote_unix_path,
                mac,
            },
        );
        let mut cmd = board_line_cmd(&line, binary);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }

    pub fn start_riscv64_virt(binary: &Path, peer_index: u8) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = board_line_cmd(&threadx_riscv64_line(peer_index), binary);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        Self::spawn(cmd)
    }
}

impl Drop for QemuProcess {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
    }
}

/// Parse test results from QEMU semihosting output
///
/// Looks for `[PASS]` and `[FAIL]` markers.
///
/// # Returns
/// (passed_count, failed_count)
pub fn parse_test_results(output: &str) -> (usize, usize) {
    let passed = output.matches("[PASS]").count();
    let failed = output.matches("[FAIL]").count();
    (passed, failed)
}

/// Check if the veth bridge network is available for ThreadX Linux simulation.
///
/// Verifies that the `qemu-br` bridge and at least `veth-tx0` + `veth-tx1`
/// interfaces exist. These are created by `sudo ./scripts/qemu/setup-network.sh`.
///
/// ThreadX Linux uses veth pairs (not TAP devices) because the NetX Duo Linux
/// network driver uses AF_PACKET/SOCK_RAW, which doesn't work correctly on TAP
/// devices with a bridge. veth pairs are purely kernel-side and route traffic
/// through the bridge correctly.
pub fn is_veth_bridge_available() -> bool {
    let bridge_exists = std::path::Path::new("/sys/class/net/qemu-br").exists();
    let veth0_exists = std::path::Path::new("/sys/class/net/veth-tx0").exists();
    let veth1_exists = std::path::Path::new("/sys/class/net/veth-tx1").exists();
    bridge_exists && veth0_exists && veth1_exists
}

/// Fail the test (`unmet!`) if veth bridge is not available for ThreadX Linux simulation
pub fn require_veth_bridge() {
    if !is_veth_bridge_available() {
        crate::unmet!(
            "veth bridge not available for ThreadX Linux \
             (setup: sudo ./scripts/qemu/setup-network.sh)"
        );
    }
}

/// Phase 127.B.5 — check whether the host has a route for the given
/// IPv4 multicast group via the loopback interface.
///
/// QEMU's `-netdev socket,mcast=…,localaddr=127.0.0.1` puts the mcast
/// egress on `lo`, but the kernel only loops the frame back to a
/// sibling QEMU's joined socket if a route for the group is present
/// on `lo`. Without that route the kernel still picks the default
/// route (a real LAN NIC), and cross-process delivery silently fails.
///
/// Returns `true` if `ip route show <group>` reports `dev lo`.
pub fn is_mcast_loopback_route_present(group: &str) -> bool {
    let group_only = group.split(':').next().unwrap_or(group);
    /* `ip route show <addr>` only matches an exact-destination route
     * entry; use `ip route get <addr>` so a prefix route like
     * `230.10.0.0/16 dev lo` is recognised when the test picks
     * `230.10.0.137` from inside it. */
    let out = Command::new("ip")
        .args(["route", "get", group_only])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains(" dev lo ") || s.contains(" dev lo\n") || s.trim_end().ends_with(" dev lo")
        }
        _ => false,
    }
}

/// Phase 127.B.5 — fail with a clear hint (`nros_tests::unmet!`) when the
/// host lacks a loopback route for the test's multicast group.
pub fn require_mcast_loopback_route(group: &str) {
    let group_only = group.split(':').next().unwrap_or(group);
    if is_mcast_loopback_route_present(group_only) {
        return;
    }
    crate::unmet!(
        "host route for multicast group {group_only} not on lo.\n\
         QEMU `-netdev socket,mcast=…,localaddr=127.0.0.1` needs the kernel\n\
         to loop the egress back to the sibling QEMU; without a `dev lo`\n\
         route the default route picks a real LAN NIC and cross-process\n\
         delivery silently drops every peer's frame (Phase 127.B.5).\n\
         One-time host setup (root):\n\
         \n\
         sudo ip route add 230.10.0.0/16 dev lo\n\
         sudo ip route add 239.0.0.0/8   dev lo\n\
         \n\
         To make it survive a reboot, drop the same commands into a\n\
         `systemd-networkd` unit, NetworkManager dispatcher script, or\n\
         `/etc/network/if-up.d/`."
    );
}

/// Check if QEMU RISC-V 64-bit is available
pub fn is_qemu_riscv64_available() -> bool {
    qemu_system_available("qemu-system-riscv64")
}

/// Phase 127.B.5 — does the host's `qemu-system-arm` support
/// `-netdev dgram,local.type=unix,…` (the AF_UNIX peer-pair
/// netdev that replaces the lossy `socket,mcast=` cross-process
/// path)? `dgram` was added in QEMU 7.2.
pub fn qemu_supports_dgram_unix() -> bool {
    qemu_cmd_supports_dgram(qemu_system_arm_cmd())
}

/// `-netdev dgram` support for `qemu-system-riscv64` specifically. The
/// patched ARM binary (stable-11.0) always has it, but the RISC-V64 binary is
/// the unpatched system one — on this host QEMU 6.2, which predates `dgram`
/// (added in 7.2). Checking the ARM binary's help here would be wrong, so the
/// ThreadX-RV64 two-node test gates on this and falls back to `socket,mcast`.
pub fn qemu_riscv64_supports_dgram_unix() -> bool {
    qemu_cmd_supports_dgram(qemu_system_cmd("qemu-system-riscv64"))
}

/// `-netdev help` lists backend types one per line; "dgram" appears iff
/// QEMU >= 7.2.
fn qemu_cmd_supports_dgram(mut cmd: Command) -> bool {
    match cmd.args(["-M", "virt", "-netdev", "help"]).output() {
        Ok(o) => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == "dgram")
                || String::from_utf8_lossy(&o.stderr)
                    .lines()
                    .any(|l| l.trim() == "dgram")
        }
        Err(_) => false,
    }
}

/// Check if QEMU ARM is available
pub fn is_qemu_available() -> bool {
    qemu_system_arm_cmd()
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if ARM toolchain is available
pub fn is_arm_toolchain_available() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("thumbv7m-none-eabi"))
        .unwrap_or(false)
}

/// Check if zenoh-pico ARM library is available
///
/// The BSP examples require the zenoh-pico library to be cross-compiled for
/// ARM Cortex-M3.
///
/// Built by `just qemu build-zenoh-pico` (which runs
/// `scripts/qemu/build-zenoh-pico.sh`), or by `just qemu setup`, which calls it.
///
/// Issue 0483 — this used to say `just build-zenoh-pico-arm`, a recipe that has
/// never existed. All 16 `emulator.rs` tests skipped when the library was
/// absent (before issue 1758), so the suite reported a green that meant "did
/// not run" and the printed remedy could not be followed. `just qemu
/// build-fixtures` does NOT build it, which is how a developer reaches that
/// state without doing anything wrong. Measured after building it by hand: 0
/// passed/16 skipped in ~1 s became 16 passed/0 skipped in 116 s.
pub fn is_zenoh_pico_arm_available() -> bool {
    // Check if the library exists at the expected location relative to project root
    let lib_path = crate::build_dir(crate::kind::QEMU_ZENOH_PICO, &[]).join("libzenohpico.a");
    lib_path.exists()
}

/// Fail the test (`unmet!`) if zenoh-pico ARM library is not available
pub fn require_zenoh_pico_arm() {
    if !is_zenoh_pico_arm_available() {
        crate::unmet!(
            "libzenohpico.a not found — build with `just qemu build-zenoh-pico` \
             (or `just qemu setup`); `just qemu build-fixtures` does NOT build it (issue 0483)"
        );
    }
}

/// A socat-managed virtual serial pair (two linked PTYs).
///
/// Creates a bidirectional PTY pair via `socat`. Data written to one
/// end appears on the other. Both ends are exposed as symlinks for
/// stable paths.
///
/// Use this to wire QEMU's UART0 to zenohd's serial listener without
/// timing races: the PTY pair exists before either side starts, so
/// data is kernel-buffered.
///
/// # Example
///
/// ```ignore
/// let pair = SocatPtyPair::create("/tmp/serial-qemu", "/tmp/serial-zenohd")?;
/// // QEMU connects to /tmp/serial-qemu
/// // zenohd listens on /tmp/serial-zenohd
/// // pair is killed on drop
/// ```
pub struct SocatPtyPair {
    handle: Child,
    /// Path for the QEMU side
    pub qemu_path: String,
    /// Path for the zenohd side
    pub zenohd_path: String,
}

impl SocatPtyPair {
    /// Create a new PTY pair with symlinks at the given paths.
    ///
    /// Blocks until both symlinks exist (up to 5 seconds).
    pub fn create(qemu_link: &str, zenohd_link: &str) -> TestResult<Self> {
        // Remove stale symlinks from previous runs
        let _ = std::fs::remove_file(qemu_link);
        let _ = std::fs::remove_file(zenohd_link);

        let pty_a = format!("PTY,raw,echo=0,link={}", qemu_link);
        let pty_b = format!("PTY,raw,echo=0,link={}", zenohd_link);

        let mut cmd = Command::new("socat");
        cmd.args([&pty_a, &pty_b])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        // Wait for symlinks to appear
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if Path::new(qemu_link).exists() && Path::new(zenohd_link).exists() {
                // Small delay for socat to finish setting up the PTY
                std::thread::sleep(Duration::from_millis(100));
                return Ok(Self {
                    handle,
                    qemu_path: qemu_link.to_string(),
                    zenohd_path: zenohd_link.to_string(),
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Err(TestError::ProcessFailed(
            "Timeout waiting for socat PTY symlinks".to_string(),
        ))
    }
}

impl Drop for SocatPtyPair {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
        let _ = std::fs::remove_file(&self.qemu_path);
        let _ = std::fs::remove_file(&self.zenohd_path);
    }
}

/// The command [`QemuProcess::start_mps2_an385_networked`] runs: MPS2-AN385,
/// LAN9118 on slirp, `-icount shift=auto`.
fn mps2_an385_networked_cmd(binary: &Path) -> TestResult<Command> {
    mps2_an385_cmd_with_nic(binary, &nros_board_common::image_net::slirp_nic("lan9118"))
}

/// The `-nic` value of an image's interface (phase-497 W3), from the network
/// file `nros build` wrote for it (`<build>/nros/net/<image>.toml`):
/// `model` is the board's QEMU NIC model. A missing file is an error, not the
/// default network — the caller asked for an image that states one.
pub fn image_nic(net_file: &Path, model: &str) -> TestResult<String> {
    let raw = std::fs::read_to_string(net_file).map_err(|e| {
        TestError::BuildFailed(format!(
            "{}: {e} — the image's resolved network, written by `nros build`",
            net_file.display()
        ))
    })?;
    let doc: toml::Table = raw
        .parse()
        .map_err(|e| TestError::BuildFailed(format!("{}: {e}", net_file.display())))?;
    let text = doc
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TestError::BuildFailed(format!("{}: no `text`", net_file.display())))?;
    let i = nros_board_common::image_net::primary(text)
        .map_err(|e| TestError::BuildFailed(format!("{}: {e}", net_file.display())))?
        .ok_or_else(|| TestError::BuildFailed(format!("{}: no interface", net_file.display())))?;
    nros_board_common::image_net::qemu_nic(&i, model).map_err(TestError::BuildFailed)
}

fn mps2_an385_cmd_with_nic(binary: &Path, nic: &str) -> TestResult<Command> {
    if !binary.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found: {}",
            binary.display()
        )));
    }

    let mut cmd = qemu_system_arm_cmd();
    cmd.args([
        "-cpu",
        "cortex-m3",
        "-machine",
        "mps2-an385",
        "-nographic",
        // Synchronize virtual clock with wall-clock time. With sleep=on
        // (default), WFI advances virtual time at wall-clock speed via
        // QEMU_CLOCK_VIRTUAL_RT instead of jumping instantly. This keeps
        // hardware timer-backed clocks (CMSDK Timer0) aligned with slirp
        // network I/O timing.
        "-icount",
        "shift=auto",
        "-semihosting-config",
        "enable=on,target=native",
        "-kernel",
    ])
    .arg(binary)
    .args(["-nic", nic])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(unix)]
    set_new_process_group(&mut cmd);
    Ok(cmd)
}

/// Run one command on a QEMU human monitor (`-monitor unix:<socket>,…`) and
/// return what it printed, without the echo and the trailing `(qemu)` prompt.
///
/// Connects per call, so a caller can poll without holding a session open
/// while the guest runs. Fails when the socket does not answer within
/// `timeout` — a QEMU that died, or one not yet listening.
#[cfg(unix)]
pub fn qemu_monitor_command(
    socket: &Path,
    command: &str,
    timeout: Duration,
) -> std::io::Result<String> {
    use std::{io::Write, os::unix::net::UnixStream};

    const PROMPT: &str = "(qemu) ";
    let deadline = Instant::now() + timeout;
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;

    // Read until the n-th prompt: the banner ends in one, the reply in the next.
    let read_to_prompt = |stream: &mut UnixStream, n: usize| -> std::io::Result<String> {
        let mut text = String::new();
        let mut buf = [0u8; 4096];
        while text.matches(PROMPT).count() < n {
            if Instant::now() > deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("QEMU monitor gave no prompt; read so far: {text:?}"),
                ));
            }
            match stream.read(&mut buf) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!("QEMU monitor closed; read so far: {text:?}"),
                    ));
                }
                Ok(k) => crate::capture::append(&mut text, &buf[..k]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(text)
    };

    read_to_prompt(&mut stream, 1)?;
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    let reply = read_to_prompt(&mut stream, 1)?;
    // The monitor echoes the command line (with terminal escapes) before the
    // reply; keep what follows the last line that carries the command.
    let body = reply.trim_end_matches(PROMPT);
    let body = match body.rfind(command) {
        Some(at) => &body[at + command.len()..],
        None => body,
    };
    Ok(body.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_results() {
        let output = "[PASS] test1\n[PASS] test2\n[FAIL] test3\n[PASS] test4";
        let (passed, failed) = parse_test_results(output);
        assert_eq!(passed, 3);
        assert_eq!(failed, 1);
    }

    /// Issue 0930 — the project prefix answers only when its marker names the
    /// version the index pins; a stale or undated one is refused, naming both.
    #[test]
    fn the_project_prefix_must_hold_the_pinned_qemu() {
        let pin = nros_build_paths::sdk_pinned_version("qemu").expect("[tool.qemu] is pinned");
        let d = tempfile::tempdir().unwrap();
        let why = project_prefix_matches_pin(d.path(), "qemu").unwrap_err();
        assert!(
            why.contains("version is unknown") && why.contains(&pin),
            "{why}"
        );
        let mark = |v: &str| {
            std::fs::write(
                d.path().join(".nros-provenance"),
                format!("kind = \"prebuilt\"\nversion = \"{v}\"\n"),
            )
            .unwrap()
        };
        mark("11.0.0-nros0");
        let why = project_prefix_matches_pin(d.path(), "qemu").unwrap_err();
        assert!(
            why.contains("holds 11.0.0-nros0") && why.contains(&pin),
            "{why}"
        );
        mark(&pin);
        assert_eq!(project_prefix_matches_pin(d.path(), "qemu"), Ok(()));
    }

    /// phase-517 W14 — the three launchers boot exactly the line the shared
    /// crate states (the CLI's printed run command asserts the same against
    /// it in `nros-cli-core`'s `cmd::build_next`), with the program from the
    /// ONE ladder.
    #[test]
    fn launchers_boot_the_shared_board_lines() {
        use nros_qemu_ladder::{Board, Net, board_line, threadx_riscv64_mac};
        let kernel = Path::new("/img/nuttx");
        let rendered =
            |line: &nros_qemu_ladder::Line| render_command(&board_line_cmd(line, kernel));
        let expected = |line: nros_qemu_ladder::Line| {
            let mut words = vec![qemu_system_path(line.exe).to_string_lossy().into_owned()];
            words.extend(line.argv("/img/nuttx"));
            words.join(" ")
        };
        let user = Net::User { mac: None };
        assert_eq!(
            rendered(&nuttx_virt_line(Board::NuttxArmVirt, true)),
            expected(board_line(Board::NuttxArmVirt, user))
        );
        assert_eq!(
            rendered(&nuttx_virt_line(Board::NuttxRvVirt, true)),
            expected(board_line(Board::NuttxRvVirt, user))
        );
        assert_eq!(
            rendered(&nuttx_virt_line(Board::NuttxRvVirt, false)),
            expected(board_line(Board::NuttxRvVirt, Net::None))
        );
        let mac = threadx_riscv64_mac(0);
        assert_eq!(
            rendered(&threadx_riscv64_line(0)),
            expected(board_line(
                Board::ThreadxRiscv64,
                Net::User { mac: Some(&mac) }
            ))
        );
    }

    #[test]
    fn test_parse_results_empty() {
        let (passed, failed) = parse_test_results("");
        assert_eq!(passed, 0);
        assert_eq!(failed, 0);
    }

    /// Issue 0877 — the recorded line must name the RESOLVED program, because
    /// that is the half a hand run gets wrong. Asserting only on the arguments
    /// would reproduce the mistake the record exists to prevent.
    #[test]
    fn recorded_command_names_the_resolved_program_not_the_bare_name() {
        let mut cmd = qemu_system_arm_cmd();
        cmd.args(["-machine", "mps2-an385"]);
        let rendered = render_command(&cmd);
        let resolved = qemu_system_arm_path();
        let resolved = resolved.to_string_lossy();
        assert!(
            rendered.starts_with(resolved.as_ref()),
            "recorded line must start with the resolved program `{resolved}`, got: {rendered}"
        );
        assert!(rendered.ends_with("-machine mps2-an385"), "{rendered}");
    }

    /// A pasteable line, not merely a readable one: anything a shell would
    /// re-interpret has to come back quoted.
    #[test]
    fn recorded_command_quotes_arguments_a_shell_would_split() {
        let mut cmd = Command::new("/usr/bin/qemu-system-arm");
        cmd.args([
            "-nic",
            "user,model=lan9118,net=10.0.2.0/24,host=10.0.2.2",
            "-semihosting-config",
            "enable=on,target=native",
            "-append",
            "two words",
        ]);
        let rendered = render_command(&cmd);
        // Commas, slashes and `=` are safe — quoting them would make the line
        // harder to read for no gain.
        assert!(
            rendered.contains("-nic user,model=lan9118,net=10.0.2.0/24,host=10.0.2.2"),
            "{rendered}"
        );
        assert!(rendered.contains("'two words'"), "{rendered}");
    }
}

---
id: 1875
title: "The registration test failed 14 generated entries: it ran ARM firmware as host processes, and asked C/C++ entries for a line only Rust boards print; and C/C++ images dropped every `log`-crate record"
status: resolved
type: bug
severity: medium
area: [testing, c-api]
related: [1819, 1551, 1874]
found: 2026-10-13
---

## Measured

Tier 2 on 2026-10-13: `rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`
failed 14 of 36 entries with "did not reach node registration". It failed the
same way run alone, in 4.6 s.

- **Seven are cross images:** `workspace-c-freertos`, `-c-freertos-realtime`
  (MPS2-AN385), `-c-nuttx`, `-c-nuttx-realtime` (armv7a) and
  `-c-nuttx-riscv-realtime`. This host registers `qemu-arm` in binfmt_misc, so
  `Command::new(<ARM ELF>)` STARTS under user-mode QEMU and dies before its
  session, instead of failing to spawn. #1819's measurement (24/24) came from
  a host that had built no cross row.
- **Seven are host images that worked:** the six `workspace-cpp-native*`
  entries, `workspace-cpp-freertos-posix` and `workspace-mixed-threadx-linux`
  (a `mixed` row: cmake-built, with the C/C++ funnel as its `main`).
  Run by hand against a router, `native_entry` exits 0. Its output contains
  neither `nros: session open` nor `nros: application complete`, the two
  lines the test greps for.

## Cause

Three defects, one of them in the product:

1. **The test ran every located row as a host process,** whatever the
   machine it was built for.
2. **"Completed" was spelled one way for every language.** A Rust board
   prints `nros: application complete` before exiting 0. A C or C++ entry
   returns its funnel's code from `main`, and every funnel
   (`nros_board_{native,rtos}_run_*`) returns non-zero when the session or
   `setup` fails. None prints the Rust board's line.
3. **C/C++ images dropped every `log`-crate record (the product defect).**
   `nros-node` writes `nros: session open`, a stability-contract marker,
   with `log::info!`. A Rust image bridges the `log` crate into the nros-log
   sinks (`nros::main!` calls `install_log_crate_bridge`). A C/C++ image has
   no such call: since issue 1551, `ensure_default_sinks` installs the sinks
   at every C/C++ session init, but not the bridge. So every framework `log`
   record in a C/C++ image reached no console.

## Fix

- **`nros-c`:** a `log` feature (`nros-node/log` plus
  `nros-platform-cffi/log-compat`), turned on by `std` exactly as
  `nros-node/std` turns on `nros-node/log`. Under it, `ensure_default_sinks`
  and `rclc_log_init` also install the bridge. It's a feature rather than
  `cfg(feature = "std")` so the std census does not grow (phase-359), and it
  keeps `log::set_logger`, which needs compare-and-swap, off no_std targets.
- **The test** runs only rows whose ELF class and machine match the host;
  cross rows are left to their QEMU cells and counted in the summary line.
- **The test** accepts a C, C++ or `mixed` row on a zero exit status. A Rust
  row still needs `nros: application complete`. Every `mixed` row is
  cmake-driven with the C/C++ funnel as `main`; the first version of this fix
  left `mixed` out, and `workspace-mixed-threadx-linux` failed the rerun alone.
  Run by hand against a router it exits 0.

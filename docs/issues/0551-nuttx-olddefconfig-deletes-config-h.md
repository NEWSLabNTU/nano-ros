---
id: 551
title: "`make olddefconfig` deletes the NuttX tree's generated `config.h`, and five build inputs were reaching that tree instead of the export snapshot"
status: open
type: bug
area: build
related: [issue-0525, issue-0511, issue-0550, issue-0488, phase-339]
---

## Symptom

`just build-test-fixtures` (lane=all), the `nuttx` platform, every Rust fixture:

```
<repo>/third-party/nuttx/nuttx/include/stdbool.h:30:10: fatal error: nuttx/config.h: No such file or directory
   30 | #include <nuttx/config.h>
```

The NuttX platform is one of the parallel lanes, so this took the whole sweep
to rc=2 while `qemu`, `freertos`, `threadx_linux`, `native` and `zephyr` all
reported OK.

## Cause, part 1 — what deleted the header

`just nuttx build-integration-app` (added for issue 0488 residue 4) mutates the
shared tree's `.config` to set `CONFIG_NROS=y`, and restores it on exit:

```sh
restore() { cp "$backup" "$nuttx_dir/.config"; ( cd "$nuttx_dir" && make olddefconfig ); }
```

`tools/Unix.mk`:

```make
olddefconfig:
	$(Q) $(MAKE) clean_context      # -> $(call DELFILE, include/nuttx/config.h)
```

So the restore puts `.config` back byte-for-byte and leaves the tree
DE-CONTEXTUALIZED. Nothing puts the header back: `build-nuttx.sh` short-circuits
on `HEAD:defconfig-hash` plus the export snapshot, all three of which still
match, so it prints `NuttX arm export up-to-date — skipping build/export` and
no-ops forever.

That is issue 0196's rule — a build-side stale probe watching different inputs
than the consumer needs. The probe watches the SNAPSHOT; these consumers were
reading the TREE.

Fixed by having the restore run `make context` after `make olddefconfig`, which
regenerates the header and the dirlinks without rebuilding.

## Cause, part 2 — why anything cared

Issue 0525 already settled that the shared tree is not a legitimate compile
input: NuttX is built in place, one checkout serves both arches, and
`build-nuttx.sh` says so in as many words —

> The contract is: this path guarantees the SNAPSHOT, never the tree.

— with `nros_build_paths::nuttx_include_root` as the one sanctioned resolution
and `check-nuttx-shared-tree-headers` as its gate. FIVE build inputs were still
reaching the tree, so a header that should have been irrelevant was load-bearing:

| # | site | why the gate missed it |
| --- | --- | --- |
| 1 | `nros-zpico-build/src/runner.rs` — `PathBuf::from(dir).join("include")` | pattern requires a receiver NAMED nuttx; the binding is `dir` |
| 2 | `nros-board-common/src/nuttx_ffi_build.rs` — `PathBuf::from(nuttx_dir).join("include").join("cxx")` | `nuttx_dir)` then `.join` — the `)` breaks the regex |
| 3 | `config/nuttx/nros-platform.toml` — `"{env:NUTTX_DIR}/include"` | a THIRD spelling, neither Rust nor shell; and `.toml` / `config/` were not scanned |
| 4 | root `CMakeLists.txt` — `${NUTTX_DIR}/include`, `${NUTTX_DIR}/include/cxx` | `SHELL_PAT` matches this on sight; the root `CMakeLists.txt` was never handed to the gate |
| 5 | (residual — see below) | cmake-passed include lists into `nros-nuttx-ffi` |

Site 3 is where the reported failure actually lived: the zenoh-pico library's
`-I` list is manifest-driven, so no amount of Rust grepping could have found it.
Site 4's own comment says "Put the NuttX EXPORT include tree on the message-lib
compile" — the intent was already the snapshot; only the code was wrong.

A gate's SCOPE is part of the rule it enforces. This one enforced a rule about
values while grepping for names, over three of the five trees the rule covers.

## Fix

* `just/nuttx.just` — `build-integration-app`'s restore runs `make context`.
* `nros-zpico-build`, `nuttx_ffi_build.rs` — resolve through the accessor.
* `manifest.rs` — new `{nuttx_include}` interpolation token, resolving through
  `nuttx_export::include_root`; `config/nuttx/nros-platform.toml` uses it.
  A token that cannot be spelled wrongly beats a second place to remember a rule.
* `cmake/platform/nano-ros-nuttx.cmake` — `nros_nuttx_include_root(<out>)`, the
  cmake sibling of the Rust accessor; root `CMakeLists.txt` calls it. Arch comes
  from the CARGO TRIPLE first and `CMAKE_SYSTEM_PROCESSOR` second, because the
  workspace fixture lane configures with the HOST compiler
  (`CMAKE_SYSTEM_PROCESSOR` = x86_64) while `Rust_CARGO_TARGET` is
  `armv7a-nuttx-eabihf` — keying on the processor alone silently fell back to
  the shared tree there. Read via `_nros_resolve_rust_target`, never
  `Rust_CARGO_TARGET` directly (phase-155's wrong-arch link).
* `integrations/nuttx/Makefile` — the top-level `$(shell mkdir -p …)` runs on
  EVERY PARSE, including `apps_preconfig`, where `DELIM` and `CONFIG_ARCH` are
  both empty. `NROS_APPS_BUILD` collapsed to `$(APPDIR)external.nros-build` and
  the mkdir created `third-party/nuttx/nuttx-appsexternal.nros-build/` beside the
  apps tree. Guarded on both variables being non-empty; an empty ARCH is also
  the shared-object-dir hazard the coordinate exists to prevent.
* `check-nuttx-shared-tree-headers.py` — taint bindings from `NUTTX_DIR`
  (proximity-scoped, since `dir` is rebound from `FREERTOS_DIR` and
  `ZENOH_PICO_DIR` in the same file), match the manifest spelling, and scan
  `.toml`, `config/` and the root `CMakeLists.txt`. Scope 1365 → 1653 files.
  Self-tests both directions for every new arm.

## Acceptance

* `nuttx/config.h` failures in `just nuttx build-fixtures-arm`: **0**
  (was every Rust fixture, then every C/C++ fixture). Verified 2026-08-13.
* The gate reports each of sites 1–4 when reverted, and is silent on the
  accessor-routed form — self-tested in-file.
* `third-party/nuttx/nuttx-appsexternal.nros-build/` does not reappear.

## Residual — still open

Two things this pass did NOT close:

1. **A fifth site.** With the in-tree header deliberately removed (a stress test
   STRICTER than the real contract), `nros-nuttx-ffi`'s cc build in
   `examples/workspaces/realtime-cpp` still reaches
   `third-party/nuttx/nuttx/include` via include lists cmake passes into it
   (`file_regular` / `file_deferred` in `nuttx_ffi_build.rs`). With a healthy
   tree this is invisible. It should be routed through the accessor like the
   other four.

2. **An unrelated blocker behind it**, newly reachable now that the header
   failure is gone — `realtime-cpp`'s nuttx entry links a HOST archive into an
   ARM image:

   ```
   -L .../nano_ros_cpp_ffi_std_msgs/target/x86_64-unknown-linux-gnu/nros-minsizerel
   ld: libnano_ros_cpp_ffi_std_msgs.a: error adding symbols: file format not recognized
   ```

   That is phase-155's wrong-arch class in the message FFI subprojects, not an
   include-resolution problem. It was masked by this issue and needs its own
   investigation; it should get its own id.

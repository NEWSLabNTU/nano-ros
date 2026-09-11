---
id: 1280
title: "A build in a worktree uses ANOTHER checkout's SDK trees — 19 `sdk-env.just` paths are inherited, absolute, and win over the worktree"
status: resolved
type: bug
area: [build, tooling]
related: [1253, 0491, 0986, 1336, phase-445, phase-454]
---

## What happened

On 2026-09-11 a phase-445 agent ran `just nuttx build-riscv-c` in its own git
worktree (`.claude/worktrees/agent-…`). It reconfigured and rebuilt the NuttX
kernel of the **main checkout** instead: `/home/aeon/repos/nano-ros/third-party/nuttx/nuttx`
went from `CONFIG_ARCH_BOARD="qemu-armv7a"` to `"rv-virt"`, with its `.config`,
`include/nuttx/config.h` and `nuttx` binary rewritten at 01:11:18. Nothing in
the worktree's own `third-party/nuttx` was touched, and nothing said so. The
tree's ARM builds turned out to be insulated: `just nuttx build` in the main
checkout reported `NuttX arm export up-to-date (nros-nuttx-export-arm) —
skipping build/export` and `NOTE: the shared tree stays configured for
"rv-virt", not "qemu-armv7a"` — ARM images link against a per-architecture
EXPORT, and the shared tree's configured board is scratch the scripts already
tolerate. So this incident cost no ARM build. That insulation is specific to
the NuttX kernel, and it does not answer either harm below.

## Why

`just/sdk-env.just` defaults every SDK source path with the inherited value
first:

```just
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "third-party/nuttx/nuttx")
```

and the process that spawns worktree builds already carries those variables as
ABSOLUTE paths into the checkout it was started from. Measured in the session
that hit this — every one of the 19 `env(…, justfile_directory()…)` defaults in
`sdk-env.just` was set, all pointing at the main checkout:

`FREERTOS_DIR`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR`, `NROS_PLATFORM_CFFI_INCLUDE`,
`NROS_PLATFORM_FREERTOS_SRC`, `NROS_PLATFORM_POSIX_SRC`,
`NROS_PLATFORM_THREADX_SRC`, `NROS_VIRTIO_NET_NETX_DIR`, `NROS_C_INCLUDE`,
`NROS_CPP_INCLUDE`, `TBAND_DIR`, `NUTTX_DIR`, `NUTTX_APPS_DIR`, `THREADX_DIR`,
`THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR`, `NROS_ESP_IDF_WORKSPACE`,
`NROS_ESP_IDF_ENV_SHIM`.

So a worktree build compiles the MAIN checkout's FreeRTOS, ThreadX, NetX,
NuttX, platform sources and public headers, beside its own crates. Two harms,
and the second is the worse one:

1. **It writes into another checkout.** The NuttX kernel build is not
   read-only; it reconfigures the tree it is pointed at, so a worktree build
   silently changes the main checkout's state and RACES any build running
   there on the same tree (the per-arch export insulates a finished ARM
   export, not a build in flight).
2. **It certifies the wrong code.** A worktree exists to test a change. An
   edit to `packages/platform/nros-platform-freertos/src` or
   `packages/api/nros-c/include` in the worktree is NOT what gets compiled —
   the build reads the main checkout's copy, links, passes, and reports the
   change as verified. That is issue 1253 exactly (a Zephyr workspace whose
   manifest bound another checkout's module, so the runner "certified
   `/mnt/evo`'s module"), one mechanism over.

CLAUDE.md prescribes linked worktrees for parallel sessions, so this is the
default shape for agent work, not an edge case.

## Fix direction (not decided)

Same answer 1253 chose for the Zephyr workspace: a path that belongs to a
DIFFERENT nano-ros checkout is refused, or re-rooted, never silently used.
Candidates:

- `sdk-env.just` resolves each default against `justfile_directory()` whenever
  the inherited value is inside a different nano-ros checkout (detected the way
  `check-zephyr-workspace-checkout.sh` detects one), and prints the rewrite;
- or a preflight (`check-tier-preconditions` / the lane preflight) refuses with
  a message naming both checkouts, as issue 1253's check does.

A value that points OUTSIDE any nano-ros checkout (a real out-of-tree SDK) must
keep working — the inherited-first order exists for that.

Acceptance: in a linked worktree with all 19 variables exported to the main
checkout, `just nuttx build` either builds the worktree's tree or refuses
naming both paths; and a FreeRTOS example built there compiles the worktree's
`nros-platform-freertos/src` (check with a deliberate `#error` in the worktree
copy).

## A 20th variable, and it is not an SDK path (2026-09-12, phase-454 W3)

`NROS_REPO_DIR` behaves the same way and is not in the 19. Inherited from the
main checkout into a linked worktree, it sent **four gates' fixtures into the
main checkout's `build/`** — so the gates ran, and measured the wrong tree.

This widens the issue's own framing. The title says "SDK trees" and the census
counted `just/sdk-env.just`'s `env(...)` defaults, but the RULE is about any
absolute path inherited from an ancestor checkout that outranks the worktree's
own. An SDK path is the case that was found first, not the boundary — the same
reach-narrower-than-the-rule shape issue 0196 keeps turning up.

Two consequences for the fix direction above:

* the enumeration cannot be "the 19 in `sdk-env.just`". It has to be every
  variable that names a path INSIDE a nano-ros checkout, wherever it is
  exported, or the next one lands the same way.
* `NROS_REPO_DIR` is the sharpest case, because it is the variable that *defines
  which checkout we are in*. Every path derived from it inherits the error, so
  it wins arguments it should never have entered.

Acceptance gains a row: in a linked worktree with `NROS_REPO_DIR` exported to the
main checkout, a gate that writes fixtures writes them under the WORKTREE's
`build/`, or refuses naming both paths.

## Resolution (2026-09-12)

### The census: 24, not 19

Measured live, in an agent worktree, with `env | grep nano-ros`:

| Where it is exported | Count | Names |
| --- | --- | --- |
| `just/sdk-env.just`, `justfile_directory()`-rooted | 21 | the issue's 19, **plus `NROS_LAN9118_LWIP_DIR` and `PX4_AUTOPILOT_DIR`** |
| `just/sdk-env.just`, derived from one of those | 1 | `IDF_PATH` (defaults to `NROS_ESP_IDF_WORKSPACE`) |
| `activate.sh` / `activate.fish`, unconditional | 2 | **`NROS_REPO_DIR`** (the addendum's 20th) and **`nano_ros_ROOT`** |

`FREERTOS_PORT` is the one `sdk-env.just` export that names no path (a port
subdirectory NAME, `GCC/ARM_CM3`); `DIRENV_DIR` carries the root but is
direnv's bookkeeping, not a build input.

The two `activate.*` variables were already correct *when activation ran in the
worktree* — they are unconditional assignments, not `env(…)` defaults. The bug
reaches them in the case that actually happens: a process that never activated,
which is every agent shell that inherited its parent's environment.

### The fix is a rule, not a list

The addendum is right that the enumeration cannot be the answer, so the census
above is evidence rather than the mechanism. The rule, stated once in
`scripts/lib/checkout-paths.sh`, is three-valued:

| the inherited value points at | behaviour |
| --- | --- |
| outside any nano-ros checkout | **KEEP** — a real out-of-tree SDK; this is what env-first exists for |
| a DIFFERENT nano-ros checkout | **RE-ROOT** onto this one, and say so |
| this checkout | KEEP |

"Which checkout does this path belong to" is answered by walking up for the
tree's one checkout marker (`packages/core/nros-core/Cargo.toml`), not by
`.git`: a linked worktree's `.git` is a FILE (issue 1336) and `git rev-parse`
answers about the caller's repository rather than about an arbitrary path. The
walk is purely lexical, so an unprovisioned SDK directory is still attributable
— which matters, because in a worktree those directories are usually absent and
their absence is exactly what made the inherited path look like it worked.

Three spellings, because the three build systems cannot call each other (the
`riscv64` precedent in `nros-build-paths`), pinned to each other by the gate:

1. **shell** — `scripts/lib/checkout-paths.sh`: `nros_checkout_root` +
   `nros_reroot_checkout_path`. `scripts/build/build-root.sh` runs the two REPO
   rungs of `nros_build_root` (`NROS_REPO_ROOT`, `NROS_REPO_DIR`) through it,
   and `scripts/sdk-env.sh` runs every already-set value through it
   so that re-sourcing `activate.sh` in the worktree now actually repairs the
   environment — before this it did not, because `_nros_sdk_env_pairs` prefers
   an already-set value over the default.
2. **`just`** — `just/sdk-env.just` gets the one other checkout the environment
   was inherited from (`scripts/lib/foreign-checkout-root.sh`, ONE subprocess
   per `just` run, ~24 ms) and prefix-rewrites it out of every export with
   `replace(…)`. One prefix covers all of them because they all came from one
   `justfile_directory()`; a value pointing outside any checkout does not
   contain the prefix and survives untouched, which is what keeps row 1 of the
   rule true without anyone maintaining a list. `NROS_REPO_DIR` and
   `nano_ros_ROOT` are not `env(…)` at all any more — `justfile_directory()` IS
   the checkout `just` is running in.
3. **cargo build scripts** — `nros_build_paths::reroot_foreign`, applied by
   `env_or_repo_path` and `env_path`, announcing any rewrite with
   `cargo::warning` (it fires only in the broken case).

Two or more foreign checkouts in one environment cannot be served by a single
prefix rewrite, so that REFUSES, naming every path involved.

### One rung deliberately left alone

`NROS_BUILD_ROOT` is NOT re-rooted, and the reason is the mirror rather than the
rule. `nros_tests::build_root` is the Rust half of `nros_build_root` — a
resolver cannot source a bash function — pinned byte-for-byte to it by
`build_root_derivation.sh`. It reads `NROS_BUILD_ROOT` with no re-root and
cannot be taught one without a new dependency edge, and it cannot have this bug
on its other rung because that one is `project_root()`, a compile-time
`CARGO_MANIFEST_DIR`. Re-rooting only the shell side would make the WRITER and
the READER disagree about where a fixture lives, which is R3's split and a fresh
instance of the bug being fixed. `NROS_BUILD_ROOT` is also not in the inherited
set — nothing exports it, not `activate.sh` and not `sdk-env.just` — so it is
always an explicit operator choice, never an ancestor checkout's leftover. The
gate asserts the pass-through so a later "finish the job" edit has to read this
argument first.

This subsumes two hand-written partial fixes of the same class
(`scripts/build/cargo.sh`'s `nros_sizes_probe_dir`, which validated an inherited
`NROS_REPO_DIR` by probing for a file, and
`scripts/build/compile-check-fixtures.sh`'s hand-pinned `NROS_REPO_ROOT`) —
0196's shape, two sites fixed where the class was live at twenty-four.

### Acceptance, measured

1. **`just nuttx build` in a worktree** — now reports
   `NuttX core: <worktree>/third-party/nuttx/nuttx not provisioned`, i.e. it
   names THIS checkout and declines, instead of reconfiguring the main
   checkout's kernel. (In a worktree the remedy is
   `git submodule update --init third-party/nuttx/nuttx`, not the other
   checkout's tree.)
2. **A FreeRTOS example compiles the worktree's `nros-platform-freertos/src`** —
   an `#error` was put in the worktree's `platform.c` and
   `nros-board-mps2-an385-freertos` built for `thumbv7m-none-eabi` with the
   inherited environment naming the main checkout:

   | resolver | `#error` hits |
   | --- | --- |
   | before (env honoured verbatim) | **0** — the main checkout's copy was compiled |
   | after | **4** |

   Same probe with `NROS_PLATFORM_FREERTOS_SRC` pointing at a genuine
   out-of-tree copy: **0** hits before and after — row 1 does not regress.
3. **`NROS_REPO_DIR` exported to the main checkout** — `nros_build_root` returns
   `<worktree>/build`, and `just check rmw-uorb` builds and passes in
   `<worktree>/build/uorb-check`. That is the "cmake cache generated against the
   main checkout (`source does not match`)" red, and it is what made four
   `check::build` gates measure the wrong tree.
4. **Out-of-tree still works** — `NROS_BUILD_ROOT=/mnt/fast/nros-build` is
   returned unchanged; so is an out-of-tree `NUTTX_DIR` evaluated through `just`
   *beside* an inherited foreign `NROS_C_INCLUDE`, which is the hard shape.

### Gate

`just check inherited-checkout-paths` (`scripts/check-inherited-checkout-paths.py`,
fast lane). Three things:

* **coverage** — every path-valued export in `just/sdk-env.just` carries the
  re-root wrapper (24 today; one declared non-path, with its reason in the
  script). A 25th added without it fails.
* **one marker** — the three spellings of the checkout marker name the same
  file.
* **behaviour** — the shell rule, `nros_build_root` and `just --evaluate` are
  each driven against synthetic checkouts for all three rows of the table.
  Reading the source alone would pass an implementation that never looks at the
  filesystem.

Negative controls, all run and confirmed red:

| mutation | reported |
| --- | --- |
| drop one `replace(…)` from `sdk-env.just` | coverage violation **and** the `just` probe |
| reverse a `replace(…)`'s arguments | coverage violation (self-test) |
| a new unwrapped export | coverage violation (self-test) |
| delete the re-root from `nros_build_root` | both `build-root.sh` REPO probes |
| add one to `NROS_BUILD_ROOT` | the mirror-symmetry probe |

The self-test runs on the normal path every invocation, and its mutations are
measured as a DELTA against the live file — an absolute expectation would make a
real violation read as "the self-test is broken".

### Sweep

```
python3 scripts/check-inherited-checkout-paths.py   # coverage + marker + behaviour
cargo test -p nros-build-paths                      # the rule's five rows, on real dirs
env | grep -i nano-ros                              # the census, in your own shell
```

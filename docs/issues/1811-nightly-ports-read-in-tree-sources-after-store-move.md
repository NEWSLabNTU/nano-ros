---
id: 1811
title: "After phase-484 moved source trees into the store, four nightly RTOS port jobs still look for them in-tree or through retired exports"
status: open
type: bug
area: [ci, build]
severity: high
found: 2026-10-11
related: [phase-484, 1758]
---

## Measured

`nightly` run **38120811245** on `main` @ `90159b5d9` (schedule, 07:13Z
2026-10-11): 8 jobs passed, 5 failed and 3 were skipped. One of the failures
is the tier-2 nightly job, which stopped at the runner-label check (issue
1755's image rebuild, not this issue). The other four are the RTOS port jobs,
and all four fail the same way. Each provisions its sources into the **store**
(`~/.nros/sources/<name>/<version>+<sha8>`, phase-484 W3c `4f7ecbf06`), and a
later step then looks for them at the old in-tree path or through an export
that phase-484 W2c retired:

| Job | Step | Decisive line |
| --- | --- | --- |
| 114415327428 `freertos` | Build (freertos) | `build-examples: line 101: FREERTOS_DIR: unbound variable` |
| 114415327450 `qemu` | `just setup qemu` | `Error: zenoh-pico submodule not found` (printed right after `zenoh-pico … → /github/home/.nros/sources/zenoh-pico/1.7.2+739b1cc4 [provisioned]`) |
| 114415327482 `threadx_linux` | Build (threadx_linux) | `Skipping ThreadX Linux examples (ThreadX/NetX not found)` → `you named threadx-linux, so this is a FAILURE, not a skip` |
| 114415327501 `threadx_riscv64` | Build (threadx_riscv64) | same refusal for `threadx-riscv64` |

`just/freertos.just:41,101` still test `$FREERTOS_DIR/include`, and
`build-zenoh-pico` (`just/…:554`) still requires the in-tree submodule.
`git grep` finds 77 non-comment uses of `FREERTOS_DIR`, `THREADX_DIR` and
`NETX_DIR` across `just/`, `scripts/` and `activate.sh`.

## What it is NOT

- **Not issue 1758's rule misfiring.** The ThreadX jobs FAIL instead of
  skipping because they were named, which is 1758 working as intended. The
  missing tree is real.
- **Not the runner image (1755).** These four jobs run on GitHub-hosted
  runners inside the CI image and all reach their build steps.
- **Not caught earlier**, because no merge-gating lane builds these ports.
  The phase-484 W2c/W3c PRs passed the gate, and the first run that builds
  the ports is this nightly.

## What would close it

Each named recipe finds its tree the way phase-484 says every reader must:
`nros locate source <name>` (cmake / `just` / shell), never an in-tree path or
a retired export. `just/sdk-env.just`'s recipe-scoped values are the
documented exception (phase-484 §"just recipes keep their recipe-scoped
values"), so either `freertos.just` and the ThreadX recipes get their paths
from there, or `build-zenoh-pico`'s presence test asks `nros locate`. A gate
that refuses a new `$FREERTOS_DIR`/`$THREADX_DIR`/`$NETX_DIR` read outside
`sdk-env.just` keeps the class closed. Acceptance is the next 07:13Z nightly
building all four ports.

## Fix (2026-10-12)

Two other changes on `main` fixed most of this while this issue was open, and
they are the fix of record:

- `6c98b7510f` (phase-484 W3c): `nros_locate_sdk_trees` in
  `scripts/build/cargo.sh` sets the four variables through
  `nros_locate_source`, and every `freertos`, `threadx_linux` and
  `threadx_riscv64` recipe that reads one calls it first. So does
  `check-sched-dim-arms-compile.sh`.
- `f7f722a94c` (issue 1833): the `nros-tests` source probes ask the build's
  ladder (`nros_tests::sdk_tree`), and `check-store-source-readers` rejects a
  direct Rust `env::var` read.

This issue adds what those two did not cover:

- **The shell rule.** `check-store-source-readers` now also rejects a `just`
  recipe or script that reads `$FREERTOS_DIR`, `$LWIP_DIR`, `$THREADX_DIR` or
  `$NETX_DIR` without first calling `nros_locate_sdk_trees` or
  `nros_locate_source` (earlier in the same recipe, or earlier in the script).
  It has a self-test. On `main` it reports nothing. Removing the first locate
  call in `just/freertos.just` makes it report 6 reads.
- **zenoh-pico.** `scripts/qemu/build-zenoh-pico.sh` locates zenoh-pico. It
  was allowlisted as "dev: builds zenoh-pico in the checkout", but
  `just setup qemu` runs it and it writes only to `nros_build_dir`. This is
  the `Error: zenoh-pico submodule not found` that stopped the live-peer board
  jobs.
- **The doctor.** `just workspace doctor` asked whether the four variables
  were exported, which reported a correct setup as incomplete. It now asks
  `nros locate`.

## What the first CI runs on the fix showed (2026-10-12)

Nightly run 38215925408 and `live-peer` run 38215928263 ran on the first
version of this fix. Both ThreadX ports built and tested green. FreeRTOS built
every row but one. The `live-peer` board jobs got past `just setup`. Three more
defects came out from behind the first one, and this issue fixes them too:

- **`realtime-c` got no board facts.** The workspace has two bringups
  (`demo_bringup`, `smp_bringup`), and `nros ws board-facts` refused to pick
  one even when the configure named the image (`--deploy freertos`). With no
  board facts, `FREERTOS_PORT` never reached cargo, and `zpico-sys` panicked.
  The verb now lets the named deploy or board choose the bringup. It still
  refuses when the name matches no bringup or more than one, and when no name
  is given. Unit test: `the_named_deploy_picks_the_bringup`.
- **The nightly ran `just <platform> test` unscoped.** `realtime_tiers` runs
  every platform's rows in one test, so the FreeRTOS job booted the NuttX
  riscv and threadx-linux rows too. Their fixtures were never built there, and
  since issue 1758 a missing fixture is a failure. The step now runs
  `just test <platform>`, which sets `NROS_TEST_COORDS` to that platform's
  rows.
- **`just setup <scope>` left the pinned nightly without `rust-src`.** The
  first thing to touch that nightly was `rust-targets`' `rustup +<nightly>
  target add`, which auto-installs the channel with no components. The
  board-zephyr job's `mps2_an385` C talker then failed build-std
  (`…/library/Cargo.lock does not exist`). `rust-targets` now installs the
  channel and the components `tools/rust-toolchain.toml` declares first.

A fourth, the `nros-tests` probes reading the variables directly, was issue
1833's and is fixed on `main`.

Not this issue: the three NuttX Rust e2e cells fail with `ConnectionFailed`,
on `main` as well (issue 1838).

Acceptance is unchanged: a nightly that builds all four ports, and a
`live-peer` run whose board jobs get past `just setup`.

## What the second runs showed (nightly 38230563668, live-peer 38230566873)

The FreeRTOS build passed every row, `realtime-c` included, and the scoped
test step ran only FreeRTOS rows (10 tests, 24 out of lane). One of them
failed: the `realtime_tiers` freertos/rust row booted, printed its heap peak
and never dispatched a tier.

The cause was the workspace cargo road. `nros build` for a workspace image
wrote `nros-cargo.toml` without the board facts, which the single-package road
adds (`NROS_BOARD`, `NROS_BOARD_TOML`). With no `NROS_BOARD_TOML`,
`nros-board-freertos`'s build script fell back to the platform network default
(`192.0.3.10`, gateway `192.0.3.1`). Since phase-484 W4b-2 every FreeRTOS QEMU
image boots on SLIRP (`10.0.2.x`), so the image could not reach its router at
`10.0.2.2`. The C and C++ rows go through cmake, which already resolved the
facts, so they passed. The same row passed on the 2026-10-07 nightly, before the
board-network change.

Fix: both cargo roads now call one helper, `leaf_settings::board_fact_layer`.
The workspace road resolves the facts from the image's bringup. Reproduced
locally on `main` and verified after the fix: the rebuilt image prints both
tiers' dispatch markers. `a_cross_image_carries_its_triple_in_the_settings_file_not_on_the_command_line`
now asserts the two rows and fails without the fix.

The `live-peer` board job now builds with build-std (the `rust-src` fix
works). The next failure behind it is in Zephyr 3.7 itself:
`libc-hooks.c: conflicting types for '__lock___libc_recursive_mutex'` while
compiling the `mps2_an385` C talker. That one is not yet diagnosed.


### 2026-10-12: the libc-hooks failure reproduces

Live-peer run [38291960490](https://github.com/NEWSLabNTU/nano-ros/actions/runs/38291960490)
on main at `a310e0305`. Job 114954544955, "rows whose board is NOT this runner",
step "Build the fixtures those rows resolve". The first fixture the Zephyr make
driver built was `zephyr/mps2_an385/c/talker/zenoh`
(`build-cortex-m-c-talker-zenoh`), and it failed the same way:

```
zephyr/lib/libc/picolibc/libc-hooks.c:112:16: error: conflicting types for '__lock___libc_recursive_mutex'; have 'struct k_mutex'
```

It is followed by matching errors for every `__retarget_lock_*` hook. The same
log shows two Zephyr SDKs in play: the image runs `/opt/zephyr-sdk/setup.sh -c`,
while setup reports `SDK directory: /github/home/.nros/sdk/zephyr-sdk/0.16.8/zephyr-sdk-0.16.8`.
A lead, not measured: the compiler's picolibc headers may come from a different
SDK than the one Zephyr 3.7's `libc-hooks.c` expects. Read the build's
`Found toolchain:` line before trusting it.

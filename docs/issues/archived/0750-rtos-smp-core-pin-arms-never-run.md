---
id: 750
title: "the NuttX / FreeRTOS / ThreadX core-pin arms compile but have never RUN — each blocked on a different thing, and only one is close"
status: wontfix
type: limitation
area: testing
related: [issue-0260, issue-0743, issue-0655, phase-356, phase-296]
---

## What this carries over

[issue 0260](archived/0260-native-dim-kernel-accept-never-exercised.md) closed
2026-08-21 on both halves of its narrowed Direction: every core-pin arm now
COMPILES (`just check sched-dim-arms` type-checks the three RTOS call sites
against their own vendored headers), and one arm RUNS and is OBSERVED on a real
multi-core image —

```
sched-dim arm: [zephyr c CorePinPlacement] ACCEPT
```

on `qemu_cortex_a53/qemu_cortex_a53/smp`, 2 CPUs, asserting `running_on=1`.

This issue is the residual 0260 explicitly declined to carry: **NuttX, FreeRTOS
and ThreadX core-pin arms have still never executed.** Their images are
uniprocessor, so those cells skip. That is board-enablement work, not a
scheduling-dim question, which is why it is its own issue.

## The three are NOT equally blocked

The blanket phrase "three multi-core board bring-ups" is what 0260 left behind,
and it is too coarse to plan from. Measured 2026-08-21:

| RTOS | SMP port available in-tree? | real obstacle |
| --- | --- | --- |
| NuttX | **yes** — `boards/arm/qemu/qemu-armv7a/configs/smp` and `knsh_smp`, plus arm64 and rv-virt variants | the SHARED kernel tree, not the port |
| ThreadX | yes — `ports_smp/cortex_a5_smp` is vendored (the compile gate already type-checks against it) | our ThreadX lane is a **Linux simulator**, not that port |
| FreeRTOS | **no** — see below | no SMP port exists for anything we emulate |

### NuttX — closest, and the plan is B + C below

**Correction (2026-08-21), same day this issue was filed.** The first draft said
the shared kernel tree is the obstacle and listed "second tree / config-identity
stamp / accept serial reconfiguration" as three open options. That framing was
wrong about the hazard, because it missed a layer that already exists.

`scripts/nuttx/build-nuttx.sh` does reconfigure `$NUTTX_DIR` in place from
`$NUTTX_DEFCONFIG`. But **consumers do not link the tree.** phase-339 W2 gives
each build a per-arch SNAPSHOT, `nros-nuttx-export-<arch>/`, carrying its own
`.nros-export-key` — and that key is already `HEAD:sha256(defconfig)`, i.e. it
ALREADY distinguishes two configs of the same arch. The script's own comment
says why: "records what produced it, so freshness never depends on the shared
tree."

So the shared tree is a BUILD-time serialization point, not a consumption
hazard. The only thing missing is granularity: the snapshot DIRECTORY is keyed
on `<arch>`, and `arm` and `arm-smp` are both `arm`, so the two configs would
land in one directory and overwrite each other — while the key file inside
correctly reports a mismatch and forces a rebuild. That is a thrash, loud and
slow, not the silent wrong-kernel the first draft feared.

It is still [issue 0743](archived/0743-nuttx-kernel-path-has-no-arch-discrimination.md)
one level in — `nuttx_kernel_path_for()` reads `e_machine`, and
arm-uniprocessor and arm-SMP share it — so the RESOLVER still cannot tell the
two apart. Both halves want the same fix.

**B — widen the identity from arch to config.** The snapshot dir becomes
`nros-nuttx-export-arm-smp/` beside `-arm`; the id comes from the defconfig's
own directory name (`nuttx-config/<id>/defconfig`), so it is derived, not a
hand-maintained list. `nuttx_include_root()`'s `snapshot_arch` argument becomes
that id — its callers already pass an arch string, so the change is the value,
not the shape. The resolver then asks for a NAMED CONFIG and can check the
snapshot's key/`config.h` (does it actually have `CONFIG_SMP`?) instead of
inferring from `e_machine`, which is the check that cannot work here.

**C — one more lane, shaped exactly like riscv.** `build-fixtures-arm-smp`
gated on `nros_lane_wants_platform nuttx-arm-smp`, mirroring
`build-fixtures-riscv` line for line, so a lane naming no SMP coordinate pays
nothing. `NUTTX_DEFCONFIG` points at the new `nuttx-config/arm-smp/defconfig`.
Then a `[[fixture]]` row and a `CorePinPlacement`-style cell asserting the exact
`running_on=1` line, `AcceptOnly`, the way the Zephyr a53 cell does.

**A — a second NuttX tree — is NOT recommended.** `NUTTX_DIR` is already
env-overridable so the wiring is trivial, and it would buy parallel builds. But
NuttX is a SUBMODULE: a second checkout means either a second submodule entry
(two pins for one upstream, which must move forward together under the
forward-only rule, with `check-submodule-pins` widened to both) or a worktree
the pin gate does not model. That is a permanent drift liability bought to avoid
a serialization cost that C only pays when a lane actually names SMP. It also
buys nothing the snapshot layer does not already provide, now that the snapshot
is understood.

Note also the shipped `qemu-armv7a/configs/smp` defconfig has **no networking**
(`CONFIG_NET=y` absent), and every nros fixture needs a transport, so the
variant is our defconfig + `CONFIG_SMP=y` + core count, not the stock config.
That is the real work here; B and C are small beside it.

### ThreadX — the port exists, the board does not

`tx_thread_smp_core_exclude` type-checks against the vendored
`ports_smp/cortex_a5_smp` headers, so the arm is API-correct. But
`threadx-linux` is the Linux simulation port with the nsos-netx driver, and it
is not that port. Running this arm means bringing up a Cortex-A5 (or A9) SMP
QEMU board with NetX Duo on it — new board, new driver wiring, new fixture
family. Nothing about the existing threadx-linux lane transfers except the
application code.

### FreeRTOS — genuinely blocked, and this is the one to stop looking at

The kernel supports SMP (`configNUMBER_OF_CORES`, 201 references in
`third-party/freertos/kernel/tasks.c`, V11.2.0). The PORTS do not, for anything
we can emulate:

* `portable/ThirdParty/GCC/Posix/port.c` — **0** references to
  `configNUMBER_OF_CORES`, and its `portmacro.h` defines none of the SMP hooks
  (`portGET_CORE_ID`, `vPortYieldCore`, `vPortRecursiveLock`). The phase-370
  freertos-posix simulator board therefore cannot be made SMP by configuration;
* `portable/ThirdParty/GCC/RP2040/port.c` — 36 references, i.e. a real SMP port,
  but it targets Raspberry Pi Pico HARDWARE;
* `mps2-an385` is a single-core Cortex-M3.

So FreeRTOS core-pin acceptance needs either physical RP2040 in the loop or a
multi-core port that does not exist in this tree. Until one of those changes,
the compile gate is the whole of the coverage that is available, and that should
be stated rather than left looking like an oversight.

## Why it still matters

The compile gate closes the "a typo is invisible" hazard, which was 0260's main
worry, and issue 0655 proved that hazard was real — the Zephyr arm could never
have worked and nobody could tell, because the body was never compiled. What the
compile gate does NOT prove is that the kernel HONOURS the request: 0655 was
found by compiling, but "accepted the mask and then ignored it" is only visible
by running and asking which CPU the tier landed on. That is precisely what the
Zephyr `CorePinPlacement` cell asserts (`running_on=1`, exact line, no fallback
arm) and precisely what the other three lack.

## Acceptance

Per RTOS, either:

* a cell in `matrix::CELLS` running on a genuinely multi-core image and
  asserting PLACEMENT (`running_on=N` for N != 0, exact line, `AcceptOnly`), or
* a recorded, specific reason it cannot be, of the FreeRTOS-port kind above —
  not "needs SMP".

And for NuttX specifically, B before C — the snapshot identity widened from
`<arch>` to `<config>` BEFORE a second arm config exists, so the two never share
a directory and the resolver can name what it wants. Building the fixture first
and adding the guard after is the ordering that produced 0743.

## Design (2026-08-22) — the fixture and cell arrangement, after reviewing the build system

Reviewing the seams changed two decisions that looked settled. Recording the
whole arrangement so the remaining work is transcription rather than discovery.

### 1. SMP is a BOARD CONF, not a platform — and a new platform is unavailable anyway

The first instinct was `PlatformId::NuttxArmSmp` with a `nuttx-arm-smp` fixture
token. Wrong twice.

**The precedent says conf.** `workspace-zephyr-c-realtime-smp` — the fixture
that produced the only SMP placement proof in the tree — carries
`platform = "zephyr"` and differs from its uniprocessor siblings only in
`board = "qemu_cortex_a53/qemu_cortex_a53/smp"`. No new PlatformId, no new port
window. That is RFC-0064's rule stated as code: a board is a conf bundle under a
family's recipes, not a namespace of its own.

**And the vocabulary has no room.** `alloc::domain_of` is
`1 + index*21 + slot*3 + lang`; index 10 tops out at 231 and index 11 computes
252, which `domains_valid` rejects. Indices 0–10 are all held by platforms that
BAKE. `PlatformId::index`'s own comment already anticipated this: when the
scheme is full it "needs narrowing, not another renumber". Adding a platform for
an SMP conf would force that narrowing for no gain.

Consequence, already applied: `build-fixtures-arm-smp` gates on
`nros_lane_wants_platform nuttx`, not on a `nuttx-arm-smp` token that no row can
ever spell. It was written the wrong way first and could never have fired.

### 2. Where the Zephyr precedent STOPS transferring

Zephyr's SMP row costs nothing structurally: west builds per board into per-board
directories, so an extra board is an extra directory. **NuttX has one kernel
tree holding one configuration**, so the same row costs a reconfigure.

That is what issue 0750 (B) was for, and it is why B had to land first. The
export is now named for the defconfig directory, so `nros-nuttx-export-arm-smp/`
sits beside `-arm/`, each with its own `HEAD:sha256(defconfig)` key. A fixture
row selects between them with `NUTTX_EXPORT_DIR`, which B also made
authoritative for HEADERS as well as libs — before B, a row could have linked
SMP libs against uniprocessor headers and nothing would have said so.

### 3. The arrangement

**Kernel provisioning** — `just nuttx build-fixtures-arm-smp` (landed). Gated on
the nuttx family; costs one reconfigure per nuttx lane run. Must run before the
manifest rows that consume its export.

**Fixture row** — a `[[workspace_fixture]]` beside `workspace-rust-nuttx-realtime`:

```toml
[[workspace_fixture]]
id = "workspace-rust-nuttx-smp-realtime"
platform = "nuttx"                 # the family token, per §1
lang = "rust"
rmw = "zenoh"
dir = "examples/workspaces/realtime-rust"
bringup = "src/smp_bringup"        # declares `core` on a SPAWNED tier
entry = "nuttx_entry"
target_dir = "target-fixtures/nuttx-smp"   # its OWN dir: a second cargo
                                           # target-dir per config, never shared
target = "armv7a-nuttx-eabihf"
env = { NUTTX_EXPORT_DIR = "<nuttx_dir>/nros-nuttx-export-arm-smp", … }
```

`target_dir` must differ from the uniprocessor row's. Sharing one would put two
different kernels' objects in one fingerprint namespace — the issue-0616 shape,
one layer down.

**Bringup** — `src/smp_bringup` mirroring the Zephyr one: `core = 1` on the
`high` tier, and `high` must be a SPAWNED tier. On Zephyr that was mandatory
(issue 0655: `cpu_mask_mod` rejects a running thread). On NuttX
`pthread_setaffinity_np(pthread_self(), …)` migrates a running thread, so the
boot tier would also work — but only spawned tiers report, and matching Zephyr
keeps one shape across boards.

**Runtime** — QEMU needs `-smp 2`; `QemuProcess::start_nuttx_virt` takes no CPU
count today and needs a variant or parameter. This is the one seam with no
existing shape to copy.

**Cell** — `sched(CorePinPlacement, NuttxArm, Rust, Runtime)` in
`matrix::CELLS`, plus an `exec_for` arm in `sched_dims_applied_e2e` resolving the
new fixture, `boot: NuttxQemu`, `shape: AcceptOnly`, and
`accept: CORE_PIN_OBSERVED_CPU1`. The DIM is the selector that picks the SMP
fixture over the uniprocessor one — exactly how the Zephyr cell distinguishes
`CorePinPlacement` from `CorePin` on one platform.

**Marker** — `ZEPHYR_CORE_PIN_OBSERVED_CPU1` is renamed `CORE_PIN_OBSERVED_CPU1`
(alias kept). The NuttX board prints the identical literal, so a board-prefixed
name would have had a NuttX cell grepping a Zephyr constant — and the next person
to slim the Zephyr banner would have had no way to see the second consumer.

### 4. What is still unproven, and must be proven before the cell

No SMP image has been observed booting. Booting the bare `$NUTTX_DIR/nuttx`
produced no console output on 1 or 2 CPUs — but that is the wrong artifact (the
cells boot per-example images; the bare kernel is 0743's leftover), and the
control never ran because rebuilding arm short-circuited on B's snapshot key.

**Order of work: build the SMP example image, boot it by hand, confirm the
`running_on=1` line appears, THEN write the row and the cell.** A cell asserting
a marker from an image nobody has seen run is a test written against a hope.

## Closed by DEMAND, not by effort (2026-08-22)

The design above is sound and I am not going to build it. The question that
settles it is one nobody asked while the plan was being drawn: **which shipped
device needs this?**

### The census

197 Runtime cells in `matrix::CELLS`:

| platform | Runtime cells | what it actually is |
| --- | --- | --- |
| Linux | 73 | host |
| ZephyrNativeSim | 44 | host simulator |
| ThreadxLinux | 21 | host simulator — not a device |
| FreertosMps2 | 19 | QEMU Cortex-M3, single core |
| NuttxArm | 18 | QEMU Cortex-A7 |
| ThreadxRiscv64 | 10 | QEMU |
| NuttxRiscv | 4 | QEMU |
| ZephyrQemuCortexM | 3 | QEMU |
| FreertosPosix / Esp32Qemu / QemuBaremetal | 2 / 2 / 1 | |
| **Fvp** | **0** | **Cortex-R52 SMP — the board the reference consumer names** |
| Px4 | 0 | CarveOut |

The inversion is the finding. `just/zephyr-setup.just` says phase-292 exists to
prove "the ASI reference consumer's exact shape in-tree —
`nano_ros_use_board(fvp-aemv8r-smp)`". That is an SMP Cortex-R52 board, the
architecture `nros-board-s32z270-freertos` targets in real silicon. It carries
ZERO Runtime cells. A host simulator carries 21.

### Why NuttX SMP is the wrong place to spend

There is no multi-core NuttX device in this tree or in the consumer.
`nros-board-nuttx-qemu` is a Cortex-A7 under QEMU, so an SMP cell there asserts
multi-core placement on a kernel nobody deploys multi-core, via `-smp 2` on an
emulator.

And the cost recurs. The CI tiers are COMPUTED covers, not hand-assigned lists:
tier 2 is 1-wise so every platform appears in it, and tier-2-nightly runs the
realtime-dim set in full. A NuttX SMP cell therefore buys a kernel reconfigure
on every nuttx lane run plus a QEMU boot every nightly, permanently — to prove a
property already proven once, on Zephyr a53, in a cell that runs today.

Coverage is not free and it is not neutral. Every cell is a claim someone
maintains, reruns, and debugs when it flakes; a cell that no device demands is
a standing tax with no payer.

### Disposition, per RTOS

The Acceptance above asked for either a placement-asserting cell on a genuinely
multi-core image, or a recorded specific reason there cannot be one. All three
now have the second:

* **NuttX** — WONTFIX by demand. No multi-core NuttX target exists here. If one
  ever ships, the design section above is the plan and B has already landed the
  part that makes it safe.
* **FreeRTOS** — blocked by port availability. `ThirdParty/GCC/Posix/port.c` has
  zero `configNUMBER_OF_CORES` and none of the SMP hooks; only the RP2040
  (hardware) port has them. Not an oversight, an absence.
* **ThreadX** — needs a Cortex-A5/A9 SMP board bring-up, and no multi-core
  ThreadX device is in scope either. Same demand answer as NuttX.

### What stays, and why

* **B (`22e511442`) stays.** It is a correctness fix in its own right: it closed
  a silent split where `snapshot_root()` honoured `$NUTTX_EXPORT_DIR` and
  `nuttx_include_root()` did not, so one image could link SMP libs against
  uniprocessor headers. Any second NuttX config needs it, SMP or not.
* **The `arm-smp` defconfig and `build-fixtures-arm-smp` stay**, opt-in. Nothing
  chains them — `build-fixtures` calls arm and riscv only — so they cost nothing
  standing, and they are how B was demonstrated: three snapshots coexisting with
  distinct keys is a claim someone can re-run.
* **The board-neutral `CORE_PIN_OBSERVED_CPU1`** stays: the rename is right
  regardless, since two boards print that literal.

### If SMP coverage should grow, it grows toward R52

Not NuttX. The demand is `fvp-aemv8r-smp` / S32Z270, and the obstacle there is
that FVP is licence-gated, which is why it bakes nothing. That is a CI-runner
and procurement question, not a test-design one, and no amount of cell authoring
substitutes for it.

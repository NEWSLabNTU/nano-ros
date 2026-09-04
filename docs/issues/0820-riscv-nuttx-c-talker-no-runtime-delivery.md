---
id: 820
title: "`c_riscv_nuttx_e2e` failed on a MUSEUM BINARY — the NuttX seam had no
  dependency edge on the Rust world, and hardcoded `--release` past a
  miscompile carve-out"
status: open
type: bug
area: cmake, testing
related: [issue-0475, issue-0445, issue-0196, issue-0805]
---

## READ FIRST — the 2026-08-27 fix was live for six days (phase-424, 2026-09-05)

The `DEPFILE` this issue landed was **silently neutralised by issue 0805's shared
cargo dir**, and the verification recipe recorded below cannot see it. Fixed
again here, by retargeting the depfile rather than copying it.

**A depfile names the artifact it describes, and ninja CHECKS that name.**
`DependencyScan::LoadDepFile` compares the rule's target against the edge's first
output; on a mismatch it discards the ENTIRE depfile and marks the edge
`deps_missing_` — permanently dirty.

0805 moved the artifact out of the shared cargo target dir with cargo's
`--artifact-dir`, so the `add_custom_command` OUTPUT became
`<build>/nros-nuttx-ffi-out/nros-nuttx-ffi` while cargo's dep-info kept naming
its own `<shared>/…/nros-minsizerel/nros-nuttx-ffi`. The seam copied that file
across verbatim, target line included.

Measured on the riscv leaf's REAL depfile (`build-zenoh/CMakeFiles/d/93e9….d`,
fed to ninja 1.13.2 with the edge's real output name):

```
ninja explain: expected depfile 'real.d' to mention
  'nros-nuttx-ffi-out/nros-nuttx-ffi', got
  '/home/…/build/corrosion-cargo/nuttx-riscv/682805377484/
   riscv32imac-unknown-nuttx-elf/nros-minsizerel/nros-nuttx-ffi'
[1/1] Building NuttX example: c_talker      <- and again on every subsequent run
```

So all **308** nano-ros Rust paths in that depfile were read by nobody, and the
seam had silently become the **always-run custom target** this issue explicitly
rejected on cost grounds ("pays a cargo + NuttX kernel invocation on every build
of every NuttX example"). It was still correct — but by accident, not by an edge.

**Why the recorded verification could not catch it.** "Touch
`nros-node/src/lib.rs`, rebuild without wiping, confirm the ELF hash changes"
passes identically whether the DEPFILE works or the edge is permanently dirty.
The recipe proves the artifact is not a museum binary; it does not prove the
mechanism that would keep it that way is alive. A no-op rebuild reporting
`ninja: no work to do` is the check that separates them.

Note also that the two branches differ: when `nros_shared_cargo_dir` is
unavailable, `_output_binary` IS cargo's own path and the depfile matched. That
is the branch the 2026-08-27 verification ran, which is why it was honest then
and stale six days later.

**Fix (2026-09-05).** `packages/api/nros-c/cmake/nros-nuttx-depfile.cmake` — a
`cmake -P` script that rewrites the rule's target to the
`add_custom_command` OUTPUT before cmake's `cmake_transform_depfile` runs;
`nros-nuttx.cmake` invokes it in place of the `copy_if_different`. The module dir
is captured `CACHE INTERNAL` at file scope because — measured —
`CMAKE_CURRENT_LIST_DIR` inside a cmake `function` resolves to the CALLER's list
dir, not the defining file's.

Measured after the fix, on the same real depfile:

* the script retargets it and preserves all 308 prerequisites;
* `cmake -E cmake_transform_depfile Ninja gccdepfile …` renders the target as
  `nros-nuttx-ffi-out/nros-nuttx-ffi:` — ninja's exact output spelling;
* ninja accepts it (no `expected depfile` explain line) and, in a controlled
  positive control, reports `no work to do` on a no-op rebuild and rebuilds when
  a depfile-named source is touched.

**Not verified:** an in-situ incremental `just nuttx build-riscv-c` (needs a full
NuttX kernel + cross cargo build). And cargo dep-info may name paths that do not
exist; ninja degrades those to always-rebuild — no worse than the state this
replaces, never a museum binary — but whether the current dep-info contains any
is unmeasured.

This changed no probe's watch set, so phase-424's 0835 constraint does not bind.

## Resolution of the reported failure: stale artifact, not a code defect

`c_riscv_nuttx_talker_delivers_cross_process` **passes in 3.5 s** after
`rm -rf examples/qemu-riscv-nuttx/c/talker/build-zenoh` and a rebuild, on
unmodified sources. It failed at the full 90 s timeout before that.

The binary the tier-2 fixture build left behind published on ROS domain **1**.
The same leaf, rebuilt clean from the same commit, publishes on domain **0** —
which is what the test's listener subscribes to, and what the sources say:

```
[probe] getenv=(null) NROS_ENTRY_DOMAIN_ID=0 domain_id=0
[probe] resolve_session_and_domain single: support=0 session=0 -> 0
```

Same guest, same two listeners, before and after the wipe:

| listener | museum binary | after rm -rf + rebuild |
| --- | --- | --- |
| `ROS_DOMAIN_ID=0` (what the test uses) | 0 received | **24 received** |
| `ROS_DOMAIN_ID=1` | 5 received | 0 received |

The only change between those two columns is the wipe. So the old ARCHIVE code
linked into that image behaved differently from the archive code in the tree.

## Why this stays open

**Issue 0475 is marked resolved** (phase-209: `LINK_DEPENDS` on the consuming
target, so a backend `.a` gains a real rebuild edge instead of an order-only
one). This leaf reproduced 0475's symptom anyway, which means either the fix
does not reach `examples/qemu-riscv-nuttx/c/talker` / the `just nuttx
build-riscv-c` path, or the tier-2 fixture build reached the binary another way.
That is the open question, and it is the one worth answering: a museum binary
here is indistinguishable from a code defect until someone wipes the directory.

Verify with the 0475 recipe: `ninja -C <build-dir> -t query <exe>` — the RMW
`.a` must appear under `|`, not `||`, and touching a backend source must relink.

## Where the 0475 gap actually is (measured 2026-08-27)

`ninja -C examples/qemu-riscv-nuttx/c/talker/build-zenoh -t query c_talker`, on a
freshly built tree:

```
c_talker:
    | libbuiltin_interfaces__nano_ros_c.a     <- real edge
    | libstd_msgs__nano_ros_c.a               <- real edge
    || c_talker_build                         <- ORDER-ONLY, phony
```

The message archives carry 0475's `LINK_DEPENDS`. The backend does not appear at
ALL. `nros_rmw_zenoh` is not a cmake target in this build (`ninja -t targets`
confirms), so `nano_ros_link_rmw`'s

```cmake
if(TARGET nros_rmw_${_chosen})
    set_property(... LINK_DEPENDS "$<TARGET_FILE:nros_rmw_${_chosen}>")
endif()
```

takes the silent `else` branch. The archive arrives instead through a cargo
CUSTOM_COMMAND (`cargo-target/riscv32imac-unknown-nuttx-elf/release/
nros-nuttx-ffi`) reached only via the phony, order-only `c_talker_build`.

**That absence is not itself the bug, and this is the part worth getting right.**
Per `cmake/NanoRosRmwDispatch.cmake`, only some backends expose a cmake target:

| rmw | `NROS_RMW_EXTRA_LINK_LIBS` | `nros_rmw_<x>` target |
| --- | --- | --- |
| cyclonedds | `nros_rmw_cyclonedds;ddsc;stdc++` | YES — 0475 was verified here |
| uorb | `nros_rmw_uorb` | YES |
| zenoh | `""` (RLIB_DEP `nros-rmw-zenoh`) | NO, by design |
| xrce | `""` (RLIB_DEP `nros-rmw-xrce-cffi`) | NO, by design |

zenoh and xrce compile the backend INTO the nros-c umbrella staticlib, so there
is no separate archive to depend on and the edge has to come from however the
UMBRELLA is linked. 0475 was diagnosed, fixed and verified on the cyclonedds
leaf, which is exactly the arm that HAS a target — so the fix is correct where it
was tested and simply does not reach the other two shapes.

So the open question is narrower than "0475 regressed": **does the umbrella
staticlib have a file-level edge on each platform, for zenoh and xrce?** On
nuttx it demonstrably does not (order-only phony, above). Whether a NATIVE
zenoh/xrce leaf has one is NOT yet measured, and it decides whether this is a
nuttx wiring bug or a two-backend hole.

A first attempt at a fix — warning from `nano_ros_link_rmw` whenever the target
is absent — was written and REVERTED before commit: it would fire on every
native zenoh/xrce build, where the absence is by design and there may be no
hazard at all. A warning that cries on correct builds gets muted, which would
leave this worse than it is now. Fix the umbrella edge, or detect the
order-only case specifically.

## ROOT CAUSE of the museum binary, located (2026-08-27)

`packages/api/nros-c/cmake/nros-nuttx.cmake:274`, the command that runs
`cargo build` to produce the NuttX kernel ELF:

```cmake
add_custom_command(
    OUTPUT "${_output_binary}"
    ... cargo build --release
    DEPENDS "${_NNBE_MAIN_SOURCE}" ${_NNBE_SOURCES} ${_NNBE_INTERFACE_SOURCES}
            "${_includes_file}" "${_ffi_libs_file}"
            "${_NNBE_FFI_CRATE_DIR}/build.rs"
            "${_NNBE_FFI_CRATE_DIR}/Cargo.toml"
```

The DEPENDS list carries the app's C sources and the FFI crate's `build.rs` /
`Cargo.toml`. **It names no nano-ros Rust source and no nano-ros archive.** So
an edit to `packages/core/nros-node`, `packages/api/nros-c` or any backend
leaves this command up to date, cmake skips it, cargo is never invoked, and the
ELF keeps the previous build's Rust code with a fresh mtime. Museum binary.

The `add_dependencies` calls immediately below do not save it, twice over:

* `add_dependencies` is ORDER-ONLY by construction — 0475's whole lesson.
* `foreach(_dep cargo-build_nros_c cargo-build_nros_cpp)` is guarded by
  `if(TARGET ${_dep})`, and **neither target exists in this build**
  (`ninja -t targets` in the riscv build dir lists no `cargo-build_nros_*` and no
  `libnros_c.a`). On NuttX the whole nano-ros Rust side is compiled INSIDE the
  `nros-nuttx-ffi` cargo build, so there are no separate archives to depend on.

## The native leaf is correctly wired — this is NOT a zenoh/xrce-wide hole

Measured, because an earlier revision of this issue speculated it might be:

```
$ ninja -C examples/native/c/talker/build -t query c_talker
    | nano_ros/packages/api/nros-cpp/libnros_cpp.a          <- real edge
    | nano_ros/nros_platform_posix_build/libnros_platform_posix.a
    || nano_ros/packages/api/nros-c/cargo-build_nros_c      <- order-only
```

The archive the native C talker actually links is `libnros_cpp.a` (it defines
`nros_support_init`; confirmed with `nm`), and it HAS a file-level edge. A
backend change rebuilds it and relinks the example. So zenoh/xrce on native are
fine, and the defect is specific to the NuttX seam above.

## FIXED for the NuttX seam (2026-08-27) — verified without wiping

`packages/api/nros-c/cmake/nros-nuttx.cmake`, all three together:

```cmake
nros_resolve_carve_out_profile(nuttx-rust _NROS_NUTTX)
set(_output_binary ".../${_NROS_NUTTX_DIR}/nros-nuttx-ffi")
    cargo build --profile ${_NROS_NUTTX_PROFILE}
    DEPFILE "${_output_binary}.d"
```

Verification — the one that matters is a rebuild WITHOUT `rm -rf`:

| step | result |
| --- | --- |
| clean build | cargo dir moved `release/` -> `nros-minsizerel/` |
| touch `packages/core/nros-node/src/lib.rs`, rebuild | `[1/2] Building NuttX example: c_talker` ran |
| ELF sha256 | `7bdbba87d389e7ad` -> `f2b6e8107a94260e` |
| probe symbol in ELF | present |
| `c_riscv_nuttx_talker_delivers_cross_process` | PASS 3.4 s |

**The profile half was not cosmetic.** `NUTTX_RUST_PROFILE`'s docstring: at
`lto = "off"` a non-deterministic cross-CGU miscompile corrupts the std
`lang_start` main-closure fat pointer and the image reboots before `main` with
no console output (phase-177.8.c). Cargo's built-in `release` IS `lto = off`.
So this seam had been building every NuttX C example in the configuration the
tree documents as broken, while `nros profile carve-out nuttx-rust` — which
exists to prevent exactly that — sat unused.

### The sibling seams, swept (2026-09-01) — this one IS one-of-a-kind

The open item assumed "freertos / threadx / esp-idf are the same custom-command
construction". They are not, and that is the answer rather than a reprieve.

**The missing edge.** Every tracked cmake file that contains both
`add_custom_command` and `cargo`:

    cmake/NanoRosCodegenCore.cmake                      codegen tool
    cmake/NanoRosCorrosion.cmake                        corrosion's own targets
    cmake/NanoRosGenerateInterfaces.cmake               codegen
    cmake/NanoRosNodeRegister.cmake                     codegen
    packages/api/nros-c/CMakeLists.txt                  config-header mirror
    packages/api/nros-cpp/CMakeLists.txt                config-header mirror
    .../NrosRmwCycloneddsTypeSupport.cmake              codegen
    zephyr/cmake/nros_generate_interfaces.cmake         codegen
    packages/api/nros-c/cmake/nros-nuttx.cmake          DEPFILE x3   <- the seam

`nros-freertos.cmake` and `nros-threadx.cmake` contain no `cargo` reference at
all; they reach Rust through corrosion, which creates real targets with real
file-level edges. So NuttX is the only place a custom command drives a cargo
build of nano-ros Rust, which is exactly why it was the only place the edge went
missing — the construction, not the platform, is what carries the hazard.

**The hardcoded profile.** `just freertos` does NOT hardcode: it passes the
`freertos-qemu` carve-out through `NROS_CARGO_PROFILE`, with the reason written
at the call site (`just/freertos.just:115`). The remaining `--release` literals
are `NanoRosCodegenCore.cmake:427` (a HOST codegen tool, no carve-out applies)
and `integrations/px4/NanoRosPx4Module.cmake` (no px4 carve-out is declared, so
nothing contradicts it).

**One real remnant, fixed here:** this file's own docstring still said
"Schedules a `cargo build --release`" — the very text the fix below replaced in
the code. A comment contradicting the code it documents is how the next reader
re-derives the wrong thing, and it is the same shape as the NuttX settle-delay
contradiction recorded further up this issue.

So the class is closed by measurement rather than by fixing more sites: there
were no more sites. What would keep it closed is a gate asserting that a cargo
build driven by a custom command carries a `DEPFILE` — not written, because with
exactly one instance it would be a rule inferred from a single example, and the
codegen commands legitimately have no depfile, so the predicate would be
guesswork.

### A note on method, because it cost most of the investigation

Four times here an ABSENT signal was read as evidence, and each nearly produced
a wrong root cause:

* `nros_log` output missing -> the image installs no log sink.
* A probe string missing from the ELF -> that IS the museum binary. Check with
  `strings <elf> | grep <probe>`.
* A probe silent again -> no router running, so `nros_support_init` returned -4
  and entity creation was never reached.
* A `#[allow(dead_code)] const` probe missing -> dead-code-eliminated before it
  reached the binary. Use `#[used]` + `#[unsafe(no_mangle)]`.

The domain-1 behaviour, the "domain mismatch root cause" and the issue-0801
comparison in this issue's history were all artifacts of that class. When a
check produces nothing, establish that the instrument works before believing
the reading.

## THE PROPER FIX (explored 2026-08-27) — three coupled defects in one seam

The missing edge is not the only thing wrong with this custom command, and the
other two are why a naive fix breaks the build.

### 1. The profile is hardcoded, and it contradicts the platform's own

```cmake
cargo build --release                                     # line 288
set(_output_binary ".../${_NNBE_TARGET_TRIPLE}/release/nros-nuttx-ffi")   # line 229
```

`nros-cargo-profile` declares `NUTTX_RUST_PROFILE = MINSIZEREL.name` =
**`nros-minsizerel`**, and `platform_profile("nuttx"|"nuttx-riscv")` returns it.
The Rust lane honours that (`armv7a-nuttx-eabihf/nros-minsizerel/nuttx_entry`).
This C lane builds `release` instead. Measured: the riscv leaf's cargo dir
contains ONLY `release/`.

phase-336's `NanoRosCargoProfile.cmake` exists to resolve one profile "for
everything cmake builds through Corrosion **or a custom command**". This is a
custom command and it never asks. The file does not even include the module (it
includes only `nros-rtos-helpers.cmake`).

Consequences, in the order they bite:

* **Size, on the platform that picked minsizerel for a reason.** NuttX images
  get `release` codegen where the tree says size-optimised.
* **`CMAKE_BUILD_TYPE` does not reach the Rust half at all.** Configure Debug
  and the C side is `-O0` while the Rust side is still `release`; a debuggable
  NuttX image is unobtainable.
* **Two profile dirs for one platform.** The Rust lane writes
  `nros-minsizerel/`, this lane writes `release/`, so shared crates compile
  twice and neither reuses the other — the 0488 shape.

### 2. The hardcoded OUTPUT path makes the profile hardcode LOAD-BEARING

`_output_binary` names `release/` literally. Pass `-DNROS_CARGO_PROFILE=...`
and cargo writes elsewhere while cmake still expects `release/`; the 0159 guard
then fires `NuttX cross-link produced no kernel ELF`. So the profile cannot be
fixed without the path — they move together or the build breaks loudly.

### 3. The missing edge — and cargo already solved it

Cargo writes a dep-info file next to the binary: `nros-nuttx-ffi.d`, Makefile
format, absolute paths, and it lists **159 nano-ros Rust sources** (verified —
`packages/core/nros-node/src/c_waker.rs` is in there). CMake consumes exactly
this shape:

```cmake
add_custom_command(
    OUTPUT  "${_output_binary}"
    DEPFILE "${_output_binary}.d"
    ...)
```

Prerequisites are already met: `cmake_minimum_required(VERSION 3.22)`, cmake
3.22.1, `CMAKE_GENERATOR=Ninja` (DEPFILE has worked with Ninja since 3.7; the
3.20 requirement is for Makefiles). A missing depfile on the first build is
tolerated.

This beats both alternatives that were on the table:

* **vs. hand-listing Rust sources in DEPENDS** — that is what is there now, and
  it is a hand-maintained approximation of a graph cargo computes exactly. It
  will drift again the moment a crate is added.
* **vs. an always-run custom target** (the previous suggestion in this issue) —
  correct but pays a cargo + NuttX kernel invocation on every build of every
  NuttX example. DEPFILE gets the same correctness for the cost of reading a
  file cargo already wrote.

### Sequencing

All three land together, because (1) alone breaks on (2), and (3) alone leaves
NuttX images silently mis-profiled. Then verify by the recipe below — touch
`packages/core/nros-node/src/lib.rs`, rebuild WITHOUT wiping, and confirm the
ELF's hash changes.

**Check the siblings in the same pass.** `FREERTOS_QEMU_PROFILE` is also
`MINSIZEREL`, and the freertos/threadx/esp-idf seams are the same
custom-command shape. A missing edge is rarely one-of-a-kind, and neither is a
hardcoded `--release`.

## Superseded: the earlier suggestion

## Suggested fix, and why it is not applied here

Cargo is authoritative about its own inputs and is fast when up to date. The
structural fix is to stop asking cmake to predict them: attach the cargo
invocation to the always-run custom TARGET rather than gating it behind an
OUTPUT whose DEPENDS list has to enumerate the Rust world (which is what went
wrong — the list is a hand-maintained approximation of a dependency graph cargo
already computes).

Not applied in this commit because it makes cargo (and the NuttX kernel build
behind it) run on every build of every NuttX example, and that cost needs
measuring before it is imposed on the freertos/threadx/esp-idf seams that
likely share the shape. Whoever takes it should check those three first — the
bug is a missing edge, and missing edges are rarely one-of-a-kind.

Verify any fix with: touch a `packages/core/nros-node` source, rebuild WITHOUT
wiping, and confirm the ELF changes (`strings`/`sha256sum`), not merely that the
build exits 0.

## The staleness probe cannot see this class, and I asserted otherwise

An earlier revision of this issue said "**Not a stale fixture**" and justified it
by mtime: the talker was rebuilt 2026-08-26 23:54, after the tree's last source
edit. That reasoning is structurally void for this failure mode. A 0475 museum
binary is NEWER than its sources while containing older archive content —
there is no dependency edge, so nothing moves the mtime. **An mtime freshness
check cannot detect the very class it was invoked against**, and reporting its
result as evidence is the same shape as issue 0445's absorbing STALE verdict,
one level down.

Everything that was built on that claim was wrong with it: a "domain mismatch
root cause", a comparison to issue 0801, and a suspected sentinel defect at
`packages/api/nros-c/src/node.rs` (`if support_domain != 0`, where 0 is both a
legal ROS domain and the unset marker). That sentinel ambiguity IS real code and
now has its own issue — [[issue-0972]], filed 2026-09-01, still live at
`node.rs:739` — but it is NOT what broke this test — the native C
talker, same source, resolves to domain 0 with no split, and the instrumented
riscv image now does too.

## Recorded for whoever picks this up

* `domain_of(NuttxRiscv, C, Pubsub)` = **86**, measured. The `1` came from
  neither the allocator nor any cmake define (`build.ninja` bakes only
  `NROS_ENTRY_LOCATOR`), which is consistent with it coming from archive code
  that predates the current resolution.
* Instrumenting this image is harder than it looks and three separate NULL
  results each nearly read as "the branch never runs": `nros_log` output goes
  nowhere (the image installs no sink), a probe string can be absent from the
  ELF (that IS the museum binary — check with `strings <elf> | grep <probe>`),
  and with no router running `nros_support_init` returns `-4` so entity creation
  is never reached at all.

## Symptom

Tier 2 (`just ci-matrix`), 2026-08-27. Of 1704 tests, two failed; one
(`test_qemu_rtic_service_e2e`) passes solo and is the usual in-sweep QEMU flake.
This one does not.

```
nros-tests::c_riscv_nuttx_e2e c_riscv_nuttx_talker_delivers_cross_process
  panicked at packages/testing/nros-tests/tests/c_riscv_nuttx_e2e.rs:94:13:
  native listener never received the riscv-nuttx C talker's /chatter — the
  riscv C-lane runtime delivery did not work (archived 0199 fixed the link;
  this is the runtime half)
```

Reproduced SOLO twice, `--retries 0`, 90.3 s each (the test's own
`wait_for_output_count(..., 3, 90s)` budget). The test already carries
`retries = 1` in `.config/nextest.toml` and failed in-sweep with it.

## ROOT CAUSE: a domain mismatch, proven by experiment (2026-08-27)

The guest publishes on ROS domain **1**; the test's native listener subscribes
on domain **0**. The domain is the FIRST segment of every rmw_zenoh keyexpr, so
the subscription can never match the publication — no error anywhere, which is
why this looked like a transport failure.

One run, one guest, two listeners differing only in `ROS_DOMAIN_ID`:

| listener | subscribes to | result |
| --- | --- | --- |
| `ROS_DOMAIN_ID=0` (what the test does) | `0/chatter/std_msgs::msg::dds_::String_/*` | nothing |
| `ROS_DOMAIN_ID=1` | `1/chatter/...` | **`I heard: [Hello World: 1..5]`** |

The guest published normally throughout (`Publishing: 'Hello World: N'`). So the
image is healthy and the riscv C lane's runtime delivery WORKS — the test asserts
against a listener on the wrong domain.

## The guest splits its own domain mid-session

From the router (`ZENOHD_LOG=debug`), one ZID, in order:

```
Declare token 1  @ros2_lv/0/<ZID>/0/0/NN/%/%/node        <- domain 0
Undeclare token 1
Declare token 4  @ros2_lv/1/<ZID>/0/0/NN/%/%/talker      <- domain 1
Declare token 5  @ros2_lv/1/<ZID>/0/3/MP/%/%/talker/%chatter/...
Declare interest   1/chatter/std_msgs::msg::dds_::String_/...
```

The default node created by `nros_support_init` lands on 0; the node created by
`nros_node_init` and its publisher land on 1. That is a single session using two
domains — **the exact shape of issue 0801**, which is marked resolved. 0801 was
the mirror image (node token on the configured domain, entities on 0); this is
the same defect with the operands swapped, so 0801's fix traded one direction
for the other rather than removing the ambiguity.

The suspect mechanism, from `packages/api/nros-c/src/node.rs`:

```rust
let support_domain = support_mut.domain_id as u32;
let domain_id = if support_domain != 0 { support_domain } else { session.domain_id() };
```

`0` is a legitimate ROS domain AND the "unset" sentinel, so a caller that
resolves to domain 0 has its answer discarded. The C ABI already has a distinct
spelling for this (`DOMAIN_ID_EXPLICIT_ZERO_C_ABI` = 255 →
`baked_domain_from_c_abi` → `Some(0)`), and the example does not use it: it
passes plain `0`, because `nros/app_main.h` defaults `NROS_ENTRY_DOMAIN_ID` to
`0` and the fixture row bakes no domain at all.

## What is NOT established

* **Where the value `1` comes from.** Not the allocator (`domain_of(NuttxRiscv,
  C, Pubsub)` = **86**, measured). Not a cmake define (`build.ninja` bakes only
  `NROS_ENTRY_LOCATOR`). `support.domain_id` resolves to 0 and
  `open_session(..., support.domain_id, ...)` opens the session with that same 0,
  so `session.domain_id()` should also be 0. Reading the code predicts 0 at every
  step and the wire says 1, three times over — so one of those steps does not do
  what it reads like, and the next move is to instrument rather than re-read.
* **Whether the fix belongs in the fixture row, `app_main.h`, or node.rs.** All
  three are implicated; picking before the `1` is explained would be guessing.
  Note the sibling riscv Rust rows DO set `NROS_DOMAIN_ID = "0"` explicitly and
  this C row sets nothing, which is suggestive but does not by itself produce a 1.
* **Whether it fails on origin/main.** Unchanged from below — still not run.

## Earlier findings (still true)

* **Not a stale fixture.** The riscv C talker was rebuilt 2026-08-26 23:54 by the
  `lane=tier2` build, after the tree's last source edit.
* **The listener side starts.** The panic is at `wait_for_output_count`, not the
  readiness wait.
* **Not an in-sweep flake.** Fails solo on an idle host; its sweep sibling
  `test_qemu_rtic_service_e2e` passes solo.
* **phase-384 W1 is not implicated.** Its six edits are all on RECEIVE paths; the
  talker is a C publisher. The domain evidence above now makes this moot.

## Reproduce

```
source ./activate.sh
just build-test-fixtures lane=tier2      # or: just nuttx build-riscv-c
cargo nextest run -p nros-tests --test c_riscv_nuttx_e2e --retries 0 \
    -E 'test(c_riscv_nuttx_talker_delivers_cross_process)'
```

Needs `zenohd` and `qemu-system-riscv32`; the test skips cleanly without them.

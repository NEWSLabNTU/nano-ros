# Phase 471 — build-script classes

**Status (2026-09-29). ALL WORK ITEMS LANDED — W0, W1, W2, W3, W4, W5, W6.** A design study of the 66
cargo build scripts: what they are actually FOR, where they are genuinely
fragmented, and where an "outlier" is a legitimately different job the old
taxonomy was mis-describing. W0 replaced the census's capability letters with
derived ROLES and fixed the population they were counted over; W1 landed the one
defect the study measured (issue 1527). Everything structural is W2 onward,
because the study's first finding is that **the converged shape already exists**
and the work is migration, not design.

**W2 LANDED 2026-09-29** (steps 1–2; step 3 refused with a measurement, see
below). It found and fixed issue 1561 — `nros-board-mps3-an536-freertos` could
not compile its own board C file from cargo at all — and filed issue 1562, the
same duplication class one family over.

**Prior:** phase-468 W3 (`check-board-build-wiring`, the shared build crate as a
rule), issue 1280 (an inherited path outranks the checkout being built), issue
0491 (watch the CONTENT, never the spelling), RFC-0049 (the knob ladder),
RFC-0064 (board organization), [docs/reference/canonical-build-path.md](../reference/canonical-build-path.md).

**Source:** a census of every tracked `build.rs`, run on `main` at `e3de8bda1`
on 2026-09-28. Every number below is reproducible with
`python3 scripts/nros-build-wiring.py --scripts`; none is authored here.

---

## W0 — the census was measuring the wrong things (LANDED)

Two defects in the instrument, both in the same direction: it described the tree
somebody grepped rather than the tree.

### The population was wrong

`tracked("*/build.rs")` is a git pathspec matching **any** file named
`build.rs`. That admitted `packages/cli/nros-cli-core/src/cmd/build.rs` — the
`nros build` COMMAND, a 4383-line Rust module, **30 % of every line the census
weighed** — as a build script. It was even given capability letters (`CBD`),
read off prose in its own doc comment.

A cargo build script is a `build.rs` beside a `Cargo.toml`. With that, the
population is **66**, not 67.

### A capability letter is a grep for a TOOL, and a tool is not a role

The old letters were `C` (`cc::Build`), `K`, `E`, `B`, `P`, `G`, `A`, `D`. Two
measurements are enough to retire them:

* `C` put `nros-platform-cffi` — three first-party C files compiled for its own
  integration tests — in the same class as `nros-board-freertos`, which compiles
  the FreeRTOS kernel and lwIP out of an env-resolved SDK root. They have nothing
  to converge and every reason to be told apart.
* `packages/rmw/cyclonedds/cyclonedds-sys` builds roughly 200k lines of vendored
  C through `cmake::Config`, and read as `P` — "re-roots inherited paths and
  nothing else".

### What replaced them

**One ROLE per script**, ordered rules, first match wins, and an `UNCLASSIFIED`
list printed by path rather than an "other" bucket — because an unnamed row is a
finding for whoever added it, not a number to carry. It is 0 today.

| role | n | what it is |
| --- | --- | --- |
| `delegating-shim` | 17 | every effect is a call into a shared build crate |
| `c-compiler` | 12 | drives a C/C++ compile (see SOURCE ORIGIN below) |
| `linker-script` | 11 | places a linker script in `OUT_DIR`, compiles nothing |
| `codegen-driver` | 9 | runs an in-repo generator at build time |
| `generated` | 6 | emitted by codegen; never hand-edited |
| `knob-resolver` | 5 | resolves RFC-0049 knobs into a generated config module |
| `abi-bindings` | 2 | generates Rust from a C header (RFC-0054) |
| `provenance-stamp` | 2 | embeds a source stamp so a stale binary can say so |
| `marker` | 2 | only rerun directives — an input cargo cannot see by itself |

Plus **two derived fields that are the questions the hazards turn on**, kept
beside the role instead of folded into it:

* **`source_origin`**, for each `c-compiler` — where its sources come from. Three
  answers exist and **only one is exposed to issue 1280**:

  | origin | n | exposed to 1280 |
  | --- | --- | --- |
  | `sdk-path-variable` | 7 | yes — the value is inherited from the environment |
  | `workspace-relative` | 3 | no — fixed by the checkout |
  | `links-channel` (`DEP_*`) | 2 | no — resolved by the crate that owns the tree |

* **`board_role`** — base vs overlay, read from the MANIFESTS (an overlay
  path-depends on the board crate that compiles the RTOS for it). 8 base,
  5 overlay.

* a **PATH RESOLUTION** section, which is issue 1280's build-script half as a
  number. The variable names are read off `just/sdk-env.just`, never authored,
  so this census and `check-inherited-checkout-paths` are subjects of the same
  fact.

---

## W1 — the one measured defect (LANDED → issue 1527)

The PATH RESOLUTION line read **5** when it was first written and reads **0**
now. Detail, measurement and the two sites deliberately left open are in
[issue 1527](../issues/archived/1527-build-script-path-resolution-skips-the-reroot-rule.md).
The short version: three board scripts read `FREERTOS_CONFIG_DIR` with a raw
`env::var` two lines under siblings that used the shared resolver, and two more
hid the same drift behind a **private helper named like the shared one**, taking
the variable name as an argument so no literal-matching probe could see it.

---

## The verdicts — outlier by outlier

The brief named six suspects. **Three converge, two are legitimately different
jobs the taxonomy was mis-describing, and one was a bug in the census itself.**

### 1. `CEBP` is not the template — it is the residue. **CONVERGE.**

The five FreeRTOS/ThreadX board crates sharing `CEBP` looked like a converged
shape. They are the opposite: they are what is LEFT once every family that DID
converge moved out.

`nros-board-common` already offers two levels of abstraction, and which one a
family gets is an accident of who last touched it:

* **NuttX and ThreadX-RISCV have RUNNERS.** `nuttx_ffi_build::run_nuttx()`,
  `threadx_qemu_riscv64_build::run(...)`, `nuttx_platform_build::run_platform()`.
  Their build scripts are **3, 7, 17 and 62 lines**, and every one of them is a
  `delegating-shim`.
* **FreeRTOS has only HELPERS.** `configure_cflags`, `add_freertos_includes`,
  `add_lwip_includes`, `emit_app_config_tu`. So each overlay carries the whole
  recipe itself: **149, 149 and 243 lines**.

The cost, measured. `nros-board-mps3-an536-freertos/build.rs` and
`nros-board-s32z270-freertos/build.rs` past their doc comments are **131 shared
lines with 5 differing ones** — the crate name, the linker-script name (twice),
the board C file name (twice). `gcc_print_file` is copied verbatim into **three**
board scripts, hardcoded `-mcpu` flags and all.

And the board/overlay table makes the point in one row: of the **5 board
overlays**, `nros-board-threadx-qemu-riscv64` is a 17-line `delegating-shim`
while the other four carry 149–260 lines each. The target shape exists, has an
exemplar, and four crates have not moved to it. → **W2**

### 2. `nros-board-mps2-an385-freertos` is the only board emitting `rustc-cfg`. **LEGITIMATELY DIFFERENT.**

The `G` is one line: `cargo:rustc-cfg=nros_trace`, inside
`if nros_trace { … }`, guarding the Tonbandgeraet trace library. It is the only
board with tband wiring, so it is the only board that can emit that cfg. `G` was
never a role — it was one board's optional feature promoted to a taxonomy axis
by a grep. No action; it disappears with W2 like the rest of the body.

### 3. `cli/nros-cli-core/src/cmd/build.rs` — **a bug in the census, now fixed.**

Not a crate, not a build script: it is `pub mod build` under `src/cmd/`, the
`nros build` command. Its three capability letters were prose matches. → W0.

### 4. `cyclonedds-sys` is `P` alone. **THE TAXONOMY WAS WRONG.**

It compiles Cyclone DDS — through `cmake::Config`, not `cc::Build`. `C` keyed on
the tool. Under the new census it is a `c-compiler` with
`source_origin = sdk-path-variable` (`CYCLONEDDS_SOURCE_DIR` via
`env_or_repo_path`), which is the same role as the board crates and the same
hazard surface. Its sibling `nros-rmw-cyclonedds-sys` is the `links-channel`
case: it reads `DEP_DDSC_INCLUDE` / `DEP_DDSC_IDLC`, so the vendored tree is
resolved once, by the crate that owns it, and handed over. **That is the shape to
prefer** — the `links` key also orders the two build scripts, which a shared env
variable cannot do.

### 5. `nros-platform-cffi` is `C` alone. **LEGITIMATELY FINE — measured, not assumed.**

The brief asked whether this is a latent 0491 or 1280. It is neither, and the
reason is checkable rather than a shrug:

* **0491 needs a `rerun-if-env-changed` on a path.** This script has **no
  `rerun-if-env-changed` at all**, and a repo-wide sweep finds **zero** such
  directives on a path-shaped name in any build script — `check-path-env-fingerprints`
  holds.
* **1280 needs an inherited absolute path.** Every path it names is relative and
  in-tree: `tests/c_stubs/platform_stubs.c`,
  `../nros-platform-api/include/nros/platform.h`,
  `../nros-platform-posix/src/platform.c`. `checkout_root_of` answers `None` for
  a relative path *by design* — it is resolved against the caller's own cwd, so
  it cannot have been inherited.

Its `source_origin` is `workspace-relative`, and that is the class that cannot
have the bug. No action.

### 6. The linker-script boards have no capabilities. **CORRECT BY DESIGN — and still the largest duplication in the tree.**

Confirmed: emitting a `memory.x` into `OUT_DIR` needs no knob, no SDK path and
no compiler, so having no capability letters was the right reading. The old
taxonomy simply had nothing to say about them.

What it could not see is that there are **11 of them, and only 6 distinct
bodies** after comments and whitespace are stripped — one body appears 4 times,
two appear twice. Every one does: embed a `.x`, write it to `OUT_DIR`, print
`rustc-link-search`, print two `rerun-if-changed` lines. The differences between
the 6 are which file is embedded and what the local variable is called.

This is not urgent (the job is self-contained and has never produced a defect)
but it is the cheapest converge in the tree. → **W4**

---

## What the study found that the brief did not ask about

**`delegating-shim` is already the largest role, at 17 of 66 — and 12 of those
are one byte-identical body.** Every Zephyr leaf's `build.rs` is:

```rust
fn main() {
    zephyr_build::export_kconfig_bool_options();
    nros_zephyr_build::bake_nros_config();
}
```

12 copies, 1 normalized body. Unlike the linker-script family this one is
**correct as it stands**: cargo requires the file to exist beside the manifest,
these leaves are standalone copy-out projects (RFC-0026), and a copied-out leaf
must not depend on a crate that only exists in this checkout. Recorded so the
next reader does not "fix" it. The same reasoning does NOT protect the FreeRTOS
overlays: those are in-repo board crates that already depend on
`nros-board-common`.

**"Where does a vendored tree come from" has three answers in this census and no
rule picking between them.** FreeRTOS/lwIP/ThreadX/NetX/NuttX/tband come from
path-valued env variables; micro-XRCE-DDS-Client comes from a workspace-relative
path through `xrce-sources.txt`; Cyclone comes from an env variable in one crate
and a `links` hand-off in the next. Only the first is exposed to 1280, and
nothing says which a new backend should use. → **W5**, which found a FOURTH the
census cannot see — nine SDK roots arrive as `{env:VAR}` tokens in three
`nros-platform.toml` descriptors, from no `build.rs` at all — and wrote the rule
down as [RFC-0101](../design/0101-vendored-source-resolution.md).

---

## Work items

### W2 — give FreeRTOS a RUNNER, like NuttX and ThreadX-RISCV have (LANDED, steps 1–2; step 3 REFUSED with a reason)

`nros_board_common::freertos_build::run_overlay(&Overlay { … })` exists. The
signature is a struct, not a positional list, because seven things differ and
five of them are `&str`:

| field | why it is a field |
| --- | --- |
| `crate_name` | `skip_cross_build`'s message (issue 0288) |
| `board_linker_script` | one of the 5 differing lines W0 measured |
| `default_port` | `GCC/ARM_CM3` vs `GCC/ARM_CRx_No_GIC` |
| `board_c_files` | another of the 5 |
| `extra_link_libs` | MPS2 names `lan9118_lwip` a second time on ld's pass |
| `extra_archives` | MPS2's LAN9118 driver and its opt-in tband library |
| `configure_glue` | includes/defines those two add to the board glue TU set |

Two hooks rather than one because MPS2's extras land on both sides of the glue
compile, and `Overlay::new(..)` + `..` covers the two boards that need neither.

**The `-mcpu` list is NOT a parameter, and the question the brief left open has
a measured answer: the `[arch.*]` profile already IS the source.**
`[arch.cortex-m3] cflags = ["-mcpu=cortex-m3", "-mthumb"]` and
`[arch.cortex-r52] cflags = ["-mcpu=cortex-r52", "-mfpu=neon-fp-armv8",
"-mfloat-abi=hard"]` are, verbatim, what the MPS2 copy and the other two copies
of `gcc_print_file` passed. So the three hardcoded lists were hand-mirrors of a
file the compile beside them already read, and `gcc_print_file` now calls the
same `resolve_cflags()` `configure_cflags` does. That also closes a latent
mismatch the copies had: `FREERTOS_CFLAGS` is the RFC-0049 rung-1 override and
wins for the COMPILE, and a hardcoded list cannot see it — so a board pointed at
another CPU through that variable compiled its C for the new one and linked the
old one's newlib.

**What each overlay lost**

| crate | before | after |
| --- | --- | --- |
| `nros-board-s32z270-freertos` | 151 | **29** |
| `nros-board-mps3-an536-freertos` | 151 | **41** |
| `nros-board-mps2-an385-freertos` | 249 | **111** |

370 lines out of the overlays; `freertos_build.rs` took 171 lines of code and
158 of comment/blank. So this is not a line saving and is not claimed as one —
it is three copies of one recipe becoming one copy, which is the thing that had
already drifted (issue 1527, and now 1561).

`nros-board-mps2-an385-freertos` keeps its `cargo:rustc-cfg=nros_trace` in the
leaf, beside the compile that earns it, exactly as verdict 2 above rules. It is
the only board with tband wiring; absorbing it would put one board's optional
feature in every board's recipe.

#### Acceptance — what was BUILT

`just ci matrix` was not used: issue 1158's 31 runs with 0 verdicts is the wrong
instrument for a build-script refactor even when it works, because a green cell
does not say whether the artifact CHANGED. The three board crates are
workspace-`exclude`d and cross-only, so each was built directly with `cargo
build --target <triple>`, at HEAD and after, **into the same `--target-dir`** so
`OUT_DIR` is the same string and the comparison is about the code:

| crate | target | build-script `output` | archives |
| --- | --- | --- | --- |
| `nros-board-s32z270-freertos` | `armv8r-none-eabihf` | byte-identical | `libstartup.a` byte-identical |
| `nros-board-mps2-an385-freertos` | `thumbv7m-none-eabi` | 3 set-valued lines differ (below) | `libstartup.a`, `liblan9118_lwip.a` byte-identical |
| `nros-board-mps2-an385-freertos`, `NROS_TRACE=1` | `thumbv7m-none-eabi` | same 3 lines | `libstartup.a`, `liblan9118_lwip.a`, `libtband.a` byte-identical |
| `nros-board-mps3-an536-freertos` | `armv8r-none-eabihf` | **no "before"** — see issue 1561 | — |

The three MPS2 lines are all `cargo:rerun-if-changed`, which cargo reads as a
SET: the LAN9118 watch moved earlier, and `trace/trace_dump.c` was ADDED. That
file was compiled with no rerun trigger before, so editing it did not rebuild —
a missing edge the migration closed rather than a difference it introduced.
Nothing was removed.

The include-order question the hooks raise was measured rather than argued: the
MPS2 glue used to take `nros-c` between the LAN9118 include and the tband ones,
and now takes it after both. The header name sets are disjoint, so resolution is
unchanged — and `libstartup.a` is byte-identical on both the trace and non-trace
paths, which is the evidence.

**Issue 1561 is what priming the negative control bought.**
`nros-board-mps3-an536-freertos` did not build at HEAD, from cargo, at all:
`board_an536.c` includes `lan9118_lwip.h` for its strong netif overrides, the
overlay was copied from the S32Z270 one whose netif is consumer-side, and only
`cmake/board/nano-ros-board-mps3-an536-freertos.cmake` carried the include. The
crate is workspace-`exclude`d and no fixture row cargo-builds it (the AN536 row
is a C++ entry), so no lane on any event had ever asked. It builds now; there is
no before/after image for it, because there was no "before".

#### What this did to W3's reach — stated, not discovered later

The migration MOVED three path reads — `FREERTOS_DIR`, `LWIP_DIR`,
`FREERTOS_CONFIG_DIR` — out of the three board `build.rs` files and into
`freertos_build.rs`, which is a build-script LIBRARY.
`check-build-script-path-resolution` reads `build.rs` files, so those reads left
its reach. **Measured, once W3's gate landed on `main` at `d2eacf1c7`**: it
reports **6** build scripts naming a path-valued SDK variable against the
pre-change overlays and **3** after, and W2 is the whole difference.

Nothing was laundered. Every one of the three still goes through
`nros_build_paths`; the only remaining `env::var` in the runner is
`FREERTOS_PORT`, a port NAME, plus cargo's own `OUT_DIR` / `CARGO_MANIFEST_DIR`
/ `TARGET`; and the gate is GREEN on this branch. The surface it must cover went
from three copies to one — and that one is in the crate **W6** is already
deciding about, for its three raw `NUTTX_DIR` reads. This is the second site
waiting on that answer, and it is recorded in `OverlayEnv`'s doc comment as well
as here, so extending the gate to `nros-board-common` reaches both at once.

#### Step 3 — `nros-board-threadx-linux` — REFUSED, and this is the reason

It is not "the same shape one family over". Of the six things `run_overlay`
does, it wants **none**: no linker scripts into `OUT_DIR` (it is a host build),
no FreeRTOS/lwIP include set (ThreadX + NetX Duo), no `[arch.*]` cflags for a
FreeRTOS platform, no `arm-none-eabi-gcc --print-file-name` multilib discovery,
and its `NROS_APP_CONFIG` is a hand-written C string with values that
DELIBERATELY differ from `BaseConfig::default()` (locator `tcp/127.0.0.1:7555`,
scheduling all zero) rather than `emit_app_config_tu`'s. Making the runner fit
it means making every step optional, which is "widen a rule until it matches" —
the thing this phase's own last section forbids.

What it shares with the FreeRTOS overlays is real and is a DIFFERENT work item:
a hand-written `emit_app_config_def` mirroring a Rust default by eye is the
exact defect phase-337 W5.d fixed for FreeRTOS, and it had drifted by 128 KiB
there. It is not fixed here because the drift is deliberate and changing it
changes the image.

#### `check-board-build-wiring` — the stronger question

It asked whether a C-compiling board *reaches* `nros-board-common`. All three
overlays answered yes while carrying the whole recipe, so the gate could not
tell reaching from delegating.

The stronger question that is still a PROPERTY and not a proxy for length:
**does this board choose a compiler target?** An ISA/ABI flag (`-mcpu=`,
`-march=`, `-mabi=`, `-mfpu=`, `-mfloat-abi=`, `-mcmodel=`, `-mthumb`) written
in a board `build.rs` is a SECOND answer to "what is this compiling for", and it
cannot see the first — neither the `[arch.*]` profile nor the rung-1 env
override. It is falsifiable at a line; a board with a 300-line build script and
no arch flag passes, and a one-line one that names `-mcpu=` does not.

Comments are stripped before the scan, with a small string-aware scanner:
naming a flag while EXPLAINING one is what five board scripts legitimately do
(issue 0478's note is in four places in `nros-board-freertos` alone), and
reporting those is 0196's shape the other way round — a reach wider than the
rule. Negative control: run against HEAD's `nros-board-s32z270-freertos` the
gate reports `-mcpu, -mfloat-abi, -mfpu`, i.e. it catches exactly the defect W2
removed.

It found **one** other site, which is filed rather than fixed:
`nros-board-threadx`'s RISC-V64 flags and picolibc probe duplicate
`threadx_qemu_riscv64_build`'s, byte for byte → **issue 1562**, exempted at the
site with that id, because acceptance there is a RISC-V64 ThreadX BUILD and not
a gate.

One census note for whoever owns `nros-build-wiring.py`: `s32z270` is now a
`delegating-shim`, but `mps3-an536` still reads `c-compiler` — its only mention
of `cc::Build` is the type annotation on its `configure_glue` closure, and it
constructs none. That is W0's own "a capability letter is a grep for a TOOL"
trap surviving in the role rule; keying on `cc::Build::new(` / `.compile(`
rather than the bare type would answer it.

### W3 — a gate for the build-script half of issue 1280 (LANDED)

**`check-build-script-path-resolution`**, on the fast line, reached by a
merge-gating event. It reports 6 build scripts naming a path-valued SDK
variable, all routed, over the 21 variables read from `just/sdk-env.just`.
**W6 widened both of those subjects** — a second population (build-script
LIBRARIES) and a second producer (board descriptors' `[env]`), each for a
reason W6 records; read the number from `python3 scripts/nros-build-wiring.py
--scripts`, not from this paragraph.

**It ENFORCES the census's number rather than computing a rival one.** The gate
imports `scripts/nros-build-wiring.py` and fails on its `unprotected` rows, so
"what counts as a path variable" has one answer that moves both halves of 1280
at once. Re-deriving would have been the defect one level up: issue 1280's own
census was an authored 19-name copy and was already short by five when written.

**It also widens the private-helper rule the census states.** The census finds
those bodies by NAME (`^fn env_path\w*`), which is narrower than the rule this
phase wrote down — *the rule is about the helper's BODY* — so a helper called
`sdk_dir()` was invisible to it. The gate asks the body: any local fn with a
`&str` parameter that does `env::var` of that parameter and never reaches
`nros_build_paths`. Both measured hazards are self-tested, and the self-test
runs on the NORMAL path, not only under `--self-test`.

Negative controls, run live against a real board crate and reverted:

* reverting one site to a raw `env::var` — `rc=1`, naming the variable;
* replacing it with a **differently-named** private helper — `rc=1`, naming the
  helper. This is the arm the census's name rule would have passed over, which
  is why it is the control worth having rather than a second copy of the first.

Exemptions are `// nros-build-paths-exempt: <reason>`, with a reason, as
phase-468's are. Nothing in the tree needs one today — which is the W6 question,
below, not an absence of cases.

<details>
<summary>The original work item, for what it asked</summary>


`check-inherited-checkout-paths` contains zero references to `build.rs`,
`nros_build_paths` or `env::var`: its reach is narrower than the rule it
enforces, which is the 0196 shape the 2026-07-28 audit found in four gates. That
is how issue 1527's five sites drifted with every gate green.

The rule to hold: **a build script that resolves a path-valued SDK variable does
it through `nros_build_paths`.** The variable set is READ from
`just/sdk-env.just` (the same subject the shell half already has), never
authored. Two things the gate must handle, both measured while writing this
phase:

* a **private helper** taking the name as an argument defeats literal matching —
  the rule is about the helper's BODY, not its call sites;
* `env::var("X").is_err()` is a presence test, not a path read, and counting it
  reports a crate with no defect.

Exemptions at the site (`// nros-build-paths-exempt: <reason>`), like
phase-468's, with a reason read by whoever changes the thing it excuses.

</details>

### W4 — one `linker-script` helper (LANDED)

**Home decided in phase-471 W5's PR; the migration landed 2026-09-29.**

11 scripts, 6 bodies, one job. The phase doc's own condition was that **the home
must be decided first**, and the home turned out to be the whole question. Three
measurements, 2026-09-29:

* **The 6 bodies are real**, confirmed by hashing each file with comments and
  blank lines stripped: one body ×4 (`wake-latency-cortex-m3`,
  `stm32f4-smoltcp-echo`, `cdr-roundtrip-qemu`, `lan9118-qemu`), two ×2
  (`stm32f4-porting/{polling,rtic}`; `heap-free-poc-mps2` +
  `logging-smoke-mps2-baremetal`), three singletons. 14–42 lines each.
* **Only 2 of the 11 are board crates.** The split is 2 boards / 2
  `packages/reference/stm32f4-porting` / 3 `nros-bench`+`nros-smoke` / 4
  `nros-tests/bins`. So `nros-board-common` is the wrong home outright — a
  testing bin depending on a board helper crate is backwards, and 9 of 11 are
  testing or reference crates.
* **None of the 11 has a `[build-dependencies]` section at all.** Centralising
  gives all of them their first build-dependency, i.e. a new compile unit in
  every one of those cross graphs.

**The home is `nros-build-paths`** (`packages/tooling/`), and the precedent is
recorded rather than invented: `nros-build-helpers/Cargo.toml` already says
*"issue 0657 — the riscv64 toolchain resolver lives in the ZERO-DEP crate, not
here: this one pulls cbindgen, and putting a directory lookup behind that
dragged cbindgen into the `nros` CLI graph (118 lock lines)."* Same class, same
answer: `nros-build-paths` has **zero dependencies**, is `host-only = true`, and
is already the crate a build script may reach for build-time path work. A
`link_script` module there costs each caller one edge to a dependency-free
crate.

**Two of the 11 are excluded, by the same rule that protects the Zephyr leaf
shims.** `packages/reference/stm32f4-porting/{polling,rtic}` are, per their own
README, *"templates for BSP developers creating new board support crates"* — a
developer copies them out. Giving a copy-out template a dependency on a crate
that exists only in this checkout is RFC-0026's hazard, and it is the reason the
12 Zephyr shims are deliberately left alone. They keep their 20 lines.

#### The shape — one literal, because two can disagree

`include_bytes!` is a macro, so a helper in another crate cannot read the
caller's file: the bytes have to be handed to it. A plain
`emit(source, dest, bytes)` therefore lets a caller name one file in the
`rerun-if-changed` watch and embed another — the watch would sit on a file the
image does not contain, so editing the real linker script would change nothing
and cargo would report the tree fresh. That is this campaign's recurring failure
(a value delivered to some readers and not others) reintroduced by the very
consolidation meant to end it.

The entry point is a macro taking ONE literal, from which both uses are derived:

```rust
nros_build_paths::link_script!("memory.x");                    // 8 of the 9
nros_build_paths::link_script!("mps2-an385.x" => "memory.x");  // the 9th
```

The literal resolves relative to the file that INVOKES the macro, not to the
crate defining it — `include_bytes!` keys on the span of its string argument,
and the argument comes from the caller. Measured before it was relied on, with a
throwaway two-crate workspace, rather than assumed from how it reads.

`emit` stays public for the rename form's sake and says in its own docs why the
macro is the way in. `write_into` splits the filesystem half off so it is
testable without `OUT_DIR`, a process-global this crate's test harness shares.

#### Acceptance — what was BUILT

All 9 migrated crates compile and LINK, and each emitted script was compared
BYTE FOR BYTE against its source, including the rename:

| crate | target | emitted |
| --- | --- | --- |
| `mps2-an385-pac` | host lib + linked into the thumbv7m images below | `device.x` |
| `nros-board-mps2-an385` | `thumbv7m-none-eabi` | `memory.x` ← `mps2-an385.x` |
| `wake-latency-cortex-m3` | `thumbv7m-none-eabi` (2 bins) | `memory.x` |
| `qemu-rs-wcet-bench` | `thumbv7m-none-eabi` | `memory.x` |
| `stm32f4-rs-smoltcp` | `thumbv7em-none-eabihf` | `memory.x` |
| `qemu-rs-test` | `thumbv7m-none-eabi` | `memory.x` |
| `heap-free-poc-mps2` | `thumbv7m-none-eabi` | `memory.x` |
| `qemu-rs-lan9118` | `thumbv7m-none-eabi` | `memory.x` |
| `logging-smoke-mps2-baremetal` | `thumbv7m-none-eabi` | `memory.x` |

The emitted `cargo:` directives were diffed against what the hand-written
scripts produced and are identical — same `rustc-link-search`, same two
`rerun-if-changed` lines, the watch still naming the SOURCE file rather than the
destination. No riscv64 crate is in this family; the phase's original acceptance
named one before the population was counted.

`cargo:rerun-if-changed=build.rs` is emitted by the helper rather than reasoned
away. All 9 scripts printed it, and whether recompiling the build-script binary
is enough on its own is a property of cargo nobody here has measured — so it is
kept, and the comment says that is why.

**What did NOT change.** The W3 gate's census is untouched (9 files naming a
path-valued SDK variable, before and after): `OUT_DIR` is cargo's own, not an
inherited SDK path, so none of these 9 was ever a subject of issue 1280. The
count this moves is `nros_build_paths` reachers, 22 → 32 (the 9 call sites
plus `nros-build-paths`'s own now-self-referencing doc text in the census's
file scan).

### W5 — state where a vendored tree comes from (LANDED → RFC-0101)

**Landed as [RFC-0101](../design/0101-vendored-source-resolution.md)**, a new
RFC rather than an amendment. The rule:

> at most one crate per resolved dependency GRAPH resolves a given vendored
> tree; every other crate in that graph reaches it through `links`; the one that
> does resolve it resolves through `nros_build_paths`.

Four things the work changed about the brief above.

**The three mechanisms are not alternatives at one site.** `links` is how a
SECOND crate reaches a tree a FIRST one already resolved — the owner still has
to resolve it somehow. So the flat preference order became two questions:
*who resolves it* (D1/D2) and *how the owner resolves it* (D3), and the
"workspace-relative vs path variable" pair collapsed into one axis whose
discriminator is a property of the TREE — *can this repository ship a copy?* —
answered by `env_or_repo_path` and `env_path` respectively.

**`links` does give build order, and it was measured rather than repeated.** A
four-crate throwaway workspace, wall-clock stamps: a dependent's build script
ran 5 ms after the `links` dependency finished and 7 991 ms before a plain
dependency did. Two limits came with it, both new to the brief — the channel is
exactly ONE HOP (a grandparent gets `Err(NotPresent)` and no ordering), and a
second crate claiming the same `links` value is a **resolve-time error**. That
last is the real argument for the preference: it is the only one of the
mechanisms cargo itself enforces, against a repository whose recurring defect is
two correct derivations of one fact that disagree (0500, 0616, 1025, 1068).

**There is a FOURTH mechanism and the census cannot see it.** Nine SDK roots —
FreeRTOS, lwIP, ThreadX, NetX, NuttX and their config dirs — reach the
zenoh-pico compile as `{env:VAR}` tokens in three `nros-platform.toml`
descriptors, interpolated by `nros-platform-config`, not from any `build.rs`.
RFC-0101 D5 rules on it: the declarative form is right and stays (it is
RFC-0049's platform rung and it can carry a capability gate), but its
interpolator must adopt D3's resolver.

**The tree does not fully conform, and the gaps are filed.** Seven sites derive
the repo root by counting `.parent()` hops — a class already fixed three times
by re-counting them (issues 0365, phase-321 W2.d, phase-400 W1), including one
that *found a real directory that no longer held what it wanted* and reported
nothing → **issue 1558**. Four path-valued inputs bypass `nros_build_paths`, no
two of them outside W3's gate for the same reason (a shared build crate that is
not a `build.rs`; a descriptor token; two variables with no `sdk-env.just` row)
→ **issue 1560**, which includes the one live wrong answer: issue 1527's
unconverted third `env_path_or`, one file from its converted twin.

### W6 — the two sites issue 1527 deliberately left open (LANDED)

**Both RE-ROOT. Neither is exempt, and in both cases the measurement said so
rather than the argument.** The brief allowed an exemption at either site, and
phase-468's "exempt by shape, not by name" would have carried "built in place"
if the shape had held up. It did not: in both files the variable's SIBLING
already re-rooted, so the raw read was not preserving one answer — it was
manufacturing two.

#### Site 1 — colon-separated lists: `env_path_list`

`nros_build_paths::env_path_list(name) -> Vec<PathBuf>`, and
`nros-board-threadx` reads both `THREADX_EXTRA_INCLUDES` and
`NETX_EXTRA_INCLUDES` through it.

**A list is not exempt because an element is a path, and here every element is a
path INSIDE a checkout.** The producer is
`cmake/board/nano-ros-board-rv-virt-threadx.cmake`, which writes
`set(ENV{THREADX_EXTRA_INCLUDES} "${THREADX_DIR}/ports/risc-v64/gnu/example_build/qemu_virt")`
from a bare `$ENV{THREADX_DIR}` — cmake applies no re-root rule and has no
equivalent of `checkout-paths.sh`. So in a linked worktree the kernel SOURCES
came from here (`build.rs:43` has gone through `env_path` since issue 1527)
while `csr.h`, `plic.h`, `uart.h` and `hwtimer.h` came from whichever checkout
the parent shell had activated. Two trees in one `cc::Build`: the 0135/0460
class, not a tidiness question.

The three questions the brief asked, answered in the function's doc comment so
the next caller does not have to re-derive them:

* **An empty element is dropped**, and that is the one place a list differs
  from `env_path` in kind. `FOO=""`, `"a:"` and `"a::b"` all have to mean "no
  directory there"; `PathBuf::from("")` as a `-I` argument names the build
  script's own CWD, which for a board crate is its manifest dir. That is why
  the list form is a function rather than a `split` at each caller — one
  spelling of the separator AND the filter (`split_list`).
* **A relative element is kept, not re-rooted.** `checkout_root_of` answers
  `None` for one BY DESIGN: it resolves against the caller's own cwd, so it
  cannot have been inherited and there is no owner to re-root off. Still
  canonicalised, which is exactly what the two sites already did.
* **`:` is right on every host this builds for.** The three producers (`just`,
  a cmake `set(ENV{…})`, a cargo `[env]` row) all write `:`, and
  `nros-sdk-index.toml` has linux and macos host keys and no windows one —
  where the separator would be `;` and a drive letter would make `:` ambiguous.
  Stated rather than assumed, so a windows host reads as a thing to fix.

#### Site 2 — NuttX: re-root, because "built in place" argues for ONE tree

The phase feared re-rooting would name an UNBUILT kernel instead of a stale one.
Measured in an agent worktree with the inherited
`NUTTX_DIR=/home/aeon/repos/nano-ros/third-party/nuttx/nuttx`:

| resolution | tree | `staging/libc.a` | `nros-nuttx-export-arm` |
| --- | --- | --- | --- |
| raw `env::var` (`nuttx_ffi_build.rs:197,385`, `nuttx_image_link.rs:81`) | the MAIN checkout | present | present |
| `nros_build_paths::nuttx_dir()` (`nuttx_platform_build.rs:27,125`) | this worktree | absent | absent |

Those two resolutions are reached **from one `build.rs` twenty lines apart** —
`nros-board-nuttx-qemu/build.rs` calls `run_platform()` then
`run_image_link()`. So the state the raw read preserved was never "the tree that
is built": it was the platform C port compiled from one checkout and the kernel
libs, linker script and headers taken from another. Issue 0511 already measured
what that costs when the two configs differ (an ARM image linked with the RISC-V
memory map, `CONFIG_FLASH_SIZE` 0, read as a 400–500 KB size regression).

Three more measurements that decided it:

* **All three sites were ALREADY guarded on existence** — `cxx.is_dir()`,
  `staging.join("libc.a").exists()`, and an env-absent early return. So
  re-rooting into an unprovisioned worktree cannot produce a wrong artifact; it
  produces the same early return these functions already take on a host
  `cargo check`, and the image link then fails LOUDLY. That is issue 1280's own
  rule: a gate that RAN and measured the wrong tree is worse than one that
  failed.
* **Nothing needs a new provisioning step.** `just/sdk-env.just` re-roots
  `NUTTX_DIR`, and `scripts/nuttx/build-nuttx.sh` derives its default from its
  own location, so `just nuttx setup` + `just nuttx build` in a worktree already
  build THAT worktree's tree. `just setup-worktree` deliberately does not — it
  initialises the three submodules two fast gates need (issue 1373), not twenty
  — and NuttX is no more special here than FreeRTOS or ThreadX, both of which
  have re-rooted since 1527.
* **The resolver is `env_path`, not `nuttx_dir()`.** Both sites' comments say
  they are strictly env-gated so a host `cargo check` of a dependent Entry pkg
  stays link-directive-free; `nuttx_dir()` would fall back to the in-repo
  default and break that. `env_path` answers `None` when unset and re-roots when
  set, which is the whole change.

#### The gate's reach — extended, in two directions, both 0196's shape

`check-build-script-path-resolution` (W3) passed over both sites, for two
independent reasons. Both are now closed, and both by DERIVING a second subject
rather than widening a declaration.

* **The POPULATION was `build.rs`.** `nros-board-common` is a build-script
  LIBRARY, so a raw read there reaches every board that calls it — strictly
  worse than one in a single script. `build_script_libs()` walks
  `[build-dependencies]` path deps out of the census's build scripts and then
  transitively through those crates' own in-repo path deps (`[dependencies]`
  count there, because the library is already running at build time;
  `[dev-dependencies]` never do, and including them reached most of the tree).
  The crate that DEFINES `pub fn reroot_foreign` is excluded — its own
  `env_path` cannot be spelled `nros_build_paths::`, so it would report the rule
  as a violation of itself — identified by what it defines, not by its name.
  **W0's narrowing of `build_scripts()` is untouched**: this is a second
  population answering a second question, not a widened first one.

  It found **seven more live sites** in two crates nobody had looked at, all
  fixed here: `nros-zpico-build/src/runner.rs` resolved
  `FREERTOS_DIR`/`FREERTOS_CONFIG_DIR`/`LWIP_DIR`/`NUTTX_DIR`/`THREADX_DIR`/
  `THREADX_CONFIG_DIR`/`NETX_DIR`/`NETX_CONFIG_DIR` raw to pick the headers its
  **ABI probe** compiles against — a probe whose whole job is to measure a
  struct layout the real build must agree with, measuring one checkout's headers
  while the board crate compiled another's. And
  `nros-platform-config/src/manifest.rs`'s `{nuttx_include}` token read
  `NUTTX_DIR` raw before handing it to `nuttx_include_root`, i.e. the one
  spelling issue 0551 created so an author could not get this wrong.

* **The SUBJECT was `just/sdk-env.just` alone.** `THREADX_EXTRA_INCLUDES` is
  exported by a board descriptor and a cmake board file and never by `just`, so
  no amount of reading `sdk-env.just` would ever find it.
  `board_env_path_vars()` derives it from the `cargo_config` `[env]` rows in
  `nros-board.toml`, keyed on `${workspace}`-rooted values — precisely the class
  1280 is about (a path INSIDE a checkout; one outside every checkout is KEPT by
  the rule anyway, so not deriving those costs nothing). Kept as a SEPARATE
  function: `sdk_path_vars()` stays paired with `check-inherited-checkout-paths`
  off one line, and widening it would have broken that pairing rather than
  extended it. 21 + 1 = 22.

**And the widening produced a false report, which is worth recording because it
is issue 1452's shape in miniature.** With libraries in the population, the
private-helper rule flagged `nros-zpico-build`'s `declared_fact(name: &str) ->
Option<String>` and `declared_floored(name: &str) -> Option<usize>` — env
readers for RFC-0049 COUNT knobs, nothing to do with paths, in a file that after
the fixes above has no defect. The remedy was NOT an exemption: the rule is
about PATH resolution, so a helper qualifies only when its return type or body
is path-typed. Both directions are self-test cases.

**Negative controls, live on the real tree and reverted:** the ThreadX list site
put back to its old `split(':')` → rc=1 naming `THREADX_EXTRA_INCLUDES`; the
NuttX image-link site put back to `env::var` → rc=1 naming `NUTTX_DIR`; the same
site with `// nros-build-paths-exempt: …` → OK with the reason printed. Census
line: **0 resolving one WITHOUT it**, now over 6 build scripts and 5
build-script library files.

**What the gate still does not see, stated rather than left to be found:** a
path variable produced by a cmake board file ALONE, with no descriptor row.
Adding cmake would be a third derivation over a language whose variables are not
distinguishable as paths by shape — and the place the rule is enforced for those
is the READER, which is in one of the two populations either way.

---

## What this phase deliberately does not do

**It does not collapse the 12 Zephyr leaf shims** (see above — standalone
copy-out leaves may not depend on an in-repo build crate).

**It does not merge `c-compiler` into one crate.** The three `source_origin`
values are three different contracts, and the 94-declaration lesson from issue
1208 applies: a shared home is only right when the things sharing it answer the
same question.

**It does not add a role for every shape.** `UNCLASSIFIED` is 0 and must stay 0,
but the remedy for a new shape is to RULE it — not to widen a rule until it
matches, which is how the capability letters became a list of greps in the first
place.

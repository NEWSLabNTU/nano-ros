# Phase 346 — The RFC-0064 seam, actually reachable from out of tree

**Status (2026-08-12). COMPLETE — W1, W2 and W3 all LANDED.** Rust now builds
for a real Zephyr board (`mps2_an385`), which issue 0432 said was impossible for
any board with gpio nodes. The measurements below are done
and reproduce on this tree. W1's blocking design question is ANSWERED by a
spike** (see "W1 SPIKE RESULT"): the framework IS resolvable for an out-of-tree
board, but the resolution moves out of macro expansion and into the Entry's
build script, because expansion-time env and file reads are invisible to cargo's
fingerprint. Two of the four routes tried are rejected on measurement.
Two issues, one claim: RFC-0064 says a board
arrives through an integration shell that nano-ros never sees, and today that
path is blocked in two independent places — one silently (issue 0415), one
loudly (issue 0432). Neither touches the build-cache program, so this phase runs
in parallel with [phase-340](archived/phase-340-build-artifact-reuse.md) and
[phase-345](phase-345-one-door-build-parity.md) with no fence.

**Closes:** issue 0415 (W1) and issue 0432 (W2/W3), both landed and verified.
**Implements:** [RFC-0064](../design/0064-board-support-organization.md) (a board
is a config, not a crate) for the out-of-tree case.
**Touches:** RFC-0032 (entry codegen pipeline — the macro's emit shape),
phase-337 (W7.a deleted the last in-tree Embassy key; W2.f added the Cortex-M
witness that found 0432).
**Related:** issue 0248 (retired with the board crate 0415's last Embassy key
pointed at), phase-341 (the same "one SSoT, a mechanical projection, a gate"
shape, applied to the board's `cargo_config`).

---

## 1. Two blocks on one path

An integrator following RFC-0064 brings their own board. Today:

| | what happens | failure mode |
| --- | --- | --- |
| **0415** | an out-of-tree board declaring `framework = "embassy"` / `"rtic"` gets a plain `fn main()` | **silent** — the image links and then does not do what the framework was for |
| **0432** | any Rust image for a Zephyr board whose devicetree has gpio nodes fails inside the `zephyr` crate | **loud** — but it is upstream, so there is no local edit that fixes it |

0415 is the worse of the two: a wrong entry shape that compiles is exactly the
class CLAUDE.md's "tests must fail on unmet preconditions" rule exists for,
applied to codegen instead of tests.

## 2. Measurements (2026-08-10, this tree)

### 2.1 0415 — the table, and the SSoT that already exists

The macro's mapping is deploy-keyed and falls through to `OwnedSpin`
(`packages/core/nros-macros/src/main_macro.rs:3049`):

```rust
fn framework_for(deploy: &str) -> Framework {
    match deploy {
        "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => Framework::Rtic,
        "zephyr" => Framework::Zephyr,
        "esp32-qemu" | "qemu-esp32-baremetal" => Framework::Esp32,
        _ => Framework::OwnedSpin,
    }
}
```

The `Framework` enum's own doc comment (`main_macro.rs:2993`) already states the
gap and names this issue as its fix, and `Framework::Embassy` is documented as
UNREACHABLE from this function since phase-337 W7.a. The second reader —
`read_board_framework` at
`packages/cli/nros-cli-core/src/cmd/check_workspace.rs:423` — parses
`[package.metadata.nros.board] framework` and is the one an out-of-tree board
already satisfies.

**Two corrections to issue 0415's stated fix shape**, both found by reading the
tree rather than the issue:

**(a) The "build-graph fs round-trip at expansion time" blocker is already
paid.** 0415 defers on it, citing `rtic_board_spec_for`'s `dispatchers`. But the
macro *already* reads manifests from disk during expansion, three times, and
already deps `toml`:

| site | reads |
| --- | --- |
| `main_macro.rs:2522` `read_entry_deploy` | the Entry pkg's `Cargo.toml` → `[package.metadata.nros.entry] deploy` |
| `main_macro.rs:2715` `read_deploy_rmw` | the same manifest, per-board rmw |
| `main_macro.rs:2728` `read_deploy_overlay` | the same manifest, deploy overlay |

`nros-macros/Cargo.toml` carries `toml = "0.8"` for precisely this. So the round
trip is not new capability — it is one more call in a function that already makes
three.

**(b) There is already a shared deploy→board SSoT crate, and it is the right
home.** Both the macro and the CLI's Rust emitter delegate to ONE table:

```
packages/core/nros-macros/src/main_macro.rs:2924   board_path_for → nros_orchestration_ir::board_path_for
packages/cli/…/codegen/entry/emit_rust.rs:189      board_path_for → nros_orchestration_ir::board_path_for
```

and `emit_rust.rs` says so in its doc comment: *"the single source of truth
shared with the `nros::main!()` proc-macro. Any board added to the IR crate is
automatically available here with no extra edit."* `nros-orchestration-ir` is
already a workspace dep of `nros-macros`. **The framework mapping belongs in that
crate**, beside the board-path mapping it is the sibling of — not copied into the
macro, and not reached by making the macro depend on `nros-cli-core` (phase-262 /
issue 0083 deliberately removed that dep; restoring it pulls the whole CLI into
every `nros` build).

**One thing the IR table cannot supply**: `board_path_for` returns a **Rust
module path**, not a directory, so it does not locate a manifest to read. For an
out-of-tree board it could not anyway — such a board is not in the table. The
resolution therefore has to go through the Entry package's **board dependency**
(a path or registry dep, which cargo can locate), with the IR table kept as the
in-tree fast path. That is 0415's step 1, and it is the half that still needs
building.

### 2.2 0432 — the delivery mechanism, and the half the issue got wrong

The upstream defect is as issued: `zephyr-lang-rust` @ `404fcef` generates
`GpioPin::new(…, 1u32)` against a 6-argument signature, and the `gpio-keys`
augment in `dt-rust.yaml` carries no `cfg:` key, so `CONFIG_GPIO=n` makes it
worse (14 errors instead of 4) rather than dodging it.

**Correction to 0432's fix shape.** The issue says:

> the existing entries are all `module: zephyr`; this would be the first patch
> against the `zephyr-lang-rust` module, so the in-tree script path needs to
> learn that module too.

The second clause is false. **The in-tree sed/script path already patches
`modules/lang/rust`**, twice:

| script | patches |
| --- | --- |
| `scripts/zephyr/aarch64-rust-patch.sh` | `modules/lang/rust/CMakeLists.txt`, `modules/lang/rust/Kconfig` |
| `scripts/zephyr/cargo-features-patch.sh` | `modules/lang/rust/CMakeLists.txt` |

What is module-`zephyr`-only is **`zephyr/patches.yml`** — all four entries carry
`module: zephyr`, and the file's own header restricts its scope: *"only the
Zephyr-module (drivers/net/nsos_\*) patches live here."* That file is the
ADDITIVE delivery path for downstream BYO Zephyr 4.x workspaces (`west patch`).

So the split is: **in-tree builds are already reachable; downstream `west patch`
consumers are not.** That matters because an RFC-0064 integrator is exactly a
downstream BYO-workspace consumer — the path this phase exists to unblock. The
work is a `patches.yml` entry (plus its `.patch` file and sha256), not a script
that already exists.

### 2.3 The witness slot for W3 already exists, and already names 0432

Zephyr fixtures do **not** come from `examples/fixtures.toml` — parsing it gives
**0** zephyr `[[fixture]]` rows and **1** `[[workspace_fixture]]`
(`workspace-rust-zephyr`, `native_sim/native/64`). They are enumerated by
`scripts/build/zephyr-fixture-leaves.sh`, whose phase-337 W2.f block builds the
Cortex-M witness for `{c, cpp} × zenoh × talker` on `mps2_an385` and says:

> No rust leaf: the pinned `zephyr-lang-rust` cannot compile for any board whose
> devicetree has gpio nodes (issue 0432), which is every real board. That is why
> `matrix::CELLS` has no rust row here either.

W3 is therefore not "invent a fixture" — it is deleting that carve-out: one more
`cm_lang` value plus the matching `matrix::CELLS` row. Anyone proposing a
`[[fixture]]` row for it has the wrong lane.

## 3. Work items

### W1 SPIKE RESULT (2026-08-10) — resolvable, but NOT at expansion

§6's first open question is answered: **yes, an out-of-tree board's framework is
resolvable — but the resolution belongs in the Entry's BUILD SCRIPT, not in the
macro's expansion.** Four routes were built and run in a throwaway 6-crate
workspace; the table is measured, not reasoned.

| route | mechanism | from leaf CWD | from workspace root | verdict |
| --- | --- | --- | --- | --- |
| **M0** | `nros sync` writes `[env] NROS_BOARD_FRAMEWORK` into the leaf's committed `.cargo/nros-board.toml`; expansion reads the var | Ok | **UNAVAILABLE** | **rejected** |
| **M1** | expansion parses the Entry manifest, follows the board dep, reads its `[package.metadata.nros.board] framework` | Ok | Ok | partial — see below |
| **M2** | board declares `links`, its build script emits `cargo::metadata=framework=…`, Entry's build script re-emits it as `rustc-env` | Ok | Ok | **rejected** |
| **M3** | Entry's build script does M1's resolution, declares `rerun-if-changed` on every file it reads, emits `cargo::rustc-env=NROS_BOARD_FRAMEWORK` | Ok | Ok | **adopt** |

**Why M0 is rejected, and it is the most tempting one.** The leaf's
`.cargo/nros-board.toml` is already a committed, `nros sync`-generated projection
of the board descriptor (phase-341), present in 33 leaves, with a generator and a
gate already built — adding one `[env]` row looks free. It is not: cargo
discovers config from the **current directory upward, not per package**, so the
row vanishes when cargo is invoked from a workspace root, and cargo does not
fingerprint config files, so the stale value survives until something else forces
a rebuild. Both were observed: the same binary printed `Ok(embassy)` from the
root (cached) and `UNAVAILABLE` after a forced rebuild. **A route that silently
yields "no framework" is 0415 again through a new door** — the defect being fixed
is precisely a silent fall-through to `OwnedSpin`.

**Why M2 is rejected.** `links` is exclusive per dependency graph, and board
crates depend on OTHER board crates — `nros-board-threadx` pulls 5,
`nros-board-threadx-qemu-riscv64` 4, `nros-board-nuttx-qemu` 3. A blanket `links`
on board crates hard-errors on every real graph. Restricting `links` to
entry-facing boards works only while no entry-facing board depends on another
entry-facing board, which is an unstated invariant needing its own gate — cost
with no benefit over M3.

**Why M1 is only partial.** It works, but the Entry names its board **by bare
version** in 39 of 60 leaves (`nros-board-x = { version = "*" }`), with the real
location in a `[patch.crates-io]` row inline in the leaf's own
`.cargo/config.toml`. So expansion would have to read cargo config — the
unfingerprinted, CWD-discovered file M0 was rejected for. A further **21 of 60**
(the `nros-board-linux` native leaves) have no leaf config at all and resolve
through the repo-root config, so a parent walk is needed too.

**M3 keeps M1's resolution and fixes its invalidation.** A build script may
declare `rerun-if-changed` on every file it reads, which is the edge neither M0
nor expansion-time reading can create: proc-macro file reads and env reads are
invisible to cargo's fingerprint. Note the one nuance the isolation test
exposed — reading a *dependency's `Cargo.toml`* is already safe, because that
file is fingerprinted through the dependency edge; it is the **cargo config**
reads that need the explicit `rerun-if-changed`.

**Two constraints hold for every route, both measured rather than assumed:**

* **The board must be a DIRECT dependency of the Entry.** Built the transitive
  case (entry → lib → board) and both M1 and M2 failed on it — M2 silently. All
  60 in-tree leaves dep their board directly, so this is a constraint to
  *document and gate*, not a blocker.
* **Entries need a build script, and 9 of 92 have one.** That is M3's whole cost.
  `nros sync` already generates committed per-leaf files (`.cargo/nros-board.toml`
  in 33 leaves), so it is the natural generator — and phase-341 already built the
  "generate it, commit it, gate the drift" machinery this would reuse.

**Amendment to W1 below:** step 2 changes from "read the metadata at
macro-expansion time" to "resolve in the Entry's build script and hand the answer
to expansion as `NROS_BOARD_FRAMEWORK`". The macro still owns the emit shape and
the hard error; it no longer owns the resolution. Everything else in W1 stands.

Testbed: `scratchpad/spike` (6 crates, throwaway) — board with `links` +
descriptor metadata, proc-macro probing all four routes, three entry shapes
(direct/transitive/build-script), run from both CWDs with forced rebuilds.

### W1 LANDED 2026-08-10 — built on the spike's M3, with one telling side effect

**Shipped**

| piece | where |
| --- | --- |
| the shared vocabulary + in-tree table | `nros_orchestration_ir::{FRAMEWORKS, is_known_framework, framework_for_board_key}` — beside `board_path_for`, the table both the macro and the CLI emitter already delegate to |
| the out-of-tree route | `nros_build::emit_board_framework()`, called from the Entry package's `build.rs`: resolves the board dep, reads its `[package.metadata.nros.board] framework`, emits `cargo::rustc-env=NROS_BOARD_FRAMEWORK` plus `rerun-if-changed` on every file it read |
| the macro | `NROS_BOARD_FRAMEWORK` wins → in-tree table → `OwnedSpin`; an **unknown name is an error naming the accepted set**, never a fall-through |
| the lint | `nros ws check` warns on a framework outside the shared vocabulary instead of absorbing it as owned-spin |

The build-script resolver handles both dependency shapes measured in the spike:
a `path` dep (28 leaves) and a bare version resolved through a
`[patch.crates-io]` row in the leaf's own cargo config (39 leaves). It reads
only the leaf's own config — walking the cargo hierarchy would mean
reimplementing cargo's merge rules, and an out-of-tree consumer keeps everything
inline anyway (#272).

**The side effect that proves it.** `Framework::Embassy` carried
`#[expect(dead_code)]` whose note read *"the expect fires the day it becomes
constructible"* — no in-tree deploy key had selected it since phase-337 W7.a.
Building this change made that expectation **unfulfilled**, i.e. the compiler
reported that Embassy is now reachable. That is the fix, observed from the
outside rather than asserted: a board declaring `framework = "embassy"` selects
the Embassy emit with no in-tree table entry.

**Tests** (7, all new): every name in the shared vocabulary has an emit branch
and vice versa (the two-way binding that keeps macro and IR in step); every
in-tree board key resolves to an emit shape; an unknown framework errors and the
message names both the bad value and the accepted set; the resolver handles path
deps, patched version deps, a board with no framework key, and — locked in
deliberately — a transitive board dep, which is **not** resolved.

**What W1 did NOT do, stated so it is not mistaken for done:**

* **`nros sync` does not yet generate the build script.** An out-of-tree
  integrator adds three lines (`fn main() { nros_build::emit_board_framework(); }`)
  themselves. Generating it is the ergonomic half and reuses phase-341's
  generate-commit-gate machinery; it is not needed for the seam to work.
* **The DIRECT-dependency constraint is a test, not a repo-wide gate.** The
  spike measured that every route fails on a transitive board; the resolver
  test locks that in as a documented limit rather than a gate over all leaves.
* **No out-of-tree Embassy image was compiled.** That needs an embassy
  dependency set for a Cortex-M target, which this repo does not carry. The
  evidence is the resolution tests plus the retired `#[expect]`, not a linked
  image.

### W1 (original plan) — the framework mapping has one SSoT, and an unknown framework is an ERROR

- [ ] Move the deploy→`Framework` mapping into **`nros-orchestration-ir`**,
      beside `board_path_for`. Both existing readers delegate to it: the macro's
      `framework_for` and `check_workspace.rs`'s `read_board_framework`.
- [ ] Resolve the board crate for an out-of-tree deploy through the **Entry
      package's board dependency** and read `[package.metadata.nros.board]
      framework` from that crate's manifest — **in the Entry's build script**
      (route M3, see the spike above), which emits
      `cargo::rustc-env=NROS_BOARD_FRAMEWORK` plus `rerun-if-changed` on every
      file it reads. Expansion consumes the variable; the metadata answer wins
      when both it and the in-tree table have one.
- [ ] `nros sync` generates that build script into the leaf, the way it already
      generates `.cargo/nros-board.toml` (phase-341's generate-commit-gate
      machinery, reused rather than re-invented). 9 of 92 leaves have a build
      script today.
- [ ] Gate that the board is a DIRECT dependency of the Entry — every route
      fails on a transitive board, M2 silently. True for all 60 leaves today,
      which is exactly when an invariant is cheap to lock in.
- [ ] **A `framework = "<unknown string>"` must be a compile error naming the
      accepted values**, not a fall-through to `OwnedSpin`. The current
      fall-through is what makes 0415 silent; keeping it while adding the
      metadata read fixes only the boards that spell it right.
- [ ] `Framework::Embassy`'s emit branch stops being reachable only from the
      macro's own parser tests — W3's acceptance covers Rtic; Embassy needs at
      minimum a compile-check fixture built against an out-of-tree-shaped board
      crate, or it is untested emit that merely looks reached.

**Gate:** a test asserting the two readers agree for every in-tree deploy key —
one call each, same answer. Two readers over one SSoT is acceptable (they live in
crates that cannot depend on each other's parents); two *mappings* is not, and
only a gate keeps that distinction real.

**Acceptance:** a board crate outside this repo, declaring
`framework = "embassy"`, expands to `#[embassy_executor::main]`. Verified by a
fixture that lives outside the workspace the same way the copy-out check does
(`scripts/zephyr/check-copy-out.sh` is the existing shape), not by a unit test
that constructs the input in memory.

### W2 + W3 LANDED 2026-08-12 — Rust runs on a real Zephyr board

The blocker was never the patch, it was the workspace: one already existed at
the legacy sibling path `../nano-ros-workspace` (which `just/zephyr.just` falls
back to), so nothing needed downloading. The 2026-08-10 "BLOCKED" note looked
only for the in-tree `zephyr-workspace/`.

**Reproduced first, exactly as issued** — `just zephyr build-one rust/talker
zenoh mps2_an385`: 4 x `error[E0061]: this function takes 6 arguments but 5
arguments were supplied`, matching 0432's count.

**One correction to the diagnosis, found by reading the devicetree.** 0432 (and
the `mps2-an385.conf` comment) describe the arity bug as the generator dropping
a cell. It is not: `arm,mps2-fpgaio-gpio` declares **`#gpio-cells = <1>`**, so
`gpios = <&gpio_led0 0>` is complete and correct and there IS no flags cell to
emit. The mismatch is that `GpioPin::new` assumes a two-cell gpio. My first
patch — pad to the CONTROLLER's declared cell count — therefore did nothing,
and the build failed identically; the fix is to pad to the CONSTRUCTOR's arity,
which is what the C side does with `DT_PHA_BY_IDX_OR(..., flags, 0)`.

**Shipped**

| piece | where |
| --- | --- |
| both upstream fixes | `scripts/zephyr/zephyr-lang-rust-gpio-patch.sh` — grep-guarded, idempotent, skips cleanly when the module is absent |
| in-tree delivery | called beside `aarch64-rust-patch.sh` at all four existing sites (3 setup, 1 ci) |
| downstream delivery | `zephyr/patches.yml` gains `zephyr-lang-rust-gpio.patch` with its sha256 — the FIRST non-`zephyr` module in that file, so its SCOPE comment was widened rather than left claiming a narrower scope |
| the witness | `zephyr-fixture-leaves.sh`'s Cortex-M block builds `rust` beside `c`/`cpp`; `matrix::CELLS` gains `(ZephyrQemuCortexM, Rust, Zenoh, Pubsub, Example, Runtime)` |

**Verified, not inferred.** The patch script was tested against a *pristine*
module (`git checkout` first, since my hand-edits had moved its anchors): both
hunks apply, a second run reports "already applied". The witness then built
through the fixture path that carries `cmake/zephyr/mps2-an385.conf` —
`zephyr/mps2_an385/rust/talker/zenoh`, exit 0, producing a real ARM ELF:

```
   text    data     bss     dec     hex
 449876    4075 1613231 2067182  1f8aee  build-cortex-m-rs-talker-zenoh/zephyr/zephyr.elf
```

**A note on the intermediate failure**, because it is the kind that reads as a
regression: with the arity fixed, the link then failed on
`undefined reference to z_impl_sys_rand_get`. That is not a defect — it is the
board conf's `CONFIG_TEST_RANDOM_GENERATOR=y`, which `build-one` does not apply
and the fixture path does. The board conf already documents exactly this.

### W2 (original plan) — the `zephyr-lang-rust` patches reach downstream consumers

- [ ] Author the two upstream fixes: emit the second phandle cell (`dt_flags`)
      for `!Phandle gpios` instances in `zephyr-build`'s devicetree generator,
      and add the missing `cfg: CONFIG_GPIO` to the `gpio-keys` augment in
      `dt-rust.yaml`.
- [ ] Deliver in-tree via `scripts/zephyr/` — the path that already patches
      `modules/lang/rust` (§2.2). Follow the existing scripts' shape, including
      the "already applied" idempotence check.
- [ ] Add the `patches.yml` entry with `module: <the lang/rust module's west
      project name>` — **the first non-`zephyr` module in that file**, so the
      file's scope comment must be updated in the same commit rather than left
      contradicting the content.
- [ ] File upstream (`zephyr-lang-rust`), flag `upstreamable: true`, and follow
      `docs/development/zephyr-upstreaming.md` for the per-patch metadata.

**Acceptance:** with the patch applied, a Rust example builds for `mps2_an385`
(a board with gpio nodes) — which is W3's precondition, so W3 IS the acceptance.

### W3 — delete the carve-out that documents the block

- [ ] Add `rust` to the Cortex-M witness block's `cm_lang` loop in
      `scripts/build/zephyr-fixture-leaves.sh`, and the matching row to
      `matrix::CELLS`.
- [ ] Delete the "No rust leaf … issue 0432" comment in the same commit — a
      carve-out comment outliving its cause is how the next reader concludes the
      block is still true.
- [ ] Check the port allocation: the block derives its locator from
      `alloc::port_of(ZephyrQemuCortexM, {C,Cpp}, Pubsub)` = 10700 / 10800 and
      computes `cm_port=$((10600 + cm_lang_idx * 100))`. A third language needs
      its allocator entry, not a third literal.

**Acceptance:** the new leaf builds in `just zephyr build-fixtures` and its cell
runs. **Native_sim passing proves nothing here** — native_sim has no gpio nodes,
which is the entire reason 0432 hid until phase-337 W2.b.

## 4. Sequencing

```
W1  (independent, in-repo, unblocks every out-of-tree framework board)
W2 ──▶ W3   (W3 cannot build until W2's patch exists)
```

W1 first: it is entirely in-repo and its value does not depend on an upstream
round-trip. W2 has a real upstream latency and W3 is gated on it, so starting W2
early and letting it run in the background is reasonable — but do not let W3's
absence hold W1.

## 5. Tier

W1 changes `packages/core` (the proc-macro) and codegen ⇒ **tier 2
(`just ci-matrix`)**, per RFC-0061, with `just build-test-fixtures lane=tier2`
first. W2/W3 are Zephyr-lane only and their acceptance is the west build, which
neither tier 1 nor tier 2 covers — run `just zephyr build-fixtures` plus the new
cell explicitly and say so, rather than reporting a tier green as if it had
exercised them.

## 6. What is NOT verified yet

* ~~**The board crate's filesystem path for an out-of-tree deploy.**~~
  **ANSWERED 2026-08-10 by the W1 spike above** — resolvable, but not from the
  macro's expansion context: the resolution moves into the Entry's build script.
  The phase's real finding was that expansion-time reads (of env or of files) are
  invisible to cargo's fingerprint, so the obvious route silently serves a stale
  or empty answer, which is the very defect 0415 is.
* **Whether `[patch.crates-io]` preserves the direct dependency edge** the M3
  build script walks. The spike modelled the path-dep shape; 39 of 60 leaves use
  a bare version plus an inline patch row. Patching replaces a package's SOURCE
  and not the graph edge, so the walk should hold — but it was not run, and it is
  the majority shape.
* **The west project name for the `lang/rust` module** — `patches.yml`'s `module:`
  key takes a west project name, and §2.2 confirms only that the scripts patch
  the `modules/lang/rust` *directory*. Read the manifest before writing the entry.
* **Whether the two upstream fixes are sufficient** — 0432 measures 4 errors with
  `CONFIG_GPIO=y` and 14 with `=n`, and attributes both to these two causes. That
  attribution is the issue's, unre-derived here.
* **Whether an `mps2_an385` Rust image fits** — the C witness exists partly
  because Rust images are larger; issue 0477 is a ROM overflow on another
  platform. W3 may surface a size problem that is not 0432's fault.

# Phase 346 — The RFC-0064 seam, actually reachable from out of tree

**Status (2026-08-10). OPEN — nothing landed; the measurements below are done
and reproduce on this tree.** Two issues, one claim: RFC-0064 says a board
arrives through an integration shell that nano-ros never sees, and today that
path is blocked in two independent places — one silently (issue 0415), one
loudly (issue 0432). Neither touches the build-cache program, so this phase runs
in parallel with [phase-340](phase-340-build-artifact-reuse.md) and
[phase-345](phase-345-one-door-build-parity.md) with no fence.

**Closes:** issue 0415, issue 0432.
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

### W1 — the framework mapping has one SSoT, and an unknown framework is an ERROR

- [ ] Move the deploy→`Framework` mapping into **`nros-orchestration-ir`**,
      beside `board_path_for`. Both existing readers delegate to it: the macro's
      `framework_for` and `check_workspace.rs`'s `read_board_framework`.
- [ ] Resolve the board crate for an out-of-tree deploy through the **Entry
      package's board dependency**, read `[package.metadata.nros.board]
      framework` from that crate's manifest at expansion, and let the metadata
      answer win when both it and the in-tree table have one.
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

### W2 — the `zephyr-lang-rust` patches reach downstream consumers

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

* **The board crate's filesystem path for an out-of-tree deploy** — W1's second
  checkbox is the unbuilt half. The Entry's board dependency is the proposed
  route, but whether it is resolvable from the macro's expansion context
  (manifest dir + `toml`, with no cargo metadata call) has not been demonstrated.
  If it is not, that is the phase's real finding and W1 needs a different shape,
  not a wider table.
* **The west project name for the `lang/rust` module** — `patches.yml`'s `module:`
  key takes a west project name, and §2.2 confirms only that the scripts patch
  the `modules/lang/rust` *directory*. Read the manifest before writing the entry.
* **Whether the two upstream fixes are sufficient** — 0432 measures 4 errors with
  `CONFIG_GPIO=y` and 14 with `=n`, and attributes both to these two causes. That
  attribution is the issue's, unre-derived here.
* **Whether an `mps2_an385` Rust image fits** — the C witness exists partly
  because Rust images are larger; issue 0477 is a ROM overflow on another
  platform. W3 may surface a size problem that is not 0432's fault.

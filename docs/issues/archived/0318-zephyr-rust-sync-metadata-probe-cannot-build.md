---
id: 318
title: "`build-rust-examples` fails: `nros sync`'s metadata probe must compile the `zephyr` crate, which needs a Zephyr cmake configure that sync itself gates"
status: resolved
type: bug
area: build
related: [issue-0314, phase-307]
---

## Finding (2026-07-28)

Every `examples/zephyr/rust/*` build failed, so `just zephyr build-rust-examples`
— and therefore `build-examples` — could not complete. Issue 0314 restored the
other three recipes in that chain; this was the fourth and last.

The surface error is a dependency that cannot be resolved:

```
error: no matching package named `zephyr-build` found
location searched: crates.io index
required by package `nros_zephyr_talker v0.1.0`
```

Reproduced under **both** backends (`rust/talker zenoh` and `rust/talker xrce`),
so it has nothing to do with RMW selection.

## Cause: two layers, and the second is the real one

**Layer 1 — the crates are not on crates.io.** A Zephyr Rust app declares
`zephyr = "0.1.0"` and `zephyr-build = "0.1.0"` registry-style, and Zephyr's own
CMake resolves them by passing patches on the cargo command line at BUILD time
(`zephyr-workspace/modules/lang/rust/CMakeLists.txt`):

```cmake
"--config" "patch.crates-io.${module}.path=\"$CACHE{RUST_MODULE_DIR}/${module}\""
```

It also writes a *sample* `.cargo/config.toml` purely so IDEs work — the comment
there says the real build overrides it. So any cargo invocation outside the west
build legitimately cannot resolve these names. `nros sync`'s source-metadata
refresh is exactly such an invocation.

**Layer 2 — patching is not enough, because the probe must COMPILE it.** The
refresh (phase-307) builds a host harness that path-depends on the example and
`cargo run`s it to extract node metadata. That drags in the real `zephyr` crate,
whose build script needs the Zephyr build environment:

```
thread 'main' panicked at zephyr-build/src/lib.rs:47:43:
DOTCONFIG must be set by wrapper: NotPresent
```

`DOTCONFIG` points at the generated `.config` produced by the Zephyr **cmake
configure** — which has not run, and cannot, because this `nros sync` is its
prerequisite. The probe is not slow here; it is **inapplicable**, and no amount
of dependency patching changes that.

## Fix

Pass `--no-metadata` to the `nros sync` in `build-one`'s `rust/*` branch
(`just/zephyr-dev.just`). The flag already exists (phase-307 W2). Codegen — the
reason the step is there, generating the `generated/*` interface crates — is
unaffected and still runs.

## Rejected: teaching `nros` to splice SDK patches

I first added a generic `NROS_EXTRA_PATCH_CRATES` env var so a caller could hand
`nros sync` fully-resolved `name=/abs/path` rows for SDK crates, with the Zephyr
knowledge staying in the parent repo's justfile (`packages/cli/CLAUDE.md`:
"`nros` is a generic tool — it must not learn the nano-ros directory layout").

It worked as designed and was still wrong. `zephyr-build` resolved, and the
build then failed one layer deeper on `DOTCONFIG` — which is what established
layer 2 above. Since `--no-metadata` fixes the actual problem in one flag, the
env var was **reverted rather than kept**: it would have been ~100 lines of
mechanism that solved nothing on its own.

Recording it because the intermediate state is the evidence for the diagnosis —
"patch the deps" is the obvious first move and it is worth knowing it does not
suffice.

## Receipts

- `just zephyr build-one rust/talker zenoh` → `zephyr.elf`.
- `just zephyr build-rust-examples` → rc=0, **6 ELFs** (was: failed at the
  first example).
- Counted by `Built: …/zephyr.elf` lines, not exit status — issue 0314 is a
  standing reminder that rc=0 can lie in these recipes.

## Caveat to watch

`--no-metadata` "leaves any existing sidecars untouched and makes bakes fall
back to the SystemModel's entity lower bound". A standalone example has no
SystemModel, so the bake falls back to whatever the macro derives from the
leaf's own declarations. That is the right input for a standalone leaf, but it
has not been verified at RUNTIME here — these six fixtures were checked for
link, not behaviour. If a Zephyr Rust example later shows a too-small executor
arena or a dropped callback slot, this is the first thing to re-examine.

## Follow-up: the flag is gone; this was a detection gap (2026-07-28)

`--no-metadata` worked but was blunt — it suppressed probing for *every*
component in the workspace, not just the Zephyr leaf. The real defect was one
predicate, and it lived in issue 0288's own mechanism.

`orchestration/workspace.rs` decided deploy-boundness with:

```rust
let deploy_bound = nros.entry.is_some();
```

`[entry]` and `[deploy.<target>]` both mean "this package is bound to a deploy
target", but only the first spelling was checked:

| package | tables | outcome |
| --- | --- | --- |
| `qemu-arm-baremetal/rust/action-client-rtic` | `[entry]` + `[node]` | deploy-bound → probe skipped, degrades |
| `zephyr/rust/talker` | `[node]` + `[deploy.zephyr]` | fell through → probe ran → `DOTCONFIG` failure |

So the Zephyr leaf was never a special case. It was one of **27** standalone
examples (freertos, nuttx, threadx-linux, zephyr) that spell deploy-boundness
with `[deploy.*]` and were all reachable by the host probe that cannot compile
them — they dep a board crate directly, or the SDK's `zephyr` crate.

Fix:

```rust
let deploy_bound = nros.entry.is_some() || !nros.deploy.is_empty();
```

**Over-triggering was the risk worth checking, and it does not occur.** Losing
exact executor sizing for a package that *is* probeable is precisely the
regression issue 0288 warns about, so: no colcon-workspace Node pkg
(`src/<pkg>`, the probeable shape) carries a `[deploy.*]` table — deploy lives
on the Entry pkg there. Verified across the whole tree; all 27 affected
packages are standalone examples. A unit test pins the guard.

The degrade message also said "node + entry in one crate", which stopped being
true; it now says "node + deploy target in one crate".

### Receipts

- `just zephyr build-one rust/talker zenoh` → green with the flag REMOVED, and
  the log shows the skip rather than a failure:
  `ws sync: source metadata — no producer for zephyr_talker::talker
  (deploy-bound: node + deploy target in one crate)`.
- `just zephyr build-rust-examples` → rc=0, 6 ELFs, 6 skip lines.
- Three unit tests: `[deploy.*]` alone marks deploy-bound; `[entry]` still
  does; a plain node pkg stays probeable (the over-trigger guard).

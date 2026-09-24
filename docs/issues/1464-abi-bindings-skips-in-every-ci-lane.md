---
id: 1464
title: "`check-abi-bindings` skips in every CI lane — no image installs
  bindgen-cli, so nothing has ever verified the committed bindgen output"
status: open
type: bug
area: [ci, tooling]
severity: medium
found: 2026-09-23
related: [1226, 1040, 1043]
---

## What happens

Every scheduled tier-2 run prints

```
[SKIPPED] abi-bindings: bindgen-cli not installed (cargo install bindgen-cli --locked --version 0.72.1)
```

The skip itself is correct in SHAPE: `just/check/abi.just` goes through
`nros_check_skip`, so this is issue 1043's third outcome — NOT VERIFIED, with
a named remedy — rather than a green that means nothing. A tier-2 runner
without bindgen is a host fact, and a reported skip is the honest answer.

The problem is that no other lane answers it either.

## What was measured

`bindgen` appears in neither CI image:

```
$ grep -rn bindgen ci/docker/ci-base/Dockerfile ci/docker/zephyr-ros/Dockerfile
(nothing)
```

`ci-base` (`ghcr.io/newslabntu/nano-ros-ci:humble`) is the `container:` for
gate.yml's `check` job, which is where `check-fast` — and therefore
`check-abi-bindings` — runs on the merge-gating events. The only `bindgen`
matches in `.github/workflows/` are `libclang` installs for `zephyr-sys`'s
`build.rs`, which is a different tool doing a different job.

So the gate runs nowhere that has what it needs, in any lane, on any event.

## Why it matters

CLAUDE.md's rule: the C headers are the SSoT and Rust consumes COMMITTED
bindgen output (`packages/rmw/cffi/`, `packages/platform/nros-platform-cffi/`,
`packages/boards/nros-board-cffi/`). `check-abi-bindings` is the only thing
that notices a header edit landing without `scripts/gen-abi-bindings.sh`. A
contributor with bindgen installed locally is what stands between that and a
silent mismatch — and the gate reads as coverage to everyone else.

This is issue 1226's shape a lane over: a gate that WORKS is not a gate that
RUNS. 1040 (`check-default-gates-run-somewhere`) asks whether a gate is NAMED
by a lane; it cannot ask whether the lane's host can execute it, and a
`nros_check_skip` is indistinguishable from a pass in an aggregate verdict.

## Not this

* **Not a bad skip.** Do not make the gate fail-closed on a host without
  bindgen: that reddens every tier-2 run for a reason that is not about the
  bindings, which is the signal loss issue 1158 already records.
* **Not tier 2's problem.** Tier 2 is where it was noticed. `check-fast` in
  the gate lane is where it should be answered.

## What would close it

Either `cargo install bindgen-cli --locked --version 0.72.1` in `ci-base` (and
the `-rN` tag revision it implies), or a rule that a `nros_check_skip` must
name at least one lane whose host provides the missing tool — so a gate nobody
can run is a failure at the ledger rather than a green line in every log.
Prefer the first; the second is the general fix and is a bigger piece of work.

Measured 2026-09-23 from run-matrix 35826999550 and the two Dockerfiles at
`fcba471be`.

## Measured 2026-09-24 — never checked, and currently CORRECT

"Never verified" and "wrong" are different findings, and it is the second one.
With bindgen-cli 0.72.1 installed locally, `scripts/gen-abi-bindings.sh`
regenerates all three surfaces and reports:

```
unchanged packages/rmw/cffi/src/generated.rs
unchanged packages/platform/nros-platform-cffi/src/generated.rs
unchanged packages/boards/nros-board-cffi/src/generated.rs
```

Mutation control (the measurement is not a no-op): appending one declaration to
`packages/boards/nros-board-cffi/include/nros/board.h` and rerunning gives
`regenerated … (38 lines)` and a 3-line diff; reverting the header restores
`unchanged` and a clean tree. The script resolves the repo from
`dirname "$0"/..`, NOT from `NROS_REPO_DIR`, so this measurement is about the
worktree it ran in — the trap that makes `nros-cbindgen-headers` print
"unchanged" for the wrong checkout does not apply here.

So the priority is the gap, not a live mismatch.

## Measured in the image, which changed the fix

Probing `ghcr.io/newslabntu/nano-ros-ci:humble` directly:

* `bindgen` is absent — confirms the report.
* `libclang` is **already there**: `clang` (added for `check-api-parity`) pulls
  `libclang1-14`, and `/usr/lib/x86_64-linux-gnu/libclang-14.so.1` is present.
  bindgen `dlopen`s that at run time and has no `DT_NEEDED` on it, so installing
  bindgen-cli needs no additional apt package.
* `rustup toolchain list` returns `stable-…` and `nightly-2026-04-11-…` and
  **no `nightly`** — and `scripts/gen-abi-bindings.sh` was the last bare
  `rustfmt +nightly` in the tree, wrapped in `2>/dev/null || true`. There,
  rustup does not fail: it goes to the network and installs whatever nightly is
  current, so the formatter deciding the committed bytes would be an unpinned
  moving target. `scripts/api_parity/extract_rust.py` already records this exact
  hazard for rustdoc. **Installing bindgen without fixing this would have
  converted a silent skip into a flapping red that is not about the headers.**

Also measured, because it decides whether the image's clang is a second moving
input beside the bindgen pin: forcing `LIBCLANG_PATH` at libclang **12** and
**14** produces all three files byte-identical. It is not.

And the pass being fixed is currently a NO-OP: bindgen-cli formats its own
output with the default toolchain, and stubbing out the `rustfmt +nightly` call
entirely leaves all three files byte-identical. The defect is latent — which is
how long it would have stayed invisible once the gate started running in CI.

## What landed, and why this option

`cargo install bindgen-cli --locked --version 0.72.1` in `ci/docker/ci-base/
Dockerfile`, plus the pinned-nightly fix above. The alternatives, judged:

* **A dedicated lane** pays the same `cargo install` PER RUN instead of once per
  image, and adds a check surface, to answer a question `check-fast` is already
  asked in a container that is already pulled.
* **Vendoring the check differently** (a committed header hash) answers "did a
  header move", not "do the bindings match the headers", and is a second SSoT.
* **A narrower fail-closed** (`NROS_ABI_BINDINGS_STRICT=1` on lanes that can
  satisfy the gate, the issue-1043 pattern) is the right BELT — it is what stops
  a future image regression from silently restoring the skip. It cannot land in
  the same change: `images.yml` republishes `humble` only after a merge to
  `main`, so setting it now would redden the very PR that adds bindgen. Same
  ordering that file already documents for moving a `container:` tag.

Cost of the chosen option, measured: one `cargo install` next to the
`cargo-nextest` one already in that file, and an 8.6 MB binary in an 8.1 GB
image. The tag is content-derived (`humble-<sha12>` from a sha over the
Dockerfile and its COPYed inputs), so there is no `-rN` revision to bump — the
parenthetical in "What would close it" above is stale.

No gate holds the Dockerfile's `BINDGEN_VERSION` and the script's `BINDGEN_PIN`
in lockstep, deliberately: the script already refuses to run when
`bindgen --version` differs from its pin and names the drift, so a wrong number
in the image is a loud, correctly-diagnosed failure rather than a bindings diff
blamed on the headers.

## Why this is still `open`

At the moment this lands, `ghcr.io/newslabntu/nano-ros-ci:humble` has not been
rebuilt — `images.yml` publishes on push to `main`, after the merge. The gate
therefore still skips in the lane until then.

**One observable closes this:** the `check` job printing
`ABI bindings match the C-header SSoT.` in place of
`[SKIPPED] abi-bindings: bindgen-cli not installed`. Confirm it on the first
`check` run after `images` publishes, then add the `NROS_ABI_BINDINGS_STRICT`
belt above and archive.

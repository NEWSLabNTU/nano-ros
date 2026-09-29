---
id: 1474
title: "The Zephyr Rust fixture build runs `cargo clippy` as a ninja step, and the
  container's stable toolchain has no clippy component — so `live-peer` dies in its
  fixture build with an error about a missing rustup component"
status: open
type: bug
area: ci, zephyr, build
severity: medium
found: 2026-09-24
related: [1364, 1353]
---

## What happens

`live-peer regression` run **35954986426** (schedule, 04:16), job
**107491570915** (`rows whose board is NOT this runner`), step
**`Build the fixtures those rows resolve`**:

```
[1305/1313] Linting Rust application
error: the 'cargo-clippy' binary, normally provided by the 'clippy' component,
       is not applicable to the 'stable-x86_64-unknown-linux-gnu' toolchain
FAILED: run_rust_clippy /github/home/.nros/workspaces/zephyr/3.7/build-ws-rs-qos-entry-zenoh/run_rust_clippy
ninja: build stopped: subcommand failed.
FATAL ERROR: command exited with status 1: /usr/bin/cmake --build .../build-ws-rs-qos-entry-zenoh
make: *** [...: zephyr-fixture-1-build-ws-rs-qos-entry-zenoh] Error 1
error: recipe `build-fixtures` failed with exit code 2
```

The failing target is `run_rust_clippy`, a ninja step contributed by
zephyr-lang-rust's `CMakeLists.txt` (`Linting Rust application`), which invokes

```
cargo clippy --no-default-features --features rmw-zenoh --target x86_64-unknown-none … \
  -- -D warnings -D clippy::undocumented_unsafe_blocks
```

Every Zephyr Rust image therefore needs the `clippy` component present for the
toolchain the build resolves, and in this container it is not.

## Why it matters

It stops the fixture build, so `live-peer`'s off-runner job produces **no
verdict** on any interop row — the lane's own summary job says so in as many
words (`live-peer board — NO VERDICT: stopped in the build`). The first Zephyr
Rust fixture in the list is enough to take the whole build down, so nothing
behind it is attempted either.

## What this is NOT

- **Not issue 1364.** That is this same job failing on `ModuleNotFoundError: No
  module named 'tomllib'` / `'tomli'`. Tonight's run gets past that and dies
  later, at the fixture build — so 1364's symptom is gone from this job and
  this is what is behind it. Re-read the error text before attributing this
  job to 1364 again.
- **Not issue 1353.** No disk pressure in this job.
- **Not a clippy finding.** No lint fired; the binary is absent.

## What would close it

Two defensible shapes, and the choice is about where the Zephyr Rust toolchain
is declared rather than about clippy:

1. **Provide the component** where the image or the workspace setup installs
   the Rust toolchain (`rustup component add clippy` for the resolved
   toolchain). Keeps the lint, which is presumably why zephyr-lang-rust runs it.
2. **Do not run the lint in a fixture build.** The fixture build exists to
   produce artifacts; `-D warnings` linting belongs in a `check` lane, where a
   missing component is a provisioning failure rather than a build failure.
   zephyr-lang-rust's CMakeLists is upstream, so this means a knob or a patch
   on our side.

Acceptance is `live-peer`'s off-runner job reaching its cells — a verdict on
the interop rows, green or red, rather than stopping in the build.

## Remedy 1 is already in the tree, it RUNS, and it is a no-op (2026-09-28)

"What would close it" above offers `rustup component add clippy` as the first
shape. A step that does exactly that already exists — **`Unblock rustup
clippy-preview conflict`**, in both `.github/workflows/live-peer.yml` (~line
430) and `.github/workflows/nightly.yml` (~line 788) — and it ran in the
`live-peer` run that failed this morning. So the first remedy has been tried;
what follows is why it does not take.

`live-peer regression` run **36377215634** (schedule, 04:19), job
**108785696260** (`rows whose board is NOT this runner`). The step's own
comment states its intent:

```
# The image bakes bin/cargo-clippy from a non-rustup-owner layer; `rustup
# target add` trips on the clippy-preview conflict. Delete the orphan files
# then re-install clippy on every toolchain.
```

and its body deletes and re-adds:

```
find "$rustup_home/toolchains" -maxdepth 3 -type f \
    \( -name cargo-clippy -o -name clippy-driver \) -delete 2>/dev/null || true
rustup toolchain list … | while read -r tc; do
    rustup component add clippy --toolchain "$tc" || echo "  (clippy add failed …)"
done
```

Its entire output in that run is one line:

```
info: component clippy is up to date
```

Thirty minutes later the same job dies in the fixture build on the error this
issue is about:

```
error: the 'cargo-clippy' binary, normally provided by the 'clippy' component,
       is not applicable to the 'stable-x86_64-unknown-linux-gnu' toolchain
```

**Measured:** the files are deleted, `component add` reports *up to date* rather
than installing anything, and the binary is still missing when the build asks
for it. The step cannot fail — its `|| echo … non-fatal` and the `|| true` on
the `find` mean it reports success whatever happens — so this sequence has been
running green and achieving nothing.

**Inference, stated as such:** deleting the files does not tell rustup they are
gone. Its manifest still records `clippy` as installed for that toolchain, so
`component add` short-circuits with "up to date" and never re-downloads what the
`find` removed. I have not instrumented rustup to prove that; what is proven is
the three facts above, and any explanation has to account for "deleted, then
*up to date*, then absent".

If that reading is right the step needs `rustup component remove clippy` before
the add (or `--force`, or simply not deleting the files), and the `|| echo`
should not swallow a failure the build later depends on. That is a change to a
workflow, so it belongs to whoever owns the runner image decision in remedy 1 —
this appendix only removes the assumption that remedy 1 is untried.

**What this does NOT change.** Remedy 2 (do not lint in a fixture build) is
untouched, and acceptance is unchanged: `live-peer`'s off-runner job reaching
its cells. The sibling job in the same run, `rows whose board IS this runner`,
failed on **issue 1353** (annotation `No space left on device : '…/_diag/
Worker_20260928-041930-utc.log'`, log `BlobNotFound`) — two jobs, two causes, as
this lane usually splits.

## The inference, measured — and the fix (2026-09-29)

The appendix above left one step open, honestly: "I have not instrumented
rustup to prove that". It is now measured, against an isolated `RUSTUP_HOME`
holding a real minimal `1.85.0` toolchain with clippy (so no runner's or
developer's rustup was touched):

| sequence | result |
| --- | --- |
| delete files → `add` (**the workflow step**) | `component clippy is up to date`, 0 binaries, and **CI's exact error**: `the 'cargo-clippy' binary … is not applicable` |
| delete files → `remove` → `add` | **wedged**: `remove` fails (`directory does not exist: 'bin/clippy-driver'`) and rolls back; `add` still says `up to date` |
| untracked orphan → `add` | `detected conflict: 'bin/cargo-clippy'` — the clippy-preview conflict the step was written to solve |
| `remove` → `add` | works |
| `remove` → clear orphans → `add` | **works, in both the clean and the orphan case** |

So the inference was right, and the second row adds what it did not say:
deleting first is not merely ineffective, it leaves rustup unable to recover
by its own commands. rustup decides "installed" from its manifest, so the
files have to be removed BY rustup while they still exist; only after that is
a leftover file an orphan that is safe to delete. The step's author had the
right diagnosis and the wrong order.

**Fix:** `scripts/ci/rustup-restore-clippy.sh`, called by both steps (they were
two hand copies — `live-peer.yml` said "Same fix as nightly's zephyr line").
It runs `remove` → clear orphans → `add`, then **verifies** with `rustup run
<tc> cargo clippy --version`, and exits nonzero if the toolchain this
checkout's builds resolve (`rustup show active-toolchain`) is left without a
working clippy. The old step could not fail; this one fails at provisioning,
naming this issue, instead of thirty minutes later in ninja. Both paths were
exercised against the isolated toolchain: the orphan state is repaired
(`clippy 0.1.85`, rc 0), and the wedged state is refused (rc 1).

Stays **open**: acceptance is `live-peer`'s off-runner job reaching its cells,
which is tonight's run at the earliest. Remedy 2 (do not lint in a fixture
build) is still available and still untried; this closes remedy 1 properly
rather than choosing between them.

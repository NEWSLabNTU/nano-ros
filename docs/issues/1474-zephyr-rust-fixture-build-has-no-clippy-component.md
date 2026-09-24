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

---
id: 619
title: "`cargo test -p nros-c` cannot link: `nros-log`'s platform sink calls
  `nros_platform_log_write`, which no test binary provides"
status: open
type: bug
area: build/api
related: [issue-0618, issue-0617, issue-0420]
---

## Symptom

`just ci-matrix` reaches `test-all` and dies before running a single test:

```
error: linking with `cc` failed: exit status: 1
  = note: /usr/bin/ld: libnros_log-….rlib(…nros_log….rcgu.o): in function `nros_log::sinks::emit':
      packages/core/nros-log/src/sinks.rs:72: undefined reference to `nros_platform_log_write'
      packages/core/nros-log/src/sinks.rs:62: undefined reference to `nros_platform_log_flush'
      collect2: error: ld returned 1 exit status
error: could not compile `nros-c` (lib test) due to 1 previous error
```

Reproduces standalone:

```sh
cargo test --no-run -p nros-c --profile nros-relwithdebinfo --locked
```

## Mechanism

`nros-log`'s `PlatformSink` calls the platform ABI's `nros_platform_log_write` /
`_flush` as `extern "C"`. Those symbols are supplied by a platform's C port
(`nros-platform-posix`, `-zephyr`, `-threadx`, …), which a real image links.

A `cargo test` binary for `nros-c` is a final artifact too, but it links no
platform port — so the sink's calls have no definition. The crate's LIBRARY
builds fine; only its test harness fails, which is why this is invisible until
something runs `cargo test` over the workspace.

This is issue 0618's family in a different register: a library assumes the FINAL
ARTIFACT provides something, and the assumption is unchecked. There it was
`#[panic_handler]` / `#[global_allocator]` (lang items); here it is an
`extern "C"` symbol from the platform ABI. Same root — the provider is decided
outside the crate, and nothing verifies each artifact ends up with exactly one.

## Not caused by the feature campaign

Checked, because the timing invited the assumption: reverting
`packages/api/nros-cpp/Cargo.toml` to its pre-`nros-rmw/alloc` state reproduces
the failure identically, so this is not fallout from phase-361's opt-in
direction and is distinct from 0617's three link failures.

## Impact

Blocks `just ci-matrix` (tier 2) and `just test-all` at the compile step, so no
test result is obtainable from those lanes regardless of the state of anything
else.

## Directions

Not diagnosed further, so these are candidates rather than a plan:

- Give the test harness a platform port — the posix one — as a `[dev-dependency]`
  of `nros-c`, matching what any real consumer links.
- Or make `PlatformSink` weak/optional so a build with no platform port
  compiles to a no-op sink rather than an undefined reference. Note issue 0420
  is exactly the danger there: a silently no-op log facade on threadx/nuttx was
  its own bug, so a no-op must be a deliberate, visible fallback rather than a
  link-order accident.

Whichever way it goes, the general rule from 0618 applies: name what the final
artifact must provide, and check it, instead of letting the linker discover it.

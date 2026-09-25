---
id: 1494
title: "`BuildRungs` reads every platform descriptor on the search path and
  watches none of them, so a knob rung — and now a FATAL missing-descriptor
  refusal — is only re-evaluated when some other input happens to move"
status: open
type: bug
area: [build, core, tooling]
severity: medium
found: 2026-09-25
related: [1401, 0491, 1018, 1145, 1486]
---

## What happens

`BuildRungs::from_build_env()`
(`packages/tooling/nros-platform-config/src/platform_config.rs`) emits exactly
three rebuild edges:

```text
cargo:rerun-if-env-changed=NROS_PLATFORM_NAME
cargo:rerun-if-env-changed=NROS_BOARD
cargo:rerun-if-changed=<$NROS_BOARD_TOML>        # only when that var is set
```

It then loads `PlatformsTree` over the whole search path
(`packages/platform/*/nros-platform.toml` + `config/*/nros-platform.toml`) and
resolves nine knob families out of it. **Not one of those files is a declared
build input.** So editing a descriptor's `[knobs.*]` does not rebuild the crates
whose knobs it sets; the next build uses whatever the fingerprint last cached,
and the edit appears to have done nothing.

The sibling road already does this correctly: `nros-zpico-build`'s
`platform_manifests_to_watch(&platform_search_path)` walks the same roots and
emits a `rerun-if-changed` per descriptor. So the rule has one correct spelling
and one road that skips it — the shape issue 1018 and issue 0491 both record.

## Why it matters more since phase-468 W1

W1 made an unanswered platform name FATAL: `BuildRungs::require_rungs` panics
instead of warning and falling through to the builtin defaults. A fatal verdict
that is not re-evaluated when its input moves is worse than a tolerant one —

* delete or rename the descriptor that answers a name and the already-built
  crates stay green until something unrelated invalidates them, so the failure
  surfaces later and somewhere else;
* the reverse is what this was measured on. Adding `esp32` to
  `config/bare-metal/nros-platform.toml`'s `names` did NOT make a previously
  refused `NROS_PLATFORM_NAME=esp32` build succeed. Measured, on the phase-468
  W1 branch:

  ```text
  $ NROS_PLATFORM_NAME=esp32 cargo build -p nros-params     # names = ["bare-metal"]
  panicked: no nros-platform.toml answers to `esp32`
  $ <add "esp32" to names>
  $ NROS_PLATFORM_NAME=esp32 cargo build -p nros-params
  rc=0                       # ...only because the build script was not re-run
  $ touch packages/core/nros-params/build.rs
  $ NROS_PLATFORM_NAME=esp32 cargo build -p nros-params
  rc=0                       # the real answer
  ```

  The two `rc=0` lines mean different things and print identically. Getting the
  cross-check to answer at all needed a `touch`.

## Not the same as issue 1401

1401 is the BOARD descriptor (`nros-board.toml`) going unwatched when the early
return at the top of `from_build_env` skips the watch. This is the PLATFORM
descriptors, which are never watched on this road at all, whether or not the
early return is taken. They will likely be fixed together; they are not one bug.

## Fix sketch

Emit a `rerun-if-changed` per descriptor file, from the same helper the zpico
runner uses, so there is one spelling rather than a second one. Watch the
DIRECTORY listing too, or a descriptor that appears where none was will not
invalidate anything — an absent file has no path to watch, which is the part
`rerun-if-changed` cannot express on its own and the reason this needs a
measurement rather than a one-line addition.

## Acceptance

* Editing a `[knobs.*]` value in any descriptor on the search path rebuilds the
  crates that read it, asserted rather than assumed.
* Adding a name to a descriptor's `names` clears a `require_rungs` refusal with
  no `touch`.
* One spelling shared with `nros_zpico_build::platform_manifests_to_watch`.

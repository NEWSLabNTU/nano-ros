---
id: 534
title: "`zpico-sys` selecting the `zephyr` platform breaks the Zephyr C zenoh leaves on a missing `version.h`"
status: resolved
type: bug
area: zephyr
related: [issue-0529, issue-0528, phase-348]
---

## Symptom

Zephyr C zenoh fixture leaves fail in `zpico-sys`'s build script:

```
cargo:warning=…/zenoh-pico/include/zenoh-pico/system/platform/zephyr.h:18:10:
fatal error: version.h: No such file or directory
```

Affected in one module run: `build-c-service-client-zenoh`,
`build-c-service-server-zenoh`, `build-c-action-client-zenoh`. It takes the
zephyr fixture module down, and zephyr is an order-only prerequisite of every
other platform, so it blocks `build-test-fixtures` and therefore `ci-matrix`.

## Cause — attributed at HUNK level, not by bisect

`292547dd5` (fix #529) added `zephyr` to `zpico-sys`'s platform selection:

```rust
    } else if use_zephyr {
        Some("zephyr")
```

Its commit message states the case for why this is safe:

> No behaviour change today: `build_c_shim` is skipped on Zephyr (below), so the
> config header these knobs feed has no consumer, and the C lane gets the same
> values from Kconfig via `nros_rmw_zenoh.cmake`.

**The evidence contradicts that.** Neutralising exactly that branch at current
HEAD (`else if false`) and rebuilding the leaf from a pristine build dir:

| tree | result |
| --- | --- |
| HEAD | `version.h: No such file or directory`, exit 2 |
| HEAD with that one branch neutralised | **exit 0**, zero errors |

Nothing else was changed between the two runs. Selecting the platform is
therefore not inert on Zephyr.

**The MECHANISM, corrected.** My first reading above — "naming it turns on the
platform manifest's include handling" — was wrong, and the fix would have been
aimed at the wrong seam. `platform_name` is the condition on
`build_zenoh_pico_unified` (`runner.rs:545`), one call ABOVE the `build_c_shim`
that #529's message checked. Naming the platform is what makes a BUILD SCRIPT
cc-compile the vendored zenoh-pico sources — including `system/zephyr/*.c`,
selected by that manifest's `include = ["system/common", "system/zephyr"]` —
and those need Zephyr's generated `version.h`, which only Zephyr's own build
produces. #529 verified the shim was skipped, which was true, and missed the
unified build entirely.

The #529 change is still right in intent: the resolver SHOULD be total over the
platforms that have a config file, so the next knob added to that table is not
silently ignored. Knob resolution and "who compiles the C" were simply the same
condition, and only one of them should have been.

## Reproduce

```
NROS_ZEPHYR_WORKSPACE=<ws> scripts/build/zephyr-fixture-make-driver.sh \
    --filter 'c/service-client.*zenoh'
```

Reproduces SOLO and from a PRISTINE build dir, so it is neither a parallel-build
race nor stale state — both were checked, because in this area they are the
usual answer.

## Fix — the comment became a field

`config/zephyr/nros-platform.toml`'s `[build.zenoh]` already stated the property,
in prose: *"this block exists so the drift gate has a manifest entry but no cc-rs
consumer hits it."* True when written, enforced nowhere, and #529 read it and
made a cc-rs consumer hit it anyway. So the claim is now checked:

```toml
[build.zenoh]
compiled_by = "platform"
```

* `manifest::CompiledBy { Cargo (default), Platform }` on `PlatformEntry`, as an
  `Option` so an unset CHILD cannot silently downgrade a parent that set it —
  same shape as `pic` and `mbedtls`, and the reason the serde default alone was
  not enough.
* `runner.rs` still RESOLVES the block for every platform, so #529's totality
  survives untouched: the knobs, defines, includes and the drift gate all keep
  reading it. Only the cc build is gated.

A declarative field rather than a fourth `!use_zephyr`: `runner.rs` already
carries three of those, and per-site booleans are exactly how the property stayed
invisible to #529. This one is stated once, where the platform is described.

## Verified

* Unit, both directions — `only_zephyr_delegates_its_c_build_to_the_platform`
  passes with the key, and FAILS with it removed (`left: []`, `right:
  ["zephyr"]`). It also asserts `checked >= 7` so it cannot pass vacuously if the
  tree fails to load. Note `nros-board-common`'s `build-helpers` feature is
  opt-in, so `cargo test -p nros-board-common` alone compiles NONE of this —
  `--features build-helpers` is required (29 tests).
* The leaf that failed — `c/service-client zenoh`, 332 s, zpico-sys compiled,
  `zephyr.elf` linked, native runner built, 0 errors and 0 `version.h` errors.

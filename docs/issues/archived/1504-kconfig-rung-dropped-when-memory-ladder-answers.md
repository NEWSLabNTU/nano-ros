---
id: 1504
title: "nros-platform's heap knob loses its Kconfig rung whenever a platform name is exported"
status: resolved
area: build
severity: high
phases: [468]
rfcs: [0049]
related: [0460, 1490, 1233]
---

# `nros-platform`'s heap knob loses its Kconfig rung whenever a platform name is exported

## The measurement

`packages/platform/nros-platform/build.rs` resolved `NROS_ZEPHYR_HEAP_SIZE`
like this:

```rust
let size = BuildRungs::from_build_env()
    .map(|r| r.memory_value("heap_bytes", DEFAULT_HEAP_SIZE))
    .unwrap_or_else(|| knob_usize("NROS_ZEPHYR_HEAP_SIZE",
                                  "CONFIG_NROS_ZEPHYR_HEAP_SIZE",
                                  DEFAULT_HEAP_SIZE));
```

Three builds of `nros-platform`, each in its own target dir, with a
`$DOTCONFIG` written by hand and `NROS_PLATFORM_NAME` varied. The builtin is
65 536; the probe value is 98 304, chosen because it is neither.

| `$DOTCONFIG` | `NROS_PLATFORM_NAME` | compiled |
| --- | --- | --- |
| `CONFIG_NROS_ZEPHYR_HEAP_SIZE=98304` | `zephyr` | **65 536** |
| `CONFIG_NROS_ZEPHYR_HEAP_SIZE=98304` | unset | 98 304 |
| (symbol absent) | `zephyr` | 65 536 |

Row 1 is the bug: Kconfig said 98 304, the C lane took 98 304, and the Rust
half compiled the crate default. Issue 0460, in the knob whose entire job is
the size of the Zephyr heap — and `NROS_PLATFORM_NAME` is exported by every
build the `nros` road drives (`nros ws board-facts`), so row 1 is the normal
case and row 2 is the accident.

Reproduce (a separate target dir per run, so the answer cannot come from a
cached build-script output — that caching is what makes this easy to measure
wrong):

```sh
printf 'CONFIG_NROS_ZEPHYR_HEAP_SIZE=98304\n' > /tmp/p/.config
DOTCONFIG=/tmp/p/.config NROS_PLATFORM_NAME=zephyr \
  NROS_PLATFORMS_DIR="$PWD/config:$PWD/packages/platform" \
  cargo build -p nros-platform --target-dir /tmp/p/t
grep -rh 'rustc-env=NROS_ZEPHYR_HEAP_SIZE=' /tmp/p/t/debug/build/nros-platform-*/output
```

Drop `NROS_PLATFORM_NAME` (and the platforms dir) for row 2; drop the
`CONFIG_` line for row 3.

## Why

`BuildRungs::memory()` has **no Kconfig rung** and never had one — it resolves
env → board → platform → builtin, with `&|k| std::env::var(k).ok()` as its env
source. So the two arms of that `unwrap_or_else` are not alternatives with one
extra rung between them; they are two different ladders, and taking the first
one silently drops `$DOTCONFIG`.

The crate's own documentation said so, one tenant over. `executor_rungs()`
carries this comment:

> Deliberately not the full ladder: `nros-node/build.rs` composes env → Kconfig
> → these → its own builtin, and the Kconfig rung sits between the front-end
> and the descriptors. What it needed shared was the ENV-POINTER DANCE above,
> not the composition, and **pretending otherwise would have quietly dropped
> its Kconfig rung**.

The memory tenant had no `memory_rungs()` sibling, only the full-ladder
`memory()`, so the one build script that needed a Kconfig rung had nothing to
compose with and chose between ladders instead. The comment above the call even
asserted that `memory_value` composed "env, then Kconfig via `$DOTCONFIG`, then
the platform/board rung" — a claim about a function that does not read
`$DOTCONFIG` at all.

`check-kconfig-knob-forwarding` was green throughout. It asked whether
`nros-platform/build.rs` NAMES `NROS_ZEPHYR_HEAP_SIZE` and whether the file
calls `nros_zephyr_build::knob_usize` somewhere. Both true; neither is "the
call that names this knob is the one that runs".

## The fix (phase-468 W4)

The two are RUNGS, so they compose:

```rust
let rung = BuildRungs::from_build_env().and_then(|r| r.memory_rungs().heap_bytes);
let size = nros_zephyr_build::knob("NROS_ZEPHYR_HEAP_SIZE")
    .rung(rung)
    .resolve(DEFAULT_HEAP_SIZE);
```

`memory_rungs()` is new and mirrors `executor_rungs()` / `rmw_rungs()` /
`param_rungs()` — platform merged with board, board winning, no env, no Kconfig,
no defaults. `knob()` is the tree's one knob ladder, and the rung is an INPUT to
it rather than a rival composition.

This is the structural half of why the box existed: with the Kconfig source
reachable only as a *separate call path*, choosing the wrong path is writable
and looks reasonable. As an input it cannot be dropped without deleting a line
that says `.rung(...)`.

## Acceptance

- [x] Row 1 of the table above reads 98 304. Measured after the fix, all three
      rows: 98 304 / 98 304 / 65 536.
- [x] `memory_rungs()` exists and states why it is not `memory()`.

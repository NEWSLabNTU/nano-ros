---
id: 614
title: "`cargo check -p nros-c` with no features fails on the host, and the error
  names none of the cause"
status: resolved
type: tech-debt
area: api-c, api-cpp
related: [phase-361, issue-0582, issue-0594]
resolved_in: "7f4da362c"
---

> **RESOLVED by `7f4da362c`** — "three ways it broke", and it fixed each at the
> level it belonged to rather than papering the symptom:
>
> 1. **No panic handler.** `default = ["panic-spin"]` on both crates. That
>    restores a complete configuration WITHOUT reintroducing a `std` default:
>    every real consumer takes these crates `default-features = false` and picks
>    its own provider, so nothing downstream changes.
> 2. **`unwinding panics are not supported without std`.** That is the panic
>    STRATEGY, which cargo accepts only per PROFILE — no feature could have
>    fixed it. `[profile.dev] panic = "abort"`, which is also the honest setting:
>    every embedded profile already aborts and every shipped image does, so
>    `dev` was the outlier. Cargo forces `unwind` for `test`/`bench` regardless,
>    so `#[should_panic]` and the harness's `catch_unwind` skip-classification
>    are untouched.
> 3. **`cannot find type ConcretePlatform`** in `nros_cpp_time_ns`'s no_std arm —
>    a compile-time requirement for what is really a LINK-time fact. Now reached
>    as `nros_platform_clock_ns`, the same contract the wake primitives use.
>
> The whole table above re-run 2026-08-16, all four green:
>
> | invocation | result |
> | --- | --- |
> | `cargo check -p nros-c` | passes |
> | `cargo check -p nros-c --features std` | passes |
> | `cargo check -p nros-cpp` | passes |
> | `cargo check -p nros-cpp --features std` | passes |
>
> Note (2) is the part this issue's own "Options" section would have missed: all
> three options here were documentation or a `compile_error!`, and the strategy
> error could not be reached by either — a feature cannot set `panic = "abort"`.
> Filing something as discoverability is a judgement about the cause, and this
> one had a real defect underneath it.


## Symptom

```
$ cargo check -p nros-c
error: `#[panic_handler]` function required, but not found
error: unwinding panics are not supported without std
error: could not compile `nros-c` (lib) due to 2 previous errors
```

Same for `-p nros-cpp`. Adding `--features std` makes both pass:

| invocation | result |
| --- | --- |
| `cargo check -p nros-c` | **fails** (2 errors, above) |
| `cargo check -p nros-c --features std` | passes |
| `cargo check -p nros-cpp` | **fails** |
| `cargo check -p nros-cpp --features std` | passes |

`just check c` and `just check cpp` pass, because they name features. Nothing in
CI is red.

## Cause, and it is deliberate

Phase-361 W3 set `default = []` on every `no_std`-capable crate, `nros-c` and
`nros-cpp` among them, and recorded the consequence: *"Breaking for out-of-tree
consumers: `nros-core = "0.5"` is now a `no_std` build. Needs a release note."*
The crate's own manifest documents this exact failure next to `panic-spin`:

> It used to be gated `all(global-allocator, not(std), not(panic-halt))` … with
> phase-361 W3's `default = []`, a plain host build of this crate had neither and
> died on `#[panic_handler] function required`.

So the behaviour is understood and intended. W8.b's `panic-spin` gives a way to
say "this image needs a panic handler" without also asking for an allocator; it
is not in any default path, because there is no default path any more.

## Why file it anyway

`cargo check -p <crate>` is what a developer types, and here it fails with two
errors that name neither the crate's feature contract nor the fix. Nothing says
"pass `--features std` for a host check". The reader's first hypothesis is that
main is broken — mine was, and I reported it as a regression from a rebase
before checking, which was wrong.

This is discoverability, not correctness. Filed at that weight deliberately: the
build works, the lanes are green, and the fix is a sentence in the right place
rather than a code change.

## Options

* a `compile_error!` on the no-features host build naming the remedy —
  `nros-node` already has two feature-naming `compile_error!` guards (phase-361
  W8.e), so the idiom exists;
* or a line in `packages/api/nros-c/README.md` + the crate docs: host checks need
  `--features std`, embedded builds name a platform;
* or leave it and accept that the crates are only ever built through cmake /
  corrosion / `just check-*`, which all pass features — in which case say so in
  the manifest so the next person stops at the note instead of at a bisect.

## Not

* not a regression: `--features std` has always been the working invocation;
* not the `nros-cpp` E0004 non-exhaustive-match break (that one WAS a real
  regression, from issue 0586's exhaustive mapper, caught by the cortex-m Zephyr
  leaves and fixed the same day);
* not caused by `cargo check` running against a held build lock — the numbers
  above were re-taken to completion after the fixture build finished, because
  the first readings were taken while it held the lock and were wrong.

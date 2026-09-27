---
id: 1507
title: "Four of the six `#[ignore]`d compile-check tests in `rosidl-codegen` fail,
  and no recipe or workflow runs them by name — so nothing has ever asked"
status: open
type: bug
area: testing, tooling
severity: medium
found: 2026-09-27
related: [issue-0652, issue-0612, issue-0667, issue-0693, issue-1392]
---

## What was measured

Found while ruling issue 1392's nested-cargo sites: those sites live in
`packages/cli/rosidl-codegen/tests/{heap,cpp_heap}_compile_check.rs`, so the
tests were RUN to confirm the ruling worked in situ. Four of six fail on a
tree whose `check-nested-cargo-lock-discipline` and `just check fast` are green.

```
cargo test --manifest-path packages/cli/Cargo.toml -p rosidl-codegen \
    --test heap_compile_check --test cpp_heap_compile_check -- --ignored
```

| test | verdict |
| --- | --- |
| `heap_compile_check::generated_heap_message_compiles` | FAIL |
| `heap_compile_check::generated_service_with_dheader_wrap_compiles` | ok |
| `heap_compile_check::generated_c_with_dheader_wrap_syntax_checks` | FAIL |
| `heap_compile_check::generated_c_service_with_dheader_wrap_syntax_checks` | FAIL |
| `heap_compile_check::generated_c_action_with_dheader_wrap_syntax_checks` | FAIL |
| `cpp_heap_compile_check::generated_heap_cpp_compiles` | FAIL (second half) |
| `cpp_heap_compile_check::generated_fixed_string_serialize_truncates_at_nul_garbage` | ok |

Both failures are independent of issue 1392's change: the nested cargo RAN in
each case and reported a genuine diagnostic, and `git diff origin/main` over
those files adds only the `--config resolver.lockfile-path=…` argument and its
comment.

## Two distinct causes

**1. The generated heap crate asks for `nros_core::heap` and does not enable
`alloc`.** `generated_heap_message_compiles` writes a temp manifest with
`nros-core = { path = "…/nros-core" }` — no features — and `nros-core` has
`default = []`, so:

```
error[E0433]: cannot find `heap` in `nros_core`
  --> src/msg/frame.rs:11:28
   |
11 |     pub pixels: nros_core::heap::Vec<u8>,
note: found an item that was configured out
   --> packages/core/nros-core/src/lib.rs:139:9
138 | #[cfg(feature = "alloc")]
```

So the test cannot have passed since `heap` went behind `alloc`. Whether the fix
belongs in the test's generated manifest (`features = ["alloc"]`) or in what
codegen emits for `mode = "heap"` is the question to answer — a consumer of
generated heap code has the same problem, and the test is the only thing that
would have said so.

**2. The generated `.h`/`.hpp` `#include <nros/nros_config_generated.h>` and
nothing provides it.** The three `generated_c_*_syntax_checks` and the g++ half
of `generated_heap_cpp_compiles` add `-I <repo>/target/nros-c-generated`, a
per-build directory that does not exist in a checkout that has not built the C
lane:

```
my_msgs_msg_frame.hpp:19:10: fatal error: nros/nros_config_generated.h: No such
file or directory
```

That is a PRECONDITION the tests do not state. Per the `check-no-vacuous-tests`
rule they are right to fail rather than skip, but they should fail with the
remedy named (the way `bare_metal_link.rs::require_bare_metal_target` does for
`thumbv7m-none-eabi`), not with a compiler's include error — or the header
should come from somewhere a test can rely on.

## A third instance, already fixed, which is what makes the class the point

`packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/bare_metal_link.rs`'s
`workspace_root()` climbed `.nth(3)` from `CARGO_MANIFEST_DIR`, which lands on
`packages/` and not the repo root — so `bare_metal_no_std_clean` built fine (cargo
walks UP from `current_dir` and finds the real workspace regardless) and then
asserted on `…/packages/target/thumbv7m-none-eabi/debug/deps`, which does not
exist. `alloc_free_audit.sh` beside it already computed the root correctly; only
the shell half was updated when phase-321 W2.d moved the group one level deeper.
Fixed to `.nth(4)` in issue 1392's commit, because that file's `--locked` ruling
had to be observable. Both tests in it now pass.

Three broken `#[ignore]`d tests in two crates, found by running them once. That
is the finding, not any one of the three.

## Why nobody noticed

`grep -rn 'heap_compile_check\|cpp_heap_compile_check' just/ scripts/ .github/`
returns NOTHING. The tests are `#[ignore]`d, so the only way to reach them is to
ask for them by name, and no recipe, gate or workflow does. This is issue 0652 /
0612 / 0667's class — a target no lane enables reads as coverage — one level
over: not `required-features` but `#[ignore]`, which `check-required-features-reachable`
does not model.

## What a fix has to decide

* the two causes above, each on its own merits;
* and whether an `#[ignore]`d test that no recipe names is a gateable shape.
  `check-required-features-reachable` answers the `required-features` half of
  this question; the `#[ignore]` half has no answer, and six tests sat behind it.

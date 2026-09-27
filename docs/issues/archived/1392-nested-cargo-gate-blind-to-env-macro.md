---
id: 1392
title: "`check-nested-cargo-lock-discipline` matches `env::var(\"CARGO\")` and not
  `env!(\"CARGO\")`, so the four nested cargos that actually exist are invisible
  — and one of them rewrote the root `Cargo.lock`"
status: resolved
type: bug
area: ci, tooling, build
severity: medium
found: 2026-09-20
resolved: 2026-09-27
related: [issue-0359, issue-0378, issue-0196, issue-1307, issue-1507, phase-454]
---

## The symptom that started it

During phase-454, a `check::build` lane **rewrote the root `Cargo.lock`**
(`heapless 0.8.0` → `heapless`) once the `cyclonedds` submodule was provisioned.
The agent reverted it and did not commit it. `Cargo.lock` is a promise that
someone else's build resolves what yours did (issues 0359/0378), and the
`--locked` PATH shim exists so a mismatch fails instead of silently rewriting.

Something escaped the shim.

## The escape route

`scripts/bin/cargo` injects `--locked` by being first on `PATH`. **Cargo sets
`CARGO` to the REAL binary for anything it spawns**, so a test that runs
`Command::new(env!("CARGO"))` bypasses `PATH` entirely — the shim never sees it.

That class is known and gated: `scripts/check/check-nested-cargo-lock-discipline.py`,
whose docstring says a nested cargo bypassing the shim must declare what it does
to the lockfile.

**The gate does not catch it.** Its detector is:

```python
BYPASS_SOURCE = re.compile(r"""env::var(?:_os)?\(\s*"CARGO"\s*\)|\bvar(?:_os)?\(\s*"CARGO"\s*\)""")
```

That matches the **runtime** spelling `env::var("CARGO")`. It does not match the
**compile-time macro** `env!("CARGO")` — which is the idiomatic spelling in a
test, and therefore the spelling the sites that exist actually use.

A site that fails `bypasses` is `continue`d **before `sites += 1`**, so it is not
merely unchecked: it is not even counted in the total the gate reports.

## Measured

| spelling | sites | gate sees it? |
| --- | --- | --- |
| `env::var("CARGO")` | 2 | yes |
| `env!("CARGO")` | **4** | **no** |

The gate reports `OK (6 shim-bypassing cargo invocation(s) … each is
non-resolving or carries lock discipline)` — those 6 are the visible ones. The
four blind sites, **none of which carries any of
`--locked` / `--frozen` / `--lockfile-path` / `resolver.lockfile-path` /
`apply_nested_lock_discipline`**:

* `packages/cli/rosidl-codegen/tests/cpp_heap_compile_check.rs`
* `packages/cli/rosidl-codegen/tests/heap_compile_check.rs`
* `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/bare_metal_link.rs`
* `packages/tooling/nros-sizes-build/tests/bitcode_probe.rs`

The cyclonedds one matches the observed symptom exactly —
`bare_metal_link.rs:92`:

```rust
let out = Command::new(env!("CARGO"))
    .current_dir(&root)
    .args(["build", "-p", "nros-rmw-cyclonedds", "--no-default-features", "--target", TARGET])
```

`cargo build` from the workspace root, resolving, with no lock discipline. It is
`#[ignore]`d as heavy, which is why it surfaces only in a lane that runs ignored
tests — and why the rewrite looked like it came from nowhere.

## A second, narrower hole in the same detector

```python
ARG_LITERAL = re.compile(r"""\.arg\(\s*"([^"]+)"\s*\)""")
```

Only the single-argument `.arg("…")` form. Every site above uses `.args([…])`,
so even a site that clears `bypasses` yields an empty argument set and is
described as `['(unknown)']` rather than by its real subcommand. That does not
hide a finding on its own — an empty `subs` still fails the
`NON_RESOLVING`/`metadata --no-deps` exits and reaches the check — but it makes
the diagnosis wrong at exactly the moment someone needs it right.

## Why this is the issue-0196 shape

The gate's rule is correct and its reasoning is written out: a nested cargo that
bypasses the shim must say what it does to the lockfile. Its **reach** is
narrower than that rule by one spelling, and the missing spelling is the common
one. It has been reporting a green over a set that excludes every site anyone
would actually write.

Same family as `check-c-array-pool-floors` requiring adjacent `#ifndef`/`#define`
lines, and `check-knob-ends` crediting a knob with a reader that was its own name
inside an error string.

## What a fix has to decide

* **Widen `BYPASS_SOURCE`** to the macro spelling. Both `env!("CARGO")` and
  `option_env!("CARGO")` resolve to the real binary; a regex over the three
  forms is the minimum.
* **Widen `ARG_LITERAL`** to `.args([…])`, so the reported reason names the real
  subcommand.
* **Then rule the four sites**, which is the substantive half: each either gains
  `--locked`, or is redirected with
  `--config resolver.lockfile-path="<probe dir>/Cargo.lock"` where it injects a
  `[patch]` the workspace lock cannot record, or is argued non-resolving. The
  gate's own `FIX` text already states these three options.
* **A negative control**: the gate must be shown to go red on an
  `env!("CARGO")` site with no discipline, or this recurs under a third
  spelling. A gate that cannot fail proves nothing.

## Reproduction

`python3 scripts/check/check-nested-cargo-lock-discipline.py` exits 0 today.
Add `--locked` to none of the four files and it still exits 0. Change one of
them from `env!("CARGO")` to `env::var("CARGO").unwrap()` and the same site is
reported.

Found while tracing a root-lock rewrite observed during phase-454.

---

## Resolution (2026-09-27)

### THREE holes, not two — and the third is the load-bearing one

The issue named `BYPASS_SOURCE` and `ARG_LITERAL`. Measured against the code,
widening `BYPASS_SOURCE` alone would have changed NOTHING, because `names_cargo`
runs FIRST and rejected the body before the bypass test was reached:

```python
>>> body = 'Command::new(env!("CARGO")).args(["build"])'
>>> names_cargo(body),  BYPASS_SOURCE.search(body),  ARG_LITERAL.findall(body)
(False, None, [])
```

`COMMAND_NEW` reads `Command::new(<ident>)`, and in `Command::new(env!("CARGO"))`
the program is a macro expansion with no identifier to read. So `COMMAND_NEW_MACRO`
had to be added beside the other two widenings. This is the detail a fix driven by
the issue's own list would have got wrong — the regexes would have been correct and
the gate would still have reported OK.

The three widenings:

* `CARGO_ENV_MACRO` = `(?:option_env|env)!\(\s*"CARGO"\s*\)`, folded into
  `BYPASS_SOURCE`. `CARGO` exactly — `env!("CARGO_BIN_EXE_*")` is a test binary
  and `env!("CARGO_MANIFEST_DIR")` is a directory, and both appear in the same
  files, so a `CARGO_`-prefix match would report sites that cannot resolve
  anything.
* `COMMAND_NEW_MACRO`, consulted by `names_cargo` before the ident scan.
* `ARGS_LIST` + `literal_args()`, covering `.args([…])` / `.args(&[…])` /
  `.args(vec![…])`. The verdicts now read `['build']`, `['check']`, `['run']`
  instead of `['(unknown)']`.

Before: `OK (6 shim-bypassing … invocation(s))`, and none of the real sites in
`--list`. After the widening alone: `FAIL`, naming 6 functions across the 4 files
(the issue's table says 4 sites; 4 is the number of FILES — they hold 6 functions).
After the rulings: `OK (12 …)`, every row accounted.

### The rulings, one per site, each measured

**`--locked` — the two that resolve the TRACKED root lock.**

* `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/bare_metal_link.rs:88`
  (`cargo build -p nros-rmw-cyclonedds --no-default-features --target
  thumbv7m-none-eabi`). `current_dir` is the workspace root and the target dir is
  the default, so the file it may rewrite is the committed one — this is the site
  whose symptom opened the issue. It injects no `[patch]`, which is what separates
  it from issue 1307: there is nothing the root lock cannot record, so `--locked`
  is the whole fix and turns a resolution change into a loud error. Measured: the
  build succeeds under `--locked`, and the root lock's md5 is unchanged before and
  after.
* `packages/tooling/nros-sizes-build/tests/bitcode_probe.rs:19`
  (`cargo build --release -p nros --features rmw-cffi,ffi-size-markers`).
  `CARGO_TARGET_DIR` redirects the TARGET dir, which is not the lockfile;
  `current_dir` is the repo. Same reasoning, same verdict. Measured: the ignored
  test passes under `--locked` (8.69 s) with the root lock unchanged.

**`resolver.lockfile-path` — the four that resolve a crate they GENERATED.**

* `packages/cli/rosidl-codegen/tests/heap_compile_check.rs:33` and `:119`
* `packages/cli/rosidl-codegen/tests/cpp_heap_compile_check.rs:28` and `:163`

Each writes a temp crate — manifest carrying `[workspace]`, `current_dir` inside
the tempdir — and runs `cargo check`/`cargo run` in it. `--locked` is NOT available
here and the reason is measured (cargo 1.98.1, on a replica of the crate
`generated_heap_message_compiles` writes):

```text
error: cannot create the lock file …/Cargo.lock because --locked was passed to
       prevent this
```

There is no lock to satisfy. What is true instead, also measured: a `cargo check`
of that exact shape leaves the repo's root `Cargo.lock` byte for byte identical,
because the tempdir is its own workspace root. So the honest discipline is the
gate's third option — the same `resolver.lockfile-path` key issue 1307's size probe
uses, for the adjacent reason: the file the resolution writes is a build artifact,
not the committed promise.

It points at a SUBDIRECTORY (`nros-throwaway-lock/Cargo.lock`) rather than at the
crate root where the lock would have landed anyway, so the redirect is an enforced
statement and not a no-op — measured: afterwards the generated crate root holds no
`Cargo.lock` at all. The path has ONE derivation,
`packages/cli/rosidl-codegen/tests/common/mod.rs::throwaway_lock_path`, which
carries the whole rationale; the call sites spell the key itself, because a gate
that credited a helper by NAME would be the shape this issue is about.

Not ruled non-resolving: all four DO resolve. Not ruled a new fourth category
either — "resolves a generated throwaway crate" would have needed a marker in the
body regardless, and `resolver.lockfile-path` is a marker that also enforces.

### The negative control, and its mutation test

`SELF_TEST_MACRO` carries an offender in each macro spelling, a DISCIPLINED macro
site that must stay green, and a `CARGO_`-prefixed variable that is not a cargo.
`self_test()` asserts 3 sites / 2 offenders, that the offenders are the right two
functions, that the reasons name `['build']` and `['check']` rather than
`['(unknown)']`, and — directly on the regex, since `names_cargo` bails before
`offenders` can reach it — that all four real `CARGO` spellings match and the three
`CARGO_`-prefixed ones do not. It runs on the NORMAL path (`main()` calls
`self_test()` first), as `check-gate-selftests` requires.

Mutation-tested, each widening reverted in isolation, all four CAUGHT with a
diagnostic naming the right thing:

| mutation | self-test verdict |
| --- | --- |
| `BYPASS_SOURCE` loses the macro alternative | FAILED — saw 0 sites |
| `COMMAND_NEW_MACRO` cannot match | FAILED — saw 1 site |
| `ARGS_LIST` cannot match | FAILED — reason was `['(unknown)']` |
| `CARGO_ENV_MACRO` widened to `CARGO[A-Z_]*` | FAILED — `env!("CARGO_MANIFEST_DIR")` |

The last is 0196's other direction (issue 1452's shape): a reach WIDER than the
rule is also a false report, so it is a case and not a comment.

### Found on the way, filed separately

Running the four tests to confirm the rulings in situ turned up three broken
`#[ignore]`d tests that no recipe, gate or workflow names → **issue 1507**. One of
them, `bare_metal_link.rs::workspace_root()` climbing `.nth(3)` to `packages/`
instead of the repo root, is fixed here, because that file's `--locked` ruling had
to be observable: both of its tests now pass. The other two (a generated heap crate
that needs `alloc` and does not enable it; three C/C++ syntax checks that `-I` a
per-build `target/nros-c-generated` nothing provides) are pre-existing, independent
of this change, and left to 1507.

### What was wrong in the issue

* "`env!("CARGO")` | sites 4" — 4 is the number of FILES. There are 6 such
  functions, and the widened gate names all 6.
* The fix list is one item short: `names_cargo`/`COMMAND_NEW` gates the body before
  `BYPASS_SOURCE` is consulted, so the issue's minimum ("a regex over the three
  forms") would not have moved the verdict.
* "Add `--locked` to none of the four files and it still exits 0" is right, but
  `--locked` is the correct remedy for only two of them; on the other four sites it
  is a hard cargo error.

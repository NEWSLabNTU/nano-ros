---
id: 1411
title: "`build_verb_pipeline`'s entry fallback guard stopped discriminating when `nros-cargo.toml` moved into the entry directory"
status: resolved
type: bug
area: testing, cli, build
severity: high
found: 2026-09-21
related: [1280, 0409, phase-454, phase-445, rfc-0098, rfc-0065]
---

# A fallback guard that stopped discriminating when the thing it tested for changed shape

`a_cargo_image_generates_an_entry_from_the_launch_file` in
`packages/cli/nros-cli-core/tests/build_verb_pipeline.rs` carries a documented
fallback for "the launch resolver is not built": when no entry was generated,
assert the weaker property and return. The fallback is chosen by

```rust
let entry = tmp.path().join("build/posix-zenoh/native_entry");
if !entry.is_dir() {
    ...
    return;
}
let manifest = std::fs::read_to_string(entry.join("Cargo.toml")).unwrap();
```

`entry.is_dir()` is not a test for "the entry package was generated". It was
one when the guard was written (phase-383 W3.b), because at that time the only
thing that ever created `build/<coord>/<entry>/` was the entry generator.

That stopped being true. RFC-0098 D1, landed by phase-445 W4/W5 and carried
into phase-454, puts the image's cargo settings file in the image's own entry
directory:

- `packages/cli/nros-cli-core/src/builder/cargo_config.rs` — "So the file sits
  in the image's own entry directory, `build/<coord>/<entry>/`, beside the
  entry's `Cargo.toml`."
- `packages/cli/nros-cli-core/src/cmd/build.rs:534-552` writes
  `image_dir.join("nros-cargo.toml")` **unconditionally**, after the model
  resolution has already failed and warned.

So the directory now exists on both sides of the question the guard asks. The
fallback is never taken, and the `unwrap()` on the next line panics on a
`Cargo.toml` that was never generated.

The correct predicate was already in this same file, three tests further down.
`a_hand_written_entry_suppresses_generation` (line 514) asserts on
`build/posix-zenoh/native_entry/Cargo.toml`, with the comment that says exactly
why:

> The image's SETTINGS still land in `build/posix-zenoh/native_entry/`
> (RFC-0098 D1 — they belong to the image, whoever wrote its main); what must
> not appear there is a generated PACKAGE.

That test was updated when the file moved. The guard at line 472 was not.

## What was measured

Both runs on pristine `origin/main` at `f0d191c98`, in a linked worktree with
no `packages/cli/nros-launch-resolve/target/release/nros-launch-resolve`.

**Solo** — inheriting the parent checkout's `NROS_REPO_DIR`, which points at a
checkout that *does* have the resolver built. Passes:

```
$ cargo test --manifest-path packages/cli/Cargo.toml -p nros-cli-core \
      --test build_verb_pipeline -- \
      a_cargo_image_generates_an_entry_from_the_launch_file --exact --nocapture
running 1 test
nros build:   resolved → /tmp/.tmpjOs2fd/build/demo_bringup__native/resolved.toml (no count derived; see [provenance].refused)
nros build: wrote the missing selection facade for `native` from the entry just generated, and regenerated the entry against it.
nros build:   entry → /tmp/.tmpjOs2fd/build/posix-zenoh/native_entry
nros build:   settings → /tmp/.tmpjOs2fd/build/posix-zenoh/native_entry/nros-cargo.toml
test a_cargo_image_generates_an_entry_from_the_launch_file ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 31 filtered out; finished in 0.43s
```

**Under the lane's own environment** — `just` re-roots `NROS_REPO_DIR` onto the
worktree it is running in (issue 1280, `just/sdk-env.just:87`), so the product's
`launch_resolver_bin()` looks for the resolver *here*, where it is not built:

```
$ just check cli-tests
...
running 32 tests
....... 7/32
a_cargo_image_generates_an_entry_from_the_launch_file --- FAILED
........................
failures:

---- a_cargo_image_generates_an_entry_from_the_launch_file stdout ----
nros build:   resolved → /tmp/.tmp434SuB/build/demo_bringup__native/resolved.toml (no count derived; see [provenance].refused)
nros build: warning: cannot resolve the model for `native`: cannot resolve the SystemModel: `nros-launch-resolve` not found. ...
nros build:   settings → /tmp/.tmp434SuB/build/posix-zenoh/native_entry/nros-cargo.toml

thread 'a_cargo_image_generates_an_entry_from_the_launch_file' panicked at nros-cli-core/tests/build_verb_pipeline.rs:483:70:
called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "No such file or directory" }

test result: FAILED. 31 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

error: test failed, to rerun pass `-p nros-cli-core --test build_verb_pipeline`
error: recipe `cli-tests` failed with exit code 101
```

The three lines of the warning are the whole story: the resolver was not found,
the builder warned and carried on (which is the documented D13 behaviour the
fallback exists to cover), and the settings file was written into the entry
directory anyway.

## Who pays for it

`check-cli-tests` is one of the five contexts in the required `CI` aggregator on
`pull_request` (CLAUDE.md "Practices"; `.github/workflows/gate.yml:797`), and
`just ci gate` is what CLAUDE.md tells every contributor to run before every
push.

Be exact about where the red lands, because the two are not the same:

- **In CI it is green**, and not by luck — `gate.yml:789` runs
  `just setup-launch-resolve` as its own step immediately before
  `just check cli-tests`, so the resolver is always present there and the test
  takes the happy path.
- **Locally it is red**, for every contributor who runs the lane in a checkout
  that has not built the resolver — which is every fresh clone and every agent
  worktree, since `setup-launch-resolve` needs the `play_launch` submodule
  initialised.

That asymmetry is the reason this sat unnoticed: the lane's verdict in the place
that gates merges does not depend on the guard at all.

## The class

**A fallback guard that stopped discriminating when the thing it tested for
changed shape.** The guard asked a question about a *directory* as a proxy for a
question about an *artifact*. The proxy held for as long as that directory had
exactly one producer. Phase-454/445 gave it a second producer that runs on the
failure path, and the proxy silently inverted: the branch meant for "the
resolver was missing" became unreachable precisely when the resolver was
missing.

Nothing about the failure names the cause. The panic is a bare `NotFound` on an
`unwrap()`, five lines below a guard that reads as though it handles exactly
this.

## The fallback itself was also wrong

Separate from the guard, and worth recording because fixing only the guard would
have preserved it: a silent degraded path is the wrong answer here anyway.

1. The fallback branch asserts nothing the test had not already asserted.
   `assert!(plans[0].handoff.is_some())` runs before the branch; all the branch
   adds is `!src/native_entry.exists()`, i.e. "the builder did not fabricate a
   hand-written entry". The test's actual subject — that the entry is generated
   *from the launch file*, with `talker_pkg` derived from it — is not tested at
   all on that path, and the run reports `ok`.
2. Its own lane already refuses the degraded environment. Five sibling tests in
   this same crate (`contract_qos_override_agreement`,
   `contract_qos_policies_resolve`, `contract_queue_buffer_reaches_the_model`,
   `param_declarations_resolve`, `publisher_depth_resolve`) and
   `plan_pipeline_e2e` assert the resolver exists and fail with
   `run `just setup-launch-resolve``. Measured in this worktree: they fail, and
   they only did not fail in the lane run above because cargo stops at the first
   failing test binary. So the tolerant path was never buying a green lane — it
   was buying a misleading `ok` inside a lane that was going to go red three
   binaries later.
3. `plan_pipeline_e2e.rs:219` already states the rule this test broke:
   "Skips loudly rather than silently passing if the resolver was never built,
   since a missing resolver and a broken plan look identical from the assertion
   below."

## A second site, same shape

`tests/entry_typed_plan.rs` has the CLAUDE.md vacuous-test shape outright. Its
`template_model()` helper prints `[SKIPPED] nros-launch-resolve not built ...`
and returns an empty `PathBuf`; the single test then does

```rust
if model.as_os_str().is_empty() {
    return; // resolver not built — the helper printed the skip reason
}
```

Measured in this worktree:

```
running 1 test
test typed_plan_from_template_emits_constructed_components ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Zero assertions executed, reported as a pass, and the `eprintln!` that explains
it is swallowed by `cargo test` without `--nocapture`. This is exactly
CLAUDE.md's "Bare `eprintln!`+`return` reports PASS — never", in the same lane,
against the same precondition.

## The sweep

Every `is_dir()` / `exists()` in the CLI test targets, read for "does this stand
in for *an artifact was generated*":

```
rg -n 'is_dir\(\)|\.exists\(\)' -g '*.rs' packages/cli/nros-cli-core/tests packages/cli/*/tests
```

Result: one defect (line 472, above). The rest are either assertions *about* a
path's existence — which is the thing being tested, not a proxy for it — or
directory-walk filters. In particular the four other `build/<coord>/<entry>/`
sites in `build_verb_pipeline.rs` (lines 240, 253, 284, 516) and
`a_hand_written_entry_suppresses_generation` all already name the file they mean
(`Cargo.toml` or `nros-cargo.toml`) rather than the directory.

## Resolution

### The guard

`a_cargo_image_generates_an_entry_from_the_launch_file` now tests the artifact:

```rust
let manifest_path = entry.join("Cargo.toml");
assert!(
    manifest_path.is_file(),
    "no entry package was generated at {} — the settings file may be there \
     (it is written either way), but `Cargo.toml` is the artifact this test \
     is about",
    entry.display()
);
```

### The fallback — removed, not repaired

It was not legitimate, for the three reasons measured above, so it is gone and
the precondition is stated up front instead:

```rust
assert!(
    nros_cli_core::orchestration::model_location::launch_resolver_bin().is_some(),
    "nros-launch-resolve not found — run `just setup-launch-resolve`, or point \
     $NROS_LAUNCH_RESOLVE at one. This test cannot answer its question without \
     it, and must say so rather than pass quietly."
);
```

Asked through `launch_resolver_bin()` deliberately: this test does not spawn the
resolver, it calls `plan_builds`, which resolves the binary through that ladder.
A hand-written path here could disagree with the code under test in either
direction — and that is not hypothetical, since the in-tree path and
`$NROS_REPO_DIR` point at different checkouts in a linked worktree, which is how
this issue was found.

### The second site, and one spelling for the precondition

`entry_typed_plan`'s silent skip is replaced by the same hard precondition. That
would have made a seventh hand-written copy of the idiom, so the six that spawn
the resolver now share
`tests/common/mod.rs::pinned_launch_resolver()` — the module that already exists
in this crate for exactly this reason. It keeps the hardcoded in-tree path on
purpose, and says so: these tests mean *the pinned resolver*, as their headers
state, which is a different question from the one `launch_resolver_bin()`
answers. Two predicates, one spelling each.

Converted: `contract_qos_override_agreement`, `contract_qos_policies_resolve`,
`contract_queue_buffer_reaches_the_model`, `param_declarations_resolve`,
`publisher_depth_resolve`, `plan_pipeline_e2e` (no behaviour change — they
already asserted) and `entry_typed_plan` (behaviour change — it used to pass
vacuously).

### Acceptance

- **Mutation, original guard restored, lane environment, no resolver** — the bug
  comes back exactly:
  `panicked at build_verb_pipeline.rs:483:70: called `Result::unwrap()` on an
  `Err` value: Os { code: 2, kind: NotFound, ... }`
- **Mutation, new guard, precondition removed, same environment** — the guard
  discriminates where `is_dir()` did not:
  `panicked at build_verb_pipeline.rs:485:5: no entry package was generated at
  /tmp/.tmpe75nsy/build/posix-zenoh/native_entry — the settings file may be
  there (it is written either way), but `Cargo.toml` is the artifact this test
  is about`
- **Lane environment, no resolver** — the lane still fails, and that is the
  correct verdict for an environment six of its tests already refuse; it now
  names the remedy instead of reporting a bare `NotFound`.
- **Lane environment, resolver present** — `build_verb_pipeline` is
  `32 passed; 0 failed`.
- **Solo** — unchanged, `1 passed`.

### Not changed

`check-cli-tests` cannot be green in a checkout without `nros-launch-resolve`,
and this issue does not change that: six tests in the lane assert it, and CI
builds it as its own step before running the lane. Making the lane tolerant of a
missing resolver would mean reintroducing the degraded path this issue is about,
seven times over.

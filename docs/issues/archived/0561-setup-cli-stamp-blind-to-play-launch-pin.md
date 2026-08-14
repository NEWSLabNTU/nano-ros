---
id: 561
title: "`setup-cli`'s source stamp excludes the play_launch submodule, so a pin bump is unfixable by `setup-cli`"
status: resolved
type: bug
area: build
related: [issue-0409, issue-0363, issue-0196, issue-0419, issue-0560, phase-330]
---

## Symptom

After `git submodule update` moves `packages/cli/third-party/play_launch`, every
`nros sync` fails on the 0409 guard:

```
Error: sync: `…/nros-launch-resolve` was built from play_launch 1792e7d34715
       but this `nros` was built from 0cd95a0030aa.
   …
   Rebuild it so both agree:   just setup-launch-resolve
```

and the suggested remedy does not help, because the resolver is already the NEW
one — it is the CLI that is old. `just setup-cli` then **reports success while
rebuilding nothing**:

```console
$ just setup-cli          # no output at all
$ ls -la packages/cli/target/release/nros
-rwxr-xr-x  … Aug 13 23:21 …        # unchanged; the submodule moved at 23:43
```

`cargo clean -p nros-cli-core` does not clear it either — the skip happens
before cargo is invoked. The lane is stuck: the guard is right, and no
sanctioned command fixes what it is complaining about.

## Cause — the skip condition and the guard measure different things

`setup-cli` skips on a CONTENT stamp (issue 0363, deliberately replacing four
drifting copies of an mtime predicate):

```sh
if [ -x "$bin" ] && "$bin" source-stamp >/dev/null 2>&1; then
    exit 0
fi
```

`source_stamp()` hashes tracked CLI inputs, filtered by `is_cli_input()`:

```rust
(rel.ends_with(".rs") || rel.ends_with(".jinja")
 || rel.ends_with("Cargo.toml") || rel.ends_with("Cargo.lock"))
    && !rel.contains("/third-party/")
    && !rel.contains("/testing_workspaces/")
```

`play_launch` lives at `packages/cli/third-party/play_launch`, so it is excluded
by that filter. The stated reason is *"vendored submodules carry their own
graphs; they are not nano-ros build inputs (and would drag in thousands of
files)"*.

**For one fact, that premise is false.** `nros-cli-core/build.rs` consumes the
submodule: it reads its HEAD and bakes the result into the binary as
`NROS_PLAY_LAUNCH_SHA`, which is precisely the value the 0409 guard compares.
So the pin IS a build input, the stamp does not watch it, and the two notions of
freshness disagree in the one case that matters.

The file names the rule it is breaking, two lines above the filter:

> Any input list here that watches less than what the build consumes is the
> issue-0196 shape.

`build.rs` does try to cover this — it emits `rerun-if-changed` for the gitlink
and for `<gitdir>/HEAD`, with a comment describing exactly this scenario. That
is the wrong layer to fix it at and it does not help: `setup-cli` never reaches
cargo, so no `rerun-if-changed` can fire. The stamp wins.

## Reproduce

```sh
git submodule update --init packages/cli/third-party/play_launch   # moves the pin
just setup-cli                                                      # silent, no rebuild
just build-test-fixtures lane=tier2                                 # dies on the 0409 guard
```

Workaround, and the only thing that worked:

```sh
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
```

## Direction

Add the play_launch HEAD sha to `source_stamp()` — the SHA, not the submodule's
file list, so the "thousands of files" objection does not apply and the stamp
stays cheap. It is one `git -C <submodule> rev-parse HEAD`, the same value
`build.rs` already bakes, which also makes the two agree by construction rather
than by coincidence.

Worth checking whether any other `build.rs` in the CLI's dependency closure
bakes something the stamp cannot see; this is the second time the stamp's input
list has been found narrower than the build (phase-330 W1.a was the first, for
local path dependencies), and the fix both times was to widen it to what the
build actually consumes.

Found while running tier 2 for issue 0528's acceptance, on 2026-08-13.


## RESOLVED 2026-08-14

`source_stamp()` now folds in the play_launch pin, and — the part that matters
for not regressing — `build.rs` no longer computes that pin itself. Both go
through one `play_launch_pin(root)` in `source_stamp.rs`, which `build.rs`
already `include!`s. The value baked at build time and the value recomputed at
run time are now ONE expression rather than two that happened to agree until
they didn't.

The SHA, not the submodule's file list, so the "would drag in thousands of
files" reason for excluding `third-party/` from `is_cli_input` stands untouched.

The 0419 gate (a submodule needs a `.git` FILE, or `git -C` walks UP and returns
the SUPERPROJECT's HEAD) moved inside that function, so it can no longer be
remembered at one call site and forgotten at the other.

### Verified

Test `moving_the_play_launch_pin_changes_the_stamp` builds a real nested repo —
a fake `.git` file cannot reproduce the 0419 walk-up — and asserts three things:
an uninitialised submodule reads as unknown rather than as the superproject's
HEAD; initialising it moves the stamp; moving the pin moves the stamp again.

Confirmed it fails without the fix before trusting it:

```
assertion `left != right` failed: initialising the submodule changes what the
next build bakes, so it must change the stamp
  left: "5d10dc1cd1e2b256"
 right: "5d10dc1cd1e2b256"
```

End to end, against this issue's own reproduce steps:

```console
$ nros source-stamp                       # fresh (73ff07515da90611)
$ git -C .../play_launch checkout 0cd95a003
$ nros source-stamp                       # STALE
$ just setup-cli
   Compiling nros-cli-core …              # <- it rebuilds now
$ git -C .../play_launch checkout 1792e7d34 && just setup-cli
$ nros source-stamp                       # fresh (73ff07515da90611), same value
```

The last line is the determinism check: returning the pin returns the stamp, so
this is a function of the tree and not of history.

### On the "worth checking" note above

Not done: whether any other `build.rs` in the CLI's dependency closure bakes
something the stamp cannot see. This fix closes the known instance and makes the
two spellings one, but that sweep is still open — and this is the THIRD time the
input list has been found narrower than the build (phase-330 W1.a for local path
deps, then `.jinja`, now the pin), so the sweep is probably worth someone's time.

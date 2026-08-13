---
id: 546
title: "The px4 compile-check codegens but never syncs, so `nros` resolves against public crates.io — three leaves that have never once type-checked, and nothing says so"
status: open
type: bug
severity: medium
area: build
related: [issue-0378, issue-0463, issue-0457, issue-0102, issue-0136, rfc-0048]
---

## Symptom

Every `just build-test-fixtures` run on this host:

```
error: no matching package named `nros` found
location searched: crates.io index
required by package `px4-offboard-companion v0.1.0 (…/examples/px4/rust/companion/offboard-companion)`
   cargo-check FAILED for px4_offboard_companion (no stamp)
   cargo-check FAILED for px4_stub (no stamp)
```

and the run then reports:

```
fixtures built (check=0 build=0 cmake=1 cxx=0 cargo-check=0 px4=0)
```

`px4=0` — **not one px4 leaf has ever stamped**. The build exits 0 and tier 1
goes green.

## Cause

`scripts/build/compile-check-fixtures.sh` (the `PX4_XRCE_EXAMPLES` block) does
two things per leaf: run `px4_gen` to produce `generated/px4_msgs`, then

```sh
if ( cd "$repo_root/$dir" && cargo check ); then …
```

It never runs `nros sync`. All three leaves name the runtime by REGISTRY name:

```toml
nros              = { version = "*", … }
nros-rmw-xrce-cffi = { version = "*", … }
nros-platform-cffi = { version = "*", … }
```

That spelling is normal for an example leaf here — dozens do it — because
`nros sync` writes the `.cargo/config.toml` whose `[patch.crates-io]` redirects
those names at in-repo paths (RFC-0048 W9). The px4 leaves are the ones nobody
syncs:

```
examples/native/rust/action-client/.cargo/   -> config.toml     (works)
examples/px4/rust/companion/*/.cargo/        -> does not exist  (fails)
git ls-files examples/px4 | grep -c cargo    -> 0
```

No config, tracked or generated. So `version = "*"` resolves the only way it
can — against the public crates.io index — which is exactly the failure
issue 0378 recorded for message crates, one dependency class over.

The codegen half works: `generated/px4_msgs` is present. Only the patch table
is missing.

## Why it is worth fixing rather than tolerating

The block's own comment states its purpose:

> Compile-check only: the runtime needs PX4 SITL + a Micro-XRCE-DDS agent, but
> the generated CDR bindings must at least type-check.

They do not type-check. They have not on any run observed here, and the
mechanism (no `[patch.crates-io]` anywhere in the tree) says they cannot have.
So the guard that exists to prove the px4 bindings still compile is reporting
nothing, and the "coverage gate keeps these as tracked leaves rather than a
silent gap" reasoning in the same comment is defeated: the leaves are tracked,
and the gap is silent anyway.

`px4=0` is printed on every build and reads as "no px4 work to do" rather than
"every px4 leaf failed" — a zero that means the opposite of what it looks like.

## Fix

1. **Run `nros sync` for the leaf before `cargo check`**, the way every other
   registry-naming leaf gets its patch table. That is the smallest change and
   matches the rule already stated in CLAUDE.md ("Rust leaf `.cargo/config.toml`
   is `nros sync`-managed").
2. Then decide what `px4=0` should do. A compile-check whose whole point is
   "these must at least type-check" should not pass the build when zero of them
   did. Either fail, or print the count against an expected one so the number is
   readable — `px4=0/3` cannot be mistaken for success the way `px4=0` can.

`px4_probe` fails identically — verified directly, not inferred:

```
$ cd examples/px4/rust/companion/px4-probe && cargo check
error: no matching package named `nros` found
```

So it is all THREE leaves, not the two the log happened to name. The log stops
at two because the third never reaches its `cargo check` line in that run.

## Reproduce

```sh
source ./activate.sh
cd examples/px4/rust/companion/offboard-companion && cargo check
```

Fails with `no matching package named nros`. The submodule
(`third-party/px4/PX4-Autopilot`) must be present, or the block skips entirely.

## Notes

Not a host-provisioning gap, which is what I first assumed: nothing about this
is specific to this machine. No `.cargo/config.toml` exists for these leaves in
the repository, so any checkout that runs the px4 compile-check hits it. The
earlier reading ("the px4 failures are a provisioning gap") was wrong and is
retracted here.

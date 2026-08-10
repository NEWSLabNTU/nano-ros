---
id: 510
title: "The px4 companion lane skips `nros sync`, so its registry-named nros deps resolve against the public crates.io and lane=all dies"
status: resolved
type: bug
severity: high
area: build, px4
related: [issue-0378, issue-0463, issue-0457, phase-233, phase-336]
resolved_in: "issue-0510 (sync the three companion leaves before building them)"
---

## Symptom

`just build-test-fixtures lane=all` fails the px4 module with rc=2, which takes
the whole lane down — and with it any tier that needs the full fixture
EXISTENCE set:

```
  → cargo build px4-stub (rmw-xrce, target-xrce/)
    Updating crates.io index
error: no matching package named `nros` found
location searched: crates.io index
required by package `px4-stub v0.1.0 (/home/aeon/repos/nano-ros/examples/px4/rust/companion/px4-stub)`
make[1]: *** [.../px4-companion-2796749.mk:8: u0] Error 101
```

All three companion leaves fail the same way: `px4-stub`, `px4-probe`,
`offboard-companion`.

## Cause

`just/px4.just build-fixtures` generated the `px4_msgs` bindings and then went
straight to `cargo build`. Its comment explained why:

> Generate its `px4_msgs` bindings from the PX4 `.msg` tree (no `nros sync` path
> — px4_msgs isn't an ament package) then build the standalone example …

That is true of the CODEGEN half of `nros sync` and false of the other half.
Sync also writes the leaf's `[patch.crates-io]`, and these three manifests
registry-NAME their nros deps:

```toml
nros                = { version = "*", default-features = false, features = [...] }
nros-rmw-xrce-cffi  = { version = "*", default-features = false, features = ["std"] }
nros-platform-cffi  = { version = "*", features = ["posix-c-port"] }
```

With no `.cargo/config.toml` present at all — and there is none in a fresh
clone, since it is gitignored as user-side — a bare version requirement resolves
against the PUBLIC crates.io. This is the #378 class exactly: a registry name in
a leaf manifest with nothing redirecting it.

The error names the crate rather than the missing step, so it reads as "nros
isn't published" instead of "this leaf was never synced".

## Why it stayed hidden

`lane=tier2` runs the px4 module too, and the same three cargo-checks fail
there — but non-fatally, so the lane still exits 0:

```
   cargo-check FAILED for px4_probe (no stamp)
fixtures built (check=0 build=0 cmake=0 cxx=0 cargo-check=0 px4=0)
```

Only `lane=all` promotes it to a hard failure. So the breakage was visible in
the tier-2 build's log for as long as it has existed, reported as a soft
"FAILED … (no stamp)" line among thousands, and nothing gated on it.

## Fix

`build-fixtures` now runs `nros sync <dir>` per companion leaf, right after the
px4_msgs codegen and before the cargo build. Verified: with the config present,

```
$ cd examples/px4/rust/companion/px4-stub
$ cargo build --no-default-features --features rmw-xrce --target-dir target-xrce
    Finished `dev` profile [optimized + debuginfo] target(s) in 13.70s
```

and sync writes exactly the expected gitignored patch:

```toml
include = ["../../../../../../nros-patch.toml"]

[patch.crates-io]
nros-platform-cffi = { path = "../../../../../packages/platform/nros-platform-cffi" }  # nros-managed
nros-rmw-xrce-cffi = { path = "../../../../../packages/rmw/xrce/nros-rmw-xrce-cffi" }  # nros-managed
```

## Left open deliberately

The soft-failure reporting is not addressed here: a `cargo-check FAILED … (no
stamp)` that leaves the lane green is how this hid, and the same shape would
hide the next one. Worth its own issue if it recurs.

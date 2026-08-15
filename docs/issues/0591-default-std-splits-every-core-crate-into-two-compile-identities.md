---
id: 591
title: "`default = [\"std\"]` on the no_std crates splits each of them into two compile identities in ONE cargo invocation"
status: open
type: tech-debt
area: build
related: [issue-0446, issue-0587, phase-334, phase-340, phase-360]
---

## The measurement

One `cargo check --workspace --timings` on a 48-core host, fresh target dir
(the run stops at `cyclonedds-sys` — issue 0390, source not provisioned — so
these are lower bounds):

```
units = 497   cpu = 113 s   wall = 4.1 s
crates compiled under more than one FEATURE SET: 19
redundant cpu from those: ~8 s (7 %)
```

The nano-ros crates in that list, and what actually differs between the two
units:

```
nros-node   v0.5.0   0.59 s  features = [alloc, log, std]
                     0.87 s  features = [alloc, log, std, default, rmw-cffi]
nros-params v0.5.0   0.64 s  features = [alloc, std]
                     0.31 s  features = [alloc, std, default]
nros-core   v0.5.0   0.60 s  features = [alloc, std]
                     0.31 s  features = [alloc, std, default]
```

For `nros-core` and `nros-params` the delta is **the string `default` and
nothing else**. Same code, same effective features, two rustc invocations, two
rlibs, two `-C metadata` hashes.

The same splitter hits third-party crates in the same run: `libc` (`default,std`
vs `[]`), `crossbeam-utils`, `winnow v1.0.3`, `toml_parser`, `memchr`.

## Why it happens

`nros-core` declares `default = ["std"]`. Consumers reach it two ways:

- through `[workspace.dependencies]`, which spells `default-features = false`
  and then names `std` explicitly — feature set `{alloc, std}`;
- through the host/build side of the graph (`nros-macros` →
  `nros-orchestration-ir` → `nros-rmw` → `nros-core`) and through the members
  that leave defaults on — feature set `{alloc, std, default}`.

Cargo's resolver v2 resolves the host and target feature graphs separately, and
`default` is an ordinary feature name for hashing purposes. Two sets that differ
only by an inert name are two units. Cargo is right; the manifest is what makes
them differ.

Dep-sites in `packages/` that leave defaults on today:

```
packages/rmw/zenoh/nros-rmw-zenoh/Cargo.toml:141   nros-platform  (dev-dep, features = ["platform-posix"])
packages/rmw/transport-callbacks/Cargo.toml:12     nros-rmw       (default = [] — harmless)
packages/testing/nros-tests/Cargo.toml:75,76,87    nros-core, nros-serdes, nros-node
packages/testing/nros-tests/bins/*/Cargo.toml      nros-core, nros-serdes, nros-rmw  (11 bins)
```

(The `nros-core` / `nros-serdes` lines under `examples/workspaces/*/src/zephyr_*`
are `[patch.crates-io]` entries, not dep declarations — they carry no features
and are not part of this.)

## Relation to issue 0446

0446 counts `nros-core` at **106 compilations across 5 distinct `-C metadata`
identities** over 60 leaf target dirs. This issue names one of those five and
shows it is reachable inside a single `cargo` invocation, with no leaf-target-dir
involvement at all. Fixing the cache layout (phase-334) and the artifact reuse
(phase-340) collapses the *cross-invocation* copies; it does not collapse this
one, because the two units are genuinely different feature sets as far as cargo
can tell. Both fixes are needed and they are independent.

## Measured outcome of `default = []` — it does NOT merge the units

Recorded here because the prediction below was wrong and the number is the
point of the issue. Phase-341 W3 landed `default = []` on all eight crates plus
every dep-site. Same command, same host, fresh target dir:

```
                      units   crates with >1 feature set   redundant cpu
before                 497              19                    8.0 s
after  (default = [])  497              19                    7.6 s
```

Nothing merged. Two reasons, both worth writing down:

1. **An empty `default` is still a feature NAME.** Declaring `default = []` does
   not remove `default` from a resolved feature set — only omitting the key
   entirely does:

   ```
   default = []        ->  cargo tree -p nros-core --format "{f}"  =>  default
   (no default key)    ->  cargo tree -p nros-core --format "{f}"  =>  (empty)
   ```

   And `--workspace` builds each member as a ROOT, which enables its own
   defaults regardless of any dep-site. So the name persists either way.

2. **The two units were never really "the same code".** They are the
   resolver-v2 host graph and target graph. Before: `[alloc, std]` vs
   `[alloc, std, default]` — which is what made `default` look like the whole
   delta. After: `[]` vs `[alloc, default, std]`. The host-side unit
   (`nros-macros` → `nros-orchestration-ir` → `nros-rmw` → `nros-core`) turns
   out to have been compiling **with `std` it never needed**; W3 took that away.
   That is real work removed, but it shows up as a cheaper unit, not a merged
   one.

So this issue's *build-effort* premise does not survive measurement, and it is
NOT one of issue 0446's five identities in the way claimed above — that
paragraph is retained as the record of a wrong call. What W3 is worth is
elsewhere: the explicitness (below) and issue 0584, which it uncovered.

## Direction

`default = []` on every crate that can build `no_std`, so there is exactly one
spelling of "I want the standard library" — not to merge compile units (it does
not; see above) but so no dep-site can acquire `std` without saying so.

`nros-rmw` already did this — its manifest carries the note *"explicitly
(matches nros-core). Previously `default = ["std"]`"*. The comment says it
matches `nros-core`; `nros-core` still declares `default = ["std"]`. The
convention was decided and then applied to one crate.

Crates to convert: `nros-core`, `nros-serdes`, `nros-params`, `nros-node`,
`nros-platform`, `nros` (umbrella), `nros-c`, `nros-cpp`.

This is a **breaking change for out-of-tree consumers** — `nros-core = "0.5"`
stops being a `std` build. It needs the whole set converted in one commit, every
in-tree dep-site made explicit in the same commit, and a release note. Pair it
with issue 0587 (the `std`/`alloc` implication), which touches the same
manifests: doing them separately means editing every `[features]` block twice.

Gate afterwards: extend `scripts/check-feature-set-ssot.sh`, or add a sibling
`check-feature-contract`, asserting no workspace member declares a non-empty
`default` if it declares `std`.

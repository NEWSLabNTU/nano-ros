---
id: 378
title: "A host set up by the documented flow cannot reach tier-1 ci — and the unsynced leaf crates resolve their message deps against the PUBLIC crates.io"
status: open
type: bug
area: build
related: [rfc-0048, rfc-0067, issue-0363, issue-0368, issue-0373, phase-218, phase-333]
---

# Unsynced leaves resolve against public crates.io; tier-1 ci is unreachable

## Summary

Follow the book's install flow to the letter on a fresh host — `bootstrap.sh`,
`source ./activate.sh`, `nros setup native --rmw zenoh` — then run the tier the
practices file asks for, `just ci`. It fails immediately in `check-fast`:

```
ERROR: 9 leaf crate(s) failed for a reason that is NOT lock drift (see above).
error: recipe `check-leaf-lockfiles` failed on line 702 with exit code 1
```

The gate deliberately does not classify those failures, and the message it
prints does not name a remedy. Running one by hand shows what is actually
wrong:

```
$ cargo metadata --manifest-path packages/testing/nros-bench/stress-zenoh/Cargo.toml
    Updating crates.io index
error: failed to select a version for the requirement `std_msgs = "*"`
  version 4.2.3 is yanked
location searched: crates.io index
required by package `native-rs-zenoh-stress-test v0.4.0 (packages/testing/nros-bench/stress-zenoh)`
```

## Two distinct problems

**P1 — the leaf resolved against the PUBLIC registry.** Per RFC-0048 the leaf's
`.cargo/config.toml` is `nros sync`-managed:

```toml
include = ["../../../../../nros-patch.toml"]
[patch.crates-io]
builtin_interfaces = { path = "generated/builtin_interfaces" }  # nros-managed
std_msgs = { path = "generated/std_msgs" }  # nros-managed
```

On this host **neither redirect target exists**: `generated/` is produced by
`nros generate-rust` (needs a ROS 2 install — absent, and unavailable on this
distro, issue 0373 F3) and the root `nros-patch.toml` is produced by `nros sync`
(gitignored, `.gitignore:93`). With the patch table pointing at absent paths,
cargo went to crates.io and matched a real, unrelated `std_msgs` crate there.
It failed only because that version happens to be **yanked**.

That is the part worth treating as a bug rather than a papercut: nano-ros
publishes nothing to crates.io (installation.md "Rust-only consumers"), yet its
leaf manifests carry bare registry names whose resolution, absent the patch
redirect, silently targets whatever a third party has published under
`std_msgs` / `builtin_interfaces` / `example_interfaces`. The intended
protection is a generated, gitignored file — the weakest link in the chain, and
exactly the one a fresh checkout lacks. A yank is not a security control.

**P2 — the documented setup flow cannot reach tier 1.** CLAUDE.md asks every
change to run at least `just ci`, and `just ci` needs generated message crates
plus a synced patch table that only the CONTRIBUTOR path (`bootstrap.sh base`,
`just setup all`) produces. Nothing in the book, in `just doctor`, or in the
gate's own failure text says "run `nros sync` first". A first-time contributor
on a clean host reads `version 4.2.3 is yanked` and has no path from that
message to the actual cause.

## Root cause CORRECTED (2026-08-01, investigation)

The two problems are real, but P1's mechanism is not the one above, and the
difference changes which fixes work.

**An unsynced tree does NOT reach crates.io — it fails CLOSED.** Verified by
hiding the redirect targets on a synced host:

- redirect present, `generated/` absent →
  `error: failed to load source for dependency 'builtin_interfaces'`
- `include` of a missing `nros-patch.toml` →
  `error: failed to load config include ... failed to read configuration file`

Cargo aborts in both cases. It never consults the registry. So the nine leaves
in the reporter's run are a SETUP gap (P2), not a supply-chain exposure.

**The crates.io resolution comes from cargo's config discovery, and reproduces
on a FULLY SYNCED tree.** Cargo reads `.cargo/config.toml` from the CURRENT
DIRECTORY upward, not from the manifest's directory. The repro in this issue
runs from the repo root:

```
$ cargo metadata --manifest-path packages/testing/nros-bench/stress-zenoh/Cargo.toml
error: failed to select a version for the requirement `std_msgs = "*"`
  version 4.2.3 is yanked
location searched: crates.io index
```

That is this checkout, today, with `generated/std_msgs` and `nros-patch.toml`
both present. The leaf's `[patch.crates-io]` is simply never loaded, because
cwd is the repo root. `--manifest-path` from anywhere outside the leaf does it.

**The exposure is real.** Those names are taken on crates.io by third parties:

```
std_msgs = "0.0.0"            # "std_msgs ros2 rust generated dependencies"
builtin_interfaces = "0.0.0"  # "Ros2 builtin_interfaces"
```

nano-ros publishes nothing there. The resolution fails today only because the
published 4.2.3 is YANKED. A yank is not a security control — publish a
matching version and the same command resolves against foreign code.

**Why no repo-side config can close it.** A `[patch]` entry maps one name to
ONE path, and every leaf redirects to its own per-leaf `generated/` tree, so
the root config cannot hold a redirect that is correct for all sixteen. Closing
it structurally needs either one canonical vendored copy of the message crates
(some leaves already commit theirs — `nros-bench/wake-latency-cortex-m3` tracks
`generated/`) or crate names that do not exist on crates.io. That is an
RFC-0048 / RFC-0023 decision, left here rather than improvised across sixteen
leaves.

## Landed

- `check-leaf-lockfiles` now classifies the unsynced case separately and prints
  the remedy (`nros sync` / `just setup all`) instead of "failed for a reason
  that is NOT lock drift" with no next step. That is what stranded the reporter.
- `check-msg-dep-redirect` (new, in `check-fast`) asserts every registry-named
  message dep has a committed redirect somewhere up its config chain — 110
  today. It stops a NEW leaf from being added unprotected, which is the
  reachable regression.

Still open: the `--manifest-path`-from-elsewhere hole, which needs the layout
decision above.

## Study + design (2026-08-02) — RFC-0067 / phase-333

Investigated the fix space with the maintainer. Rejected: enumerated stub crates
(not SSoT — only `package.xml` is) and an `nros-`prefix (still a squattable
crates.io name). The maintainer surfaced the deeper coupling: the message crate's
committed identity has TWO env-varying axes — the **ament version** (→ committed
lock drifts across distros under `--locked`; observed pinning 4.9.1 / 4.9.0 /
5.3.6) and the **crates.io registry source** (→ this exposure). `0.0.0`-constant
alone makes the exposure worse (`std_msgs = "0.0.0"` is a real squatted crate).

Decision (RFC-0067): make BOTH axes env-invariant from `package.xml` — message
deps become `path` deps (no crates.io name anywhere → fails closed, never the
registry) and the generated crate stays `version = "0.0.0"` (ament → metadata),
so a committed lock is byte-identical across distros. Prototype-validated on
`int32-sink`: builds, unifies to one copy, lock pins `std_msgs 0.0.0` (no
registry source), and from the repo ROOT resolves to `path+file://…` not
crates.io — closing the `--manifest-path`-from-elsewhere hole this issue declared
unclosable. **This refutes the "no repo-side config can fix that one" claim
below.** Implementation: phase-333 (W1/W2 close the exposure with no ROS env; W3
regen → `0.0.0` needs a ROS host).

## Direction

1. **Make the redirect not depend on a generated file for its safety.** Options,
   in rough order of strength: vendor the message crates the in-tree leaves need
   (issue 0368 F4 already proposes completing `packages/cli/interfaces/` for the
   same reason); or give the msg crates names that do not exist on crates.io; or
   set `[registry] default = "…"`/an offline-by-default profile for leaves so an
   accidental registry fetch fails closed instead of resolving to a stranger's
   crate.
2. **Teach `check-leaf-lockfiles` to classify the "unsynced tree" case** and
   print `run: nros sync` — it already separates lock drift from everything
   else, so the branch exists.
3. **Say it in the book**: the contributor section should state that gates and
   tests need `nros sync` + generated interfaces, not only the SDK store.
4. Consider a `just doctor` probe for "leaf patch targets exist".

## Evidence trail

Arch Linux, x86_64, 2026-08-01, checkout at `3de28c939`, host provisioned by
`bootstrap.sh` + `nros setup native --rmw zenoh` only. Nine leaves fail:
`nros-bench/{executor-fairness,large-msg-baremetal,large-msg-xrce,stress-xrce,stress-zenoh,wcet-cycles-qemu}`,
`nros-tests/bins/cdr-roundtrip-qemu`, and two more listed in the run log.

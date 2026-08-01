---
id: 378
title: "A host set up by the documented flow cannot reach tier-1 ci — and the unsynced leaf crates resolve their message deps against the PUBLIC crates.io"
status: open
type: bug
area: build
related: [rfc-0048, issue-0363, issue-0368, issue-0373, phase-218]
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

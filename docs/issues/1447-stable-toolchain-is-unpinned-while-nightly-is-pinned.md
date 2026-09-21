---
id: 1447
title: "`rust-toolchain.toml` pins the LINTING toolchain to a moving channel
  (`stable`) while the FORMATTING one is pinned to a date — so a lint set can
  change under a checkout with no commit, on the one lane that runs clippy and
  gates no merge"
status: open
type: bug
area: [ci, build]
severity: medium
related: [1445, 1380, 0319, 1040]
found: 2026-09-22
---

## The asymmetry, measured

```
$ grep -A1 '^\[toolchain\]' rust-toolchain.toml
[toolchain]
channel = "stable"

$ awk '/^channel/ {print}' tools/rust-toolchain.toml
channel = "nightly-2026-04-11"
```

The NIGHTLY is pinned to a date. The STABLE is a floating channel. Live in the
`ros2` box today:

```
clippy 0.1.98 (48a229ceae 2026-09-01)
rustc 1.98.1 (48a229cea 2026-09-01)
```

The nightly is pinned for a stated reason CLAUDE.md gives: "`rustfmt.toml`
enables nightly-only options; stable produces different output". That reason —
*a toolchain change silently changes this tool's verdict* — is at least as true
of clippy, whose lint SET grows every release, and clippy is the one left
floating.

## What it costs, and why nobody sees it

CLAUDE.md already names the consequence: "`check` runs clippy with
`-D warnings`, so a toolchain bump can surface NEW pre-existing lints". What it
does not say is that the bump needs no commit, so there is no diff to review,
no blame to read and no moment at which anyone decided to take it.

That would be survivable if the lane were merge-gating. It is not. Per
`gate.yml`'s own comments the compile gates `check-c` / `check-cpp` run on NO
merge-gating event — only their clang-format halves do — which is issue 1445.
So the sequence is:

1. stable moves; clippy gains a lint;
2. nothing on any pull request or merge-group event runs `nros-cpp` clippy;
3. the red lands on `main` and stays;
4. it is found by whoever next runs a full local `just ci gate`, as a red in
   `check::build` that withdraws every step behind it.

That is exactly how the two lints fixed in `45d6c1790` arrived —
`doc_lazy_continuation` on `publisher.rs` and `manual_is_multiple_of` on
`lib.rs`. Neither was new code. `git show 7904ac503:packages/api/nros-cpp/src/
publisher.rs` has the offending line wrapping unchanged, so the SOURCE stood
still and the verdict moved.

## What this issue is NOT claiming

`45d6c1790` fixed both sites; this is not a request to fix them again, and
`just check cpp` is green on `main` as of that commit.

It is also NOT claiming a caching bug, which is where I first went. Cargo's
fingerprint includes the compiler version, so a toolchain change DOES
invalidate and re-lint; a stale cached verdict is not the mechanism. I observed
the same checkout answer `All C++ checks passed!` at 14:00 on 2026-09-21 and
fail on those two lints at 02:00 on 2026-09-22 with no source change to either
site, and I **cannot explain that transition** — the two runs straddle a rebase
and nothing else I can identify. It is recorded here as an unexplained
observation, not as evidence for a mechanism. Issues 0859-0862 are this repo's
receipts for what a confident wrong root cause costs, and one of them would be
cheaper to avoid than to retract.

## The open question, which is the point

**How many crates are currently carrying a clippy verdict from a toolchain
nobody chose?** The tree has ~73 crates in the unsafe census alone, and the
lanes that lint them are uneven — issue 1380 records `nros-rmw-zenoh` red under
`platform-bare-metal` because no lane runs clippy on that feature combination
at all; issue 0379 records `packages/cli` never being clippied. Two of those
have been found by accident this month.

Guessing at the number here would be worse than naming it, so it is named. The
sweep is the work, not the two sites.

## Fix shape

Pin the stable channel to a version the way the nightly is pinned, so a
toolchain move is a reviewable commit that runs the lanes, rather than
something that happens to a checkout. Then the ratchet question above becomes
answerable once, at the bump, instead of discovered one crate at a time.

If the channel must float, the alternative is to make a clippy lane
merge-gating so a new lint cannot reach `main` — but that is issue 1445's
decision, not this one's, and the two should not be conflated: 1445 is about
which lanes gate, this is about a toolchain that changes with no diff.

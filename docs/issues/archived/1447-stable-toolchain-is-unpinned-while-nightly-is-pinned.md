---
id: 1447
title: "`rust-toolchain.toml` pins the LINTING toolchain to a moving channel
  (`stable`) while the FORMATTING one is pinned to a date — so a lint set can
  change under a checkout with no commit, on the one lane that runs clippy and
  gates no merge"
status: resolved
type: bug
area: [ci, build]
severity: medium
related: [1380, 0319, 1040, 1642]
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
merge-gating event — only their clang-format halves do. (This sentence used to
end "which is issue 1445"; that citation was wrong — see the correction at the
end.) So the sequence is:

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
merge-gating so a new lint cannot reach `main` — but that is a separate
decision, not this one's, and the two should not be conflated: one is about
which lanes gate, this is about a toolchain that changes with no diff. (This
paragraph used to name issue 1445 as that decision; see the correction at the
end.)

## 2026-10-01 — it happened, and `main` went red for everyone

Rust **1.99.0** released, CI picked it up, and `main` failed on lints nobody
introduced. PRs **#1480** and **#1481** had merged green days earlier; **#1516**
was the first run on 1.99 and failed, then **#1523** failed the same way —
two unrelated branches, two authors, one wall. Every failing site was already
on `origin/main`, so no diff and no blame pointed at it. This is the sequence
this issue described, with the one detail it could not supply: there was no
commit anywhere in it.

Failing gates: `rustdoc-links`, `rustdoc-workspace`, `test-targets`,
`workspace-embedded`, `workspace-all`.

### The A/B this issue could not produce on 2026-09-21

One checkout, one tree, the only variable the toolchain:

```
RUSTUP_TOOLCHAIN=1.98.1 just check cli-clippy   -> "CLI clippy passed!"
RUSTUP_TOOLCHAIN=1.99.0 just check cli-clippy   -> 4 errors
```

The earlier unexplained transition is still unexplained and is NOT claimed to
be this. What is now measured is the mechanism in isolation, which is what the
September observation lacked.

### The sweep question, partially answered

Nine sites, three crates, **three different lints** — and only one of them is
clippy, which is the part this issue got too narrow:

| lint | tool | sites |
| --- | --- | --- |
| `clippy::needless_borrows_for_generic_args` | clippy | 2 — `nros-node` `executor/spin.rs:9112,9153` |
| `rustdoc::redundant_explicit_links` | **rustdoc** | 5 of 7 occurrences — `nros` `lib.rs:166`, `node_runtime.rs:4,6,7,9` |
| `semicolon_in_expressions_from_non_local_macros` | **rustc** | 4 — `cargo-nano-ros` `scaffold.rs:157`, `workspace_scaffold.rs:237,268,276` (+3 more in `nros-pkg-index`, capped to warnings there) |

So the framing "clippy's lint SET grows every release" undersells it: **three
tools move with `channel = "stable"`** — clippy, rustdoc, and rustc's own
future-incompat set, which `-D warnings` turns into a hard error the day the
compiler starts emitting it. A pin covers all three; a merge-gating clippy lane
covers one.

The last row is also the one a lane could never have caught in advance, and
not for a lane reason: the defect is in **eyre 0.6.12's `bail!`**, which expands
to `return Err(..);` — a trailing semicolon in expression position. Our source
is unremarkable; upstream fixed the macro in **0.6.14**. A dependency we had
already resolved became non-compiling because the compiler's opinion of it
changed.

What remains unmeasured is unchanged: the crates no lane lints at all
(issue 1380's `platform-bare-metal` combination, and whatever else), which this
sweep could not reach because a green lane is the only instrument we have.

### Fixed in

`#1447`'s two commits on the `work/1447-rust-199-lints` branch. Verified under
`RUSTUP_TOOLCHAIN=1.99.0`: `rustdoc-links`, `rustdoc-workspace`, `test-targets`
(incl. `cli-clippy`), `workspace-embedded`, `workspace-all` and
`workspace-features` all green.

**The pin itself is deliberately NOT part of that change** — the fix shape above
is a decision for the maintainer, and clearing a red is not the moment to take
it silently.

## RESOLVED 2026-10-02 — pinned to `1.99.0` (PR #1556)

`rust-toolchain.toml` now reads `channel = "1.99.0"`, with the same comment
discipline `tools/rust-toolchain.toml` already had. The version is the one
PR #1536 had already cleared, so the pin changed no diagnostics on the day it
landed; what it changed is who decides when they change next. A toolchain move
is now a reviewable commit that runs every lane before it lands, instead of
something that reaches every checkout and CI at once with no diff.

`tools/rust-toolchain.toml`'s comment, which asserted "the root
`rust-toolchain.toml` stays on stable", was corrected in the same PR — true
when written, false from that commit on.

### What this issue was right about, and what it undersold

The fix shape below held exactly. The framing did not: it said clippy's lint
SET moves with the channel, and **three tools move, not one**. PR #1536's nine
sites were 2 clippy, 5 rustdoc, and 7 errors from rustc's own future-incompat
set (`semicolon_in_expressions_from_non_local_macros`), which `-D warnings`
turns into a hard error — and those seven were not even our code, but eyre
0.6.12's `bail!`. Anyone weighing a future "let it float again" proposal should
weigh all three, not the one this issue named.

### What the pin does NOT do — each now owned elsewhere

- **It creates a drift risk.** A pin that ages silently is how a six-week bump
  becomes a sixty-site one. The mitigation the post-mortem recommended — an
  advisory "try the next stable" lane that is allowed to be red — is filed as
  **issue 1642**, rather than kept open here as residue this issue never owned.
- **It makes no clippy lane merge-gating.** That is a separate decision, as
  this issue's own fix-shape section said, and **no issue owns it** — this
  bullet used to say "That is issue 1445", which was wrong (see the correction
  below). A pin stops the surprise, not the gap.

## Correction (2026-10-03) — "issue 1445" was never the clippy-gating issue

Four sentences above (and the `related:` list) cited **issue 1445** as "which lanes gate merges" / "the
merge-gating clippy decision". That was false from this issue's original text
onward, and the error was copied into issue 1642 and the `rust-toolchain.toml`
comment without anyone opening 1445. **Issue 1445 is
`l3-link-check-passes-the-linker-script-twice`** — `rust-rtos-link-check`
passing `-Tmps2_an385.ld` twice (resolved and archived 2026-09-22), nothing to
do with clippy. The citations are corrected in place above, in 1642, and in
`rust-toolchain.toml`; this note is here so the edit to an archived record is
visible rather than silent.

**No issue owns "clippy lanes do not gate merges".** Searched every issue
(open and archived) for clippy x merge-gating: the nearest are 1163
(`compile-smoke` checks the wrong feature set — a compile gap, not clippy),
1380 (one `nros-rmw-zenoh` feature combination no lane lints) and 0379 (the CLI
was never clippied — resolved). None is the general question, so the sentences
now cite nothing.

**The premise, measured on `origin/main` d3c4e96fa6 (2026-10-03).** The
blanket reading — "clippy gates no merge" — is FALSE, and was already false when
this issue was filed:

- `check workspace-all` runs on `pull_request` AND `merge_group` in
  `gate.yml`'s `check` job, which feeds the required `CI` context. Since
  `2440745e86` (2026-09-10) its host half is `check test-targets`: `cargo
  clippy --workspace --all-targets -D warnings`, then every crate ALONE, then
  `check cli-clippy`. Its embedded half is `check workspace-embedded`
  (thumbv7em clippy, `-D warnings`).
- `check rustdoc-links` and `check rustdoc-workspace` run on `pull_request`.
- The 2026-10-01 incident above is itself the proof: all five failing gates
  (`rustdoc-links`, `rustdoc-workspace`, `test-targets`, `workspace-embedded`,
  `workspace-all`) are on the required context, which is exactly why they
  blocked #1516 and #1523 rather than sitting unseen on a nightly.

What does NOT gate a merge, and so is what this issue's title is actually
about: **`nros-cpp`'s clippy.** `nros-cpp` is excluded from BOTH workspace
clippies (`scripts/build/embedded-only-members.sh` for the host,
`scripts/build/host-only-members.sh` for the embedded target) and is linted
only by `check cpp`, which is in `check build` — `schedule`/`workflow_dispatch`
only. That is where the two lints fixed in `45d6c1790` sat. The same holds for
the feature-combination clippies in `check workspace-features` (including the
`nros-c` shipped-shape combo), also `check build` only. So
"the one lane that runs clippy and gates no merge" is true of `check cpp`, as
the title says, and wrong if read as all clippy.


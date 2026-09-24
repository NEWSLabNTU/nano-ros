---
id: 1486
title: "`check-platform-name-answered` scanned one of the loader's two search
  roots, so `bare-metal` read as unanswered — and the ratchet meant to retire
  that entry could never fire, because it compared against the same partial view"
status: resolved
type: bug
area: [build, tooling]
found: 2026-09-25
related: [1362, 0196, phase-468]
---

# A gate whose reach was narrower than the rule, and a ratchet built on the gap

`PlatformsTree::default_search_path` searches, in order:

```rust
if let Some(p) = env_path { out.extend(...) }       // $NROS_PLATFORMS_DIR
out.push(repo_root.join("packages/platform"));
out.push(repo_root.join("config"));
```

`check-platform-name-answered.py` read **only the second** —
`PLATFORM_DIR = os.path.join(ROOT, "packages", "platform")`, and
`declared_names()` listed that one directory.

So `config/bare-metal/nros-platform.toml`, which has carried
`names = ["bare-metal"]` since phase-349 W1, was invisible to the gate. The
name resolves perfectly at runtime; the gate said it did not.

## The repair taken at the time made it permanent

`bare-metal` went into `BASELINE_UNANSWERED` — a row recording a defect that
did not exist. Issue 1362's table repeats the claim in prose.

**The second-order effect is the worse one.** `stale_baseline()` exists to
retire a baseline row once the name becomes answered, and it compares the
baseline against `declared` — which came from the same narrow root. So the one
mechanism that could have removed the wrong row was computed from the very
view that made it wrong. A ratchet compared against the same partial picture it
is ratcheting cannot tighten.

Measured both ways after the fix: putting `bare-metal` back into the baseline
now fails with

```
check-platform-name-answered: FAIL — the baseline is STALE.
  'bare-metal' no longer needs baselining; drop it from BASELINE_UNANSWERED.
  A ratchet that does not tighten stops being one.
```

which is the message that could never print before.

## The fix

`declared_names()` walks the loader's search path — `packages/platform` then
`config` — with an earlier root winning a duplicate, matching the loader's
"first root defining a name wins". `bare-metal` leaves the baseline.

`$NROS_PLATFORMS_DIR` is deliberately NOT read. It is a per-invocation
override, and a gate that answered differently depending on the caller's
environment would report a property of the shell rather than of the tree.

## What is left in the baseline, and why it matters to phase-468

**One name: `esp32`.** That is now the entire answer to phase-468 W1's first
box — the complete list of platform names that would become a hard error if a
missing descriptor were fatal. `nros-board-esp32-qemu/nros-board.toml:10`
declares `platform = "esp32"` and no descriptor answers it.

Worth stating because it is easy to misread: this is the ESP32-C3 QEMU
**bare-metal** path, which is live, and NOT the ESP-IDF port that phase-468 W2
removes.

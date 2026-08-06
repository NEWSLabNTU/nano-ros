---
id: 457
title: "Leaf .cargo/config.toml is a hand-kept copy of nros-board.toml's cargo_config — it should be generated and gitignored"
status: open
type: tech-debt
area: build
related: [issue-0440, issue-0444, phase-338, rfc-0048]
---

## The question

Should a leaf `.cargo/config.toml` be tracked at all? The maintainer's instinct
is no — "`.cargo` depends on generated msg types, and the msg pkg version
depends on the user's AMENT env; we don't ship generated msg pkgs or their
descendants."

## What is actually in them (measured 2026-08-06)

That instinct is right about the *direction* but the current reason is not the
binding one. **No tracked leaf config patches a generated msg crate** — checked
all 46. What they carry is board wiring: `[build] target`, `[unstable]
build-std`, `[target.<triple>]` linker + rustflags, `[env]` cross-compiler vars,
and a `[patch.crates-io]` block of board/platform path entries.

And all of that is **already declared**, verbatim, in the board descriptor:

```toml
# packages/boards/nros-board-nuttx-qemu/nros-board.toml
cargo_config = '''
[build]
target = "armv7a-nuttx-eabihf"
[unstable]
build-std = ["std", "panic_abort"]
[target.armv7a-nuttx-eabihf]
...
[env]
...
[patch.crates-io]
...
'''
```

| | count |
|---|---|
| tracked leaf `.cargo/config.toml` under `examples/` | 46 |
| of those, deploy-bound (carry `[package.metadata.nros.entry]`) | **39** |
| board descriptors declaring a `cargo_config` | 5 |

So ~85% of the tracked set is a hand-maintained COPY of a declared SSoT.

## Why this matters — it has already cost twice

`check-board-cargo-config-applied.sh` states the current contract plainly: *"The
leaf file is TRACKED and `nros sync` leaves it alone, so the two are kept in
step by hand."* Both failures this phase were that gap:

* **issue 0440** — the `-entry` collapse kept the NODE package's config and
  dropped the board's static link args; all six NuttX Rust entries failed with
  ~3680 undefined libc references. Fixed by restoring the 24 args *from
  `nros-board.toml`* — i.e. the SSoT could regenerate them all along.
* **issue 0444 / phase-338** — the same collapse buried the entry's config under
  `.cargo/.cargo/` (a `git mv` into an existing directory), and the surviving
  node config lacked the link recipe. Identical symptom class.

Neither is possible if the file does not exist in the tree.

## Proposal

Make `nros sync` RENDER the leaf config from `nros-board.toml`'s `cargo_config`
plus the patch block it already writes, and gitignore the result — the same
treatment `nros-patch.toml` (central, gitignored) and SystemModels ("BUILD
ARTIFACTS — never committed, never referenced by entries") already get.

Consequences:

* `check-cargo-config-tracked` inverts: today it enforces "tracked ⟺
  hand-authored"; it would become "no deploy-bound leaf config is tracked".
* `check-board-cargo-config-applied` (added for 0440) becomes unnecessary — you
  cannot drift from a file you regenerate.
* The copy-out contract still holds: a copied leaf runs `nros sync`, which is
  already required (CLAUDE.md: "moved checkout → re-run `nros sync`").

## What needs deciding first

The **7 non-deploy-bound** leaves. Those have no board descriptor to render
from — native examples with a QEMU `runner`, a TLS feature target dir, and so
on. Either they keep tracked configs (and the gate keeps its current shape for
that minority), or their content moves into a descriptor too. Worth settling
before implementing, because it decides whether the gate inverts or just
narrows.

## Note

This is a design change to how 46 files are managed, so it is filed rather than
done. The measurement is the contribution: the tracked content is not
user-specific and not unreproducible — it is a copy, and the copy is what broke.

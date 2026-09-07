---
id: 1220
title: "5 of 8 `nros-platform.toml` descriptors live under `packages/platform/` with NO `package.xml`, while 4 `config/*/package.xml` announce a platform with no descriptor beside them — `check-provider-announcements` globs only `config/*` and reports OK on both halves"
status: open
area: [cli, tooling, build]
severity: medium
related: [phase-349, phase-420, phase-400, "RFC-0087", "RFC-0064", "RFC-0072"]
---

# The platform family has a descriptor and an announcement, in different directories

RFC-0064 R5 D2 names the defect this issue reports, one family over: *"boards
were found by their own walk with its own rules, which is how a board came to
have a descriptor and no announcement."* The board family was fixed —
`board_descriptor.rs::require_announcement` makes a `nros-board.toml` with no
sibling `kind="board"` announcement a hard `BoardLoadError::Unannounced`. The
platform family reproduces the original shape, and RFC-0087's own Open
Questions section flags it as unresolved: *"Whether `platform` descriptors move
from `config/*/nros-platform.toml` beside their packages. They are the one
family whose descriptor does not sit next to a `package.xml`, which D4's
derivation assumes."*

Measured, this is worse than the RFC states — the descriptors have already
half-moved, and the two halves now sit in different trees.

## Measured

**8 `<nano_ros_provides kind="platform">` tags, in 7 directories, all under
`config/`:**

```
config/generic/package.xml      + nros-platform.toml   ✓ adjacent
config/bare-metal/package.xml   + nros-platform.toml   ✓ adjacent
config/freertos/package.xml     + nros-platform.toml   ✓ adjacent
config/zephyr/package.xml       — NO descriptor
config/posix/package.xml        — NO descriptor
config/threadx/package.xml      — NO descriptor
config/nuttx/package.xml        — NO descriptor
```

`config/posix/` and `config/zephyr/` contain **`package.xml` and nothing else**.

**8 `nros-platform.toml` descriptors, in two trees:**

```
config/bare-metal/nros-platform.toml
config/freertos/nros-platform.toml
config/generic/nros-platform.toml
packages/platform/nros-platform-freertos/nros-platform.toml
packages/platform/nros-platform-nuttx/nros-platform.toml
packages/platform/nros-platform-posix/nros-platform.toml
packages/platform/nros-platform-threadx/nros-platform.toml
packages/platform/nros-platform-zephyr/nros-platform.toml
```

`find packages/platform -name package.xml` returns **zero**. The five real,
content-bearing descriptors announce nothing.

## The gate is blind to both halves, and says so wrongly

`scripts/check-provider-announcements.py:88-94`:

```python
    # phase-349 W1. Platform descriptors live under `config/`, not
    # `packages/platform/` — a fact that cost a wrong "the family does not
    # exist" claim in phase-348 W2 (corrected there).
    "platform": (
        "config/*/nros-platform.toml",
        lambda d: list(d.get("names", [])),
    ),
```

That comment is now false. `PlatformsTree::default_search_path`
(`packages/tooling/nros-platform-config/src/platform_config.rs:1359-1367`)
searches **`packages/platform` FIRST**, `config` second, with the stated reason
*"so a descriptor that has moved beside its crate wins over a stale copy left
behind"* — i.e. `packages/platform` is the intended home and `config` is the
legacy one.

Rule A1 is keyed on the descriptor (*"a package.xml sitting beside a descriptor
announces provisions of that kind"*), so:

* the 5 descriptors under `packages/platform/` are outside the glob → A1 never
  evaluates them → their missing `package.xml` is invisible;
* the 4 `config/` dirs that announce with no descriptor beside them have no
  descriptor to key A1 on → also invisible.

The gate passes clean today:

```
$ python3 scripts/check-provider-announcements.py ; echo $?
provider announcements: OK (23 migrated provider(s) across 4 famil(ies),
56 name(s) announced; named families match their descriptor, ...)
0
```

It covers 3 of the 8 platform descriptors and reports on all four families as
though coverage were uniform.

## A third reader, on a third root

`freertos` is the one name declaring `names = ["freertos", "freertos-lwip"]` in
**both** trees, and the two files hold disjoint sections:

* `packages/platform/nros-platform-freertos/nros-platform.toml` —
  `[capabilities]`, `[build.zenoh]`, four `[arch.*]` tables. No
  `priority_plan`.
* `config/freertos/nros-platform.toml` — `names` and `[priority_plan]` only
  (3 occurrences).

`PlatformsTree::load_search_path` merges with `acc.files.entry(name).or_insert(file)`
(`platform_config.rs:1321-1326`) — **first root wins, later definitions are
skipped silently**. `packages/platform` is first, so no `PlatformsTree`
consumer ever sees `config/freertos`'s `[priority_plan]`.

That turns out to be intentional rather than broken: `PlatformConfigFile`'s
field is documented as *"`[priority_plan]` — ACKNOWLEDGED here, interpreted
elsewhere"* (`platform_config.rs:93-109`), present only so
`deny_unknown_fields` does not reject the file, and the real reader is
`scripts/lib/priority_plan.py:92`, which globs `config/*/nros-platform.toml`
**directly**, bypassing `PlatformsTree`. So one platform's facts are assembled
by two independent readers over two different roots, with no single place that
states the split and nothing checking it holds. A `[priority_plan]` added to a
`packages/platform/*` descriptor would be accepted by every parser and read by
nobody.

## Why it matters

* **The "one road" claim is untestable here.** RFC-0087's whole premise is
  *"the in-tree providers become indistinguishable from a user's … any gap in
  the provider path now breaks our own build."* A user's platform provider is a
  package announcing `kind="platform"` with a sibling `nros-platform.toml`;
  five of our own do not have that shape, so the road they take is not the one
  a user's package would.
* **`provider_scan`'s generic resolution cannot find them.**
  `ProviderPackage::descriptor_path(kind)` is `self.dir.join("nros-{kind}.toml")`
  (`provider_scan.rs:146-148`) — it looks beside the announcing `package.xml`.
  For `posix`, `nuttx`, `threadx` and `zephyr` there is nothing there.
  Selection works today only because it bypasses `provider_scan` entirely:
  `cmd/board_facts.rs::workspace_platform_roots` runs its own walk for
  `nros-platform.toml` presence with its own skip list and depth cap.
* **It is the exact class RFC-0064 R5 D2 fixed for boards**, still open one
  family over, with a gate whose comment asserts the state is otherwise.

## Suggested shape

Either move the five descriptors' announcements (give each
`packages/platform/nros-platform-<x>/` a `package.xml` and retire the
`config/<x>/package.xml` stub), or move the descriptors back beside the
announcements. Whichever direction, the gate's `platform` row must glob **both**
roots, and A1 should gain the announcement→descriptor direction the board
family already enforces (`require_announcement`'s inverse), so an announcement
with no descriptor is refused rather than unexamined.

Note the `[priority_plan]` split has to be settled in the same change: today
`scripts/lib/priority_plan.py`'s `config/*` glob is load-bearing for
`freertos`, so moving that file without moving the script's root silently drops
a priority plan.

## Evidence

* `scripts/check-provider-announcements.py:66-103` — `FAMILIES`, and the
  platform row's stale comment.
* `packages/tooling/nros-platform-config/src/platform_config.rs:1313-1367` —
  `load_search_path` (`or_insert`, first root wins) and `default_search_path`
  (`packages/platform` before `config`).
* `packages/tooling/nros-platform-config/src/platform_config.rs:93-109` —
  `priority_plan` acknowledged, not interpreted.
* `scripts/lib/priority_plan.py:92` — the third reader, on `config/*`.
* `packages/cli/cargo-nano-ros/src/provider_scan.rs:144-148` —
  `descriptor_path` assumes adjacency.
* `packages/cli/nros-cli-core/src/orchestration/board_descriptor.rs:905-950` —
  `require_announcement`, the enforcement the platform family lacks.
* `packages/cli/nros-cli-core/src/cmd/board_facts.rs:520-560` —
  `workspace_platform_roots`, the walk that bypasses `provider_scan`.

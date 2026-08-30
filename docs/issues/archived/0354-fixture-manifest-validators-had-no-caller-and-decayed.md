---
id: 354
title: "`fixtures-manifest.py`'s workspace/compile-check validators had NO caller, and decayed until 74 of 86 rows failed — on checker staleness, not fixture breakage"
status: resolved
type: limitation
severity: medium
area: build, testing
related: [issue-0309, issue-0350, issue-0351, rfc-0048]
---

## Finding (2026-07-31, running the validator by hand after phase-319)

`scripts/build/fixtures-manifest.py validate-workspaces` failed **74 of 86**
rows. Not one was a broken fixture. Every failure was the *checker* holding a
stale model of the tree:

| failing rows | what the checker demanded | why it was wrong |
| --- | --- | --- |
| 47 | a `nano_ros_entry` / `add_executable` / `add_library` call naming the entry | RFC-0048 / phase-287 W3 introduced `nano_ros_add_executable`, which every C/C++/mixed entry now uses. The detector never learned the verb. |
| 16 | `[system].default_launch` + that launch file | phase-296 R4 retired the launch bake — `nros::main!(launch = …)` is a compile error — so this demanded a pointer to a path that no longer exists. Model-path bringups (`config/system_model.yaml`) failed by construction. |
| 11 | an entry `package.xml` | genuinely missing — see below |

So the validator, if anyone had wired it into a gate, would have been *useless
noise*: 86% red on a green tree.

## Cause: it had no caller

`git grep validate-workspaces` over the whole repo returns the script's own
usage string and its argparse dispatch. Nothing else. Same for
`validate-compile-checks`. Both were written, and then never run by any recipe,
lane, or hook.

That is the whole mechanism. A gate nobody runs cannot report its own decay, so
each sweep that changed a verb or retired a path left it further behind — and
the further behind it got, the less affordable turning it on became. The
end state is a gate that exists, looks like coverage in a directory listing, and
proves nothing. Issue 0309's silent-lane class, one layer up: there the signal
existed and nothing watched it; here the *watcher* existed and nothing ran it.

The 47-row half is also literally issue 0350 again — a verb migration swept the
CMakeLists and a consumer that reads them did not follow. #350 was the cmake
module; this is the checker.

## Fix

Three parts, in the order they must land (the validator has to be *correct*
before wiring it in is anything but a broken build):

1. **Detector learns the current verb.** `_cmake_has_entry_target` accepts
   `nano_ros_add_executable`, alongside the older spellings which remain valid.
2. **`default_launch` becomes one of two ways, not the only way.** A bringup
   declares its topology the launch way (`[system].default_launch` + the file)
   or the model way (`config/system_model.yaml`). Either satisfies the rule;
   declaring NEITHER still fails, so the check keeps its teeth.
3. **The 11 missing `package.xml` files were written**, not excused. Direction
   evidence: `zephyr_entry_robot1` gained one on 2026-07-03, after these entries
   were created in June — entries *do* carry a manifest, and these were
   oversights. Weakening the rule to match the omission would have been the
   wrong repair. Each one's `<exec_depend>` list is derived from what the entry
   actually consumes (sibling path-deps in `Cargo.toml`; linked `*_pkg` targets
   in `CMakeLists.txt`), and `<build_type>` follows its workspace's convention
   (`ament_cargo` for Rust, `cmake` for C).

Then: **`just check fixtures-manifest`**, wired into `check-fast`. It is
buildless and source-free — path existence plus regex over tracked files, ~0.1s
for all 112 rows — so it fits the per-push tier that the fast gate exists for.

## Verification

`validate-workspaces` 86/86, `validate-compile-checks` 26/26, both via the new
recipe, which `check-fast` now depends on.

## The sweep

Per the CLAUDE.md "fix the CLASS, then prove the sweep" rule, every script in
`scripts/` was checked for a caller:

```sh
for f in $(git ls-files 'scripts/**/*.py' 'scripts/**/*.sh' 'scripts/*.py' 'scripts/*.sh'); do
  git grep -qF "$(basename $f)" -- justfile just/ .github/ scripts/ ":!$f" || echo "NO CALLER: $f"
done
```

Fifteen hits, and — usefully — the validators were the only *gate* among them.
The rest are legitimately caller-free by kind: one-shot migrations
(`scripts/docs/migrate-*.py`, `scripts/zephyr/migrate-workspace.sh`), operator
tools invoked by hand (`scripts/esp32/launch-esp32c3.sh`,
`scripts/installers/arm-fvp-installer.sh`, `scripts/sdk-env.sh`), and repro /
capture aids (`scripts/ros/domain-bridge-repro.sh`,
`scripts/ros/capture-edition-fixtures.sh`). A script that *asserts an invariant*
and has no caller is the defect; a script a human runs deliberately is not. So
the class is closed at two members, both fixed here.

(The migration scripts having outlived their migrations is a tidiness question,
not this issue's — filed nowhere, noted here.)

## The general shape

Worth a sweep of its own: **a validator with no caller is not coverage.** The
question to ask of any checker in `scripts/` is not "is it correct?" but "what
runs it, and when did that last fail?" — a checker whose answer is "nothing"
has an unbounded staleness debt that grows with every sweep, and it is
discovered at exactly the worst moment, when someone finally tries to switch it
on. Same family as issue 0351's stamps answering presence instead of truth: the
artifact is real, but it is not evidence of what its existence implies.

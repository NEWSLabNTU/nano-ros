---
id: 1515
title: "Four template workspaces carry NEITHER workspace-root marker — the
  resolver has two spellings and these have both missing"
status: resolved
type: bug
area: examples, cli
severity: medium
found: 2026-09-27
resolved: 2026-09-27
related: [rfc-0065, rfc-0098, issue-1453, issue-0196, phase-445, phase-470]
---

## Outcome, first

**The finding was a GATE, and the survey that produced the table below was
STALE.** Three of the four trees already carried `.colcon_workspace`, tracked,
since 2026-09-11 — and the fourth is measurably not a workspace root. Nothing was
broken. What landed is `check-bringup-workspace-root`, the gate that keeps it
that way, plus a documented reason on the one tree that has none.

The issue's own instruction ("do not fix this by adding four files until a build
says what the absence costs") is what saved four wrong files.

## What this said, and what was true

The table below was the filing's central claim. Measured against the same
checkout on 2026-09-27:

| tree | claimed `.colcon_workspace` | ACTUAL | has a bringup |
| --- | --- | --- | --- |
| `c-and-cpp-mixed-workspace` | no | **tracked** | yes |
| `multi-node-workspace-cpp` | no | **tracked** | yes |
| `pure-c-workspace` | no | **tracked** | yes |
| `multi-package-workspace` | no | absent, correctly | no |

All three markers were added by phase-445 W5 ("every workspace entry is
generated, and a workspace with no bringup builds like colcon") — sixteen days
before this issue was filed. `multi-node-workspace-cpp`'s could not have been
missing under any circumstances: `cargo-nano-ros/src/workspace_scaffold.rs`
`include_str!`s that exact path, so its absence is a compile error of the CLI.

Two lessons worth more than the table:

* A static survey that reads "is this file present" must read it with
  `git ls-files` / `stat`, not from a reader's model of the tree. This one was
  written from the shape of a related finding.
* The filing's conclusion was nevertheless RIGHT for the right reason. It refused
  to prescribe four files, and the measurement it demanded is what found the
  staleness.

## What the absence actually costs — measured

`detect_workspace_root` has **four** rungs, not the two this issue named
(`packages/cli/nros-pkg-index/src/lib.rs`): `$NROS_WORKSPACE_ROOT`, then
`.colcon_workspace`, then a `Cargo.toml` with `[workspace]`, then a `.git` entry.
The fourth is why a missing declaration is invisible rather than loud.

### The three trees that have the marker: they resolve

With the CLI and `nros-launch-resolve` built from this checkout, run in each tree:

```
$ nros sync
sync: resolved system.launch.xml → …/pure-c-workspace/build/nros/models/demo_bringup/system_model.yaml
sync: no Rust consumer pkgs — patch tables not written.
sync: source metadata — 2 rebuilt, 0 already current
sync: done.

$ nros build --dry-run
nros build:   resolved → …/pure-c-workspace/build/demo_bringup__native/resolved.toml
nros build: demo_bringup:native -> board native (platform posix), driver cmake
cmake -S build/posix-zenoh-native -B build/posix-zenoh-native/cmake …
cmake --build build/posix-zenoh-native/cmake
```

Identical for `c-and-cpp-mixed-workspace` and `multi-node-workspace-cpp`.

### Removing the marker from a C/C++ tree changes NOTHING

`pure-c-workspace` with `.colcon_workspace` moved aside, `build/` cleared:
`nros ws list`, `nros sync` and `nros build --dry-run` produce **byte-identical**
output. Same for the Rust `multi-node-workspace`. The reason is that neither verb
asks `detect_workspace_root` for the tree: `nros build --workspace` defaults to
the CURRENT DIRECTORY, and the generated entry is handed
`NROS_WORKSPACE_ROOT` through `builder::cargo_config` besides.

So a measurement that stops at `nros build` concludes the marker is decoration.
It is not.

### Where it IS load-bearing: a bare `cargo` on the generated entry

The generated entry carries its own `[workspace]` table (deliberately — RFC-0098
D9, `builder::entry::render_manifest`), so rung 3 stops AT the entry directory.
Rung 2 is the only thing that reaches past it to the workspace. Run in
`examples/templates/multi-node-workspace/build/posix-zenoh/native_entry`, with no
`--config` and so no `NROS_WORKSPACE_ROOT`, forcing a macro re-expansion:

```
# marker PRESENT
    Checking native_entry v0.0.0 (…/build/posix-zenoh/native_entry)
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.06s

# marker ABSENT
error: nros::main!: pkg `demo_bringup` not found in workspace
`…/examples/templates/multi-node-workspace/build/posix-zenoh/native_entry`.
Known pkgs: []
  --> src/main.rs:10:14
   |
10 |     launch = "demo_bringup",
   |              ^^^^^^^^^^^^^^

error[E0601]: `main` function not found in crate `native_entry`
error: could not compile `native_entry` (bin "native_entry") due to 2 previous errors
```

That is the cost, and it lands on the two callers that do not go through
`nros build`: a contributor iterating with `cargo build`/`cargo check` on the
generated entry, and rust-analyzer. It is also what a copied-out tree gets, where
rung 4 has no `.git` to find and `detect_workspace_root` `bail!`s naming four
markers.

### `multi-package-workspace`: not a workspace root, and the marker would be a lie

It has no bringup. Its three packages are three INDEPENDENT single-package
projects sharing one nano-ros checkout: `pkg_c_talker` and `pkg_cpp_listener`
have their own root `CMakeLists.txt`, and `pkg_rust_publisher` has its own
`[workspace]` table and its own `system.toml` (Form-1 self-bringup). Measured,
with no marker:

* `nros ws list` resolves the directory and lists all three;
* `nros build --dry-run` plans all three (two cmake, one cargo);
* `nros sync` succeeds;
* a bare `cargo check` in `src/pkg_rust_publisher` expands `nros::main!` cleanly
  — rung 3 answers with the package itself, which is the correct root;
* **adding** a marker at the tree root changes none of the above;
* `scripts/check-template-copy-out.sh multi-package-workspace` →
  `OK — 3 artifact(s)`, i.e. it builds from a copy of the tracked file set with
  no `.git` anywhere above it.

So the deliverable here is the documented reason, in
`examples/templates/multi-package-workspace/README.md`, not a file. Issue 1453 is
about `pure-c-workspace`, which has its marker.

## Resolution

1. **No marker was added.** Three trees had one; the fourth is not a workspace
   root and now says so in its README, with the measurements above.
2. **Gate `check-bringup-workspace-root`** (`scripts/`, fast lane via
   `just/check/cargo.just`, so it runs on every merge-gating event —
   `check-default-gates-run-somewhere` verified). Predicate: *every tracked
   `system.toml` with no `Cargo.toml`/`CMakeLists.txt` beside it — a BRINGUP by
   `leaf_system::is_package_dir` — has an ancestor that declares a workspace root
   by one of the two TRACKED spellings, strictly below the repository root.*

   Three design points, each a trap that was measured rather than reasoned about:

   * **Reach = the rule, not the four sites** (issue 0196). The subject is every
     tracked `system.toml` in the repository: **33 bringups**, 21 under
     `examples/` and 12 under test fixtures, which pass by the cargo spelling at
     their own fixture root. Scoping to the four paths, or even to `examples/`,
     would have been narrower than the rule.
   * **The walk stops BELOW the repository root**, because the repo's own
     `Cargo.toml` declares `[workspace]` on line 1. A walk that reached it would
     rescue every bringup in the tree and the gate could never fail. It is also
     the wrong answer on its own terms: that table is the nano-ros workspace, and
     it does not travel with a copy-out. `examples/`,
     `examples/workspaces/` and `examples/templates/` track no root files at all,
     so nothing in between can rescue a tree either.
   * **A bringup is identified by SHAPE, never by name.** The predicate is
     `leaf_system::is_package_dir` negated, the same discrimination the resolver
     makes. `realtime-cpp-subnode-portable` calls its bringup `deploy_bringup`
     and `multi_pkg_workspace_zephyr`'s sits at the fixture root with no `src/`,
     so a `*_bringup` convention would have missed both.

   It carries a self-test on the normal path (nine cases, including a repo-root
   `[workspace]` that must rescue nothing, a `Cargo.toml` with no `[workspace]`
   table, and both spellings of `is_package_dir`), and it was shown to fail
   against the REAL tree, not only the synthetic one:

   ```
   $ git rm --cached examples/templates/pure-c-workspace/.colcon_workspace && \
     mv examples/templates/pure-c-workspace/.colcon_workspace /tmp/ && \
     python3 scripts/check-bringup-workspace-root.py
   check-bringup-workspace-root: 1 of 33 bringup(s) sit in a tree that declares NO workspace root:

     examples/templates/pure-c-workspace/src/demo_bringup/system.toml
   ```

Sweep, 0 remaining and the command the next person re-runs:

```
python3 scripts/check-bringup-workspace-root.py
```

## Not verified here

* **A gate cannot answer the rung-4 question.** It checks that a tracked
  declaration exists; it does not check that the declaration names the directory
  a human meant. One observation left deliberately un-asserted:
  `packages/cli/nros-cli-core/tests/fixtures/refused_resolve/src/demo_bringup`
  resolves to `packages/cli` — the CLI sub-workspace root, not the fixture root —
  because that is genuinely what `detect_workspace_root` answers there. Whether a
  fixture should declare its own root is a separate question from whether the
  tree declares one, and asserting the stricter rule would have put a red on
  `main` for a fixture nobody reported a problem with.
* No embedded or QEMU image was built. Every measurement above is a host
  `nros sync` / `nros build --dry-run` / `cargo check`, plus one copy-out build.

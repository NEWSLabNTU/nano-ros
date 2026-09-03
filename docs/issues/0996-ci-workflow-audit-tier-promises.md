---
id: 996
title: "CI audit: the same lane passes in the queue and fails after merge, four of
  six lanes carry no signal, and the provisioning system is bypassed by hand"
status: open
type: bug
area: ci, tooling
related: [0992, 0876, 0883]
---

Audit of all 11 workflows against the four properties they are supposed to have:
a workflow reads as a user session; provisioning goes through `nros setup`; each
tier delivers the promise it advertises; and a missing prerequisite fails rather
than skips.

## 1. The same lane passes in the queue and fails after merge

`just ci matrix build` dispatches to `_matrix-build`, whose entire body is `l3`.
So `queue.yml` (`just ci l3`, on `merge_group`) and `build-wide.yml`
(`just ci matrix build`, on `push` to main) run **the same recipe on the same
runner labels**. Their verdicts, on the same commits, minutes apart:

| commit | queue.yml (merge_group) | build-wide.yml (push main) |
| --- | --- | --- |
| `36c306405` (pr-216) | success 16:25:49 | **failure** 16:26:23 |
| `da0344af1` (pr-219) | success 17:02:18 | **failure** 16:43:26 |
| `ad5a8ecf9` (pr-217) | success 17:06:55 | **failure** 17:22:19 |
| `d2a8955c5` (pr-220) | success 01:23:54 | **failure** 17:27:51 |

Five queue successes, four post-merge failures, one lane. The difference is not
the code — it is that `queue.yml` hand-rolls the codegen step:

```yaml
- name: Build nros CLI (nros sync needs it)
- name: Build nros-launch-resolve (nros sync shells out to it)
- name: nros sync (writes the central `nros-patch.toml` the leaves include)
- name: nros sync the leaves L3 builds        # a for-loop over three leaves
```

while `build-wide.yml` runs `just setup tier2` and trusts the build verb to do
it. It did not — that is issue 0992, and this table is its proof in the wild.
**The boilerplate is load-bearing**: it is exactly what keeps the defect
invisible to the queue while the post-merge lane burns.

Two consequences, and the second is the expensive one:

* The same self-hosted lane is paid for twice per change, once in the queue and
  again after the merge it already gated.
* A gate that has already passed in the queue cannot fail differently after the
  merge unless the two spellings have diverged — so a red `build-wide` reads as
  noise, and stayed red for a day.

Once 0992 lands, those four steps are dead weight and `queue.yml`'s job body
should be the same two lines `build-wide.yml` has. Better still: **decide
whether this lane belongs in the queue or after it, and run it once.**

## 2. Four of six verification lanes carry no signal

| workflow | last 5 conclusions |
| --- | --- |
| `gate.yml` | green (the required `CI`) |
| `post-submit.yml` | success, cancelled, success, success, success |
| `build-wide.yml` | failure ×4 |
| `run-matrix.yml` | failure ×2 |
| `nightly.yml` | failure ×4 |
| `host-tests.yml` | failure ×4, cancelled |

CLAUDE.md already names this class outright: *"A uniformly-red lane has NO
signal capacity: a regression landing in it looks exactly like yesterday's
failure."* That is the state of every lane above the merge gate. The tiers
promise coverage they are not delivering — not because the tier is wrong, but
because nobody can tell a new failure from the standing one.

`host-tests` is the honest case and worth keeping: it dies in **Build workspace
fixtures**, a fixture step that fails loudly instead of laundering the failure
into a skip. That is the policy working. It still needs fixing.

## 3. Provisioning is bypassed by hand, for packages the index already declares

`nros setup --system` resolves the `[prereq.*]` closure for the detected package
manager (RFC-0062 / phase-327). On this host it reports **38 present, 5 missing,
3 unprobed** — the mechanism works. Yet:

| workflow | does | already declared? |
| --- | --- | --- |
| `docs.yml` | `sudo apt-get install -y doxygen graphviz` | **yes** — `[prereq.doxygen]`, `[prereq.graphviz]` |
| `nightly.yml` | `apt-get install -y clang libclang-dev` | **yes** — both in the index |
| `gate.yml` (colcon-parity) | adds the ROS apt repo, installs `ros-humble-ros-base`, `colcon`, then curl-installs `just` | `colcon` **yes**; ROS no |

`[prereq.doxygen]`'s own `why` field reads *"found undeclared by
check-sysdep-remedies"* — so a gate already exists that finds sysdeps missing
from the index. **Nothing checks the reverse**: that a workflow does not
apt-install what the index already knows. That is the gate this audit asks for.

`gate.yml`'s colcon-parity job is the sharpest case. `images.yml` builds
`ghcr.io/newslabntu/nano-ros-ci:humble` — described in its own header as *"ROS 2
Humble + host tools, the base `container:` for the check / …"* — and **only
`nightly.yml` ever uses it**. colcon-parity installs a ROS 2 desktop stack by
apt on every run instead of declaring `container: ci-base`.

## 4. `nros setup` is index-driven, not `package.xml`-driven

The intent is the rosdep experience: deps are written in `package.xml`, the tool
scans the packages and installs accordingly. Today the two halves do not meet.

* **406 tracked `package.xml` files.** Their entire dependency vocabulary is ROS
  message and build-tool keys — `std_msgs` (177), `example_interfaces` (106),
  `ament_cmake` (19), `rosidl_default_*` (27), and workspace-local node
  packages. Not one names a system dependency.
* Those declarations are consumed **only by codegen** — which msg packages to
  generate. No provisioning path reads them.
* Provisioning reads `nros-sdk-index.toml` instead: `[prereq.*]`, `[rust.*]`,
  `[python.*]`, `[source.*]`, `[tool.*]`. Hand-maintained, repo-global.

So a `package.xml` cannot express "this package needs libssl-dev", and there is
no `nros setup <workspace>` that walks a workspace's manifests and resolves
their closure. The nearest thing, `nros setup --build-sources`, provisions a
repo-global union from the index, not a per-workspace scan.

This is the gap between "we have a provisioning system" and "we have rosdep".
Closing it means: a `package.xml` dep key resolves through the index's
`[prereq.*]` table the way a rosdep key resolves through the rosdep DB, and
`nros setup` takes a workspace path.

## 5. Tier promises vs what the critical path actually runs

`just ci gate` promises *"compile + unit; no fixtures were built or needed"* and
runs `check::cli-fresh check::fast check::build check::api-parity test-unit
test-lane-contracts`. What the required `CI` context runs, per event:

| step | pull_request | merge_group | schedule/dispatch |
| --- | --- | --- | --- |
| `check fast` (168 gates) | ✓ | ✓ | ✓ |
| `check submodule-commits-reachable` | ✓ | ✓ | ✓ |
| `check compile-smoke` | ✓ | — | — |
| `check cli-tests` | ✓ | ✓ | — |
| `check build` + `no-std` | — | — | ✓ |
| `generate-bindings`, compile-check fixtures | — | — | ✓ |
| **`test-unit`** | **—** | ✓ | ✓ |

**`test-unit` does not run on `pull_request`.** A PR shows a green required `CI`
having never run a unit test; they run first in the merge queue. Nothing broken
lands (the queue is the real gate, and `queue-notify.yml` exists to comment on
ejections), but CLAUDE.md states the required context *is* `check-fast` +
`test-unit` + `check-cli-tests`, which is true only in the queue. Either the
doc or the trigger list should move.

`check::api-parity` looked absent from every workflow and is not: its members
(`rmw-api-parity`, `rmw-abi-shape`, `api-parity-ledger`) are in the
`fast-serial` registry, so `check fast` runs them on every PR. No gap — recorded
because the grep says otherwise and the next reader will run it.

## 6. One scope vocabulary, five spellings

phase-411 W4 says a CI job is `just setup <scope>` then one command a developer
can type. Three lanes do this; the rest predate it:

| workflow | provisioning | work |
| --- | --- | --- |
| `build-wide` | `just setup tier2` | `just ci matrix build` |
| `run-matrix` | `just setup tier2` | `just ci matrix` |
| `nightly` (matrix) | `just setup tier2-nightly` | `just ci matrix-nightly` |
| `host-tests` | 5 hand-rolled steps (`nros setup --source ×6`, 3× `--tool`, `provision-zenohd`, `setup-launch-resolve`) | `just native build-fixture-rust-core`, `just native build-workspace-fixtures`, `just ci tier1` |
| `queue` | 4 hand-rolled steps (§1) | `just ci l3` |
| `nightly` (platform) | `just <plat> setup` + 6 more steps | `just <plat> test` |

`nightly.yml` repeats a 22-line "Build nros CLI from packages/cli/" block in
four jobs verbatim.

## What the skip policy got right

Worth recording, because it is the part that works and should not be undone:

* `host-tests`' fixture steps `exit 1` on failure. The comment above them
  records the ten runs where a green step sat over a failed build.
* `gate.yml`'s `ci-ok` refuses a vacuous pass: a skipped `check` is accepted
  only for a `pull_request` that `changes` proved touched no code, and any other
  skip is a hard failure.
* `report-interlock-coverage.sh` turns "this self-hosted lane did not run" into
  a stated outcome instead of silence — the fix for the no-verdict class.

## Order to fix

1. Land 0992, then delete `queue.yml`'s four hand-rolled sync steps and decide
   whether l3 runs in the queue or after it — **once**, not both.
2. Get `host-tests` green (workspace fixtures), then `run-matrix`, then
   `nightly`. Until a lane is green once it cannot report anything.
3. Gate: no workflow may `apt-get install` a package the index declares.
   Convert `docs.yml`, `nightly.yml`'s clang step, and colcon-parity (to
   `container: ci-base`).
4. Decide the `package.xml`-vs-index question in an RFC before writing code —
   it changes what a `package.xml` in this repo means.

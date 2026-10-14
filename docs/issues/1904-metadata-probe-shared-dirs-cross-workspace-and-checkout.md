---
id: 1904
title: "The metadata probe's shared build dirs are keyed by configuration, not by workspace or checkout: nine `probe_talker` binaries share one uplifted `probe-talker`, and `NROS_BUILD_ROOT` widens both shared dirs across checkouts"
status: open
type: bug
area: [build, tooling]
severity: medium
found: 2026-10-15
related: [1901, 0522, 0616, 0805, 0987, 1593, 1869]
---

## Where the dirs come from

`nros-cli-core/src/orchestration/metadata_build.rs`, `shared_probe_dir`:

```rust
if let Some(root) = std::env::var_os("NROS_BUILD_ROOT").filter(|v| !v.is_empty()) {
    return Some(PathBuf::from(root).join(kind));
}
let shared = nano_ros.join("build").join(kind);
```

Two callers use it:

- **Rust harness:** `probe_target_dir` → `<root>/metadata-probe`, passed as
  `cargo run --target-dir`. One target dir for every component harness (issue
  0522).
- **C/C++ probe:** `metadata_probe_cmake::configure_project` →
  `-DNROS_SHARED_CARGO_ROOT=<root>/corrosion-cargo/metadata-probe` (phase-489
  W4). `nros_shared_cargo_dir` then appends a hash of the cargo INPUTS
  (features, rmw, board, caps, profile, target, knobs) and symlinks the probe
  build dir's `cargo` to it.

Without `NROS_BUILD_ROOT`, `<root>` is `<checkout>/build`, so sharing stops at
the checkout. With it set, `<root>` is one path for every checkout that sets
it. `scripts/build/build-root.sh` deliberately does not re-root it (issue 1280
notes). Nothing in the repo exports it: `git grep NROS_BUILD_ROOT` finds
readers and comments only, so it is an operator choice.

## Measured (one checkout, `e8400d5a92`, after `just build-test-fixtures lane=linux`)

**1. One uplift slot, nine harness binaries.** The harness bin is
`probe-<slug>`, and the slug is the component id (`probe_slug`). That id is
unique within a workspace, not across workspaces:

```
$ grep -rh '^name = "probe-' examples/workspaces/*/build/nros-metadata/metadata-probe/*/Cargo.toml | sort | uniq -c | sort -rn | head -2
      6 name = "probe-talker"
      2 name = "probe-listener"
$ ls -li build/metadata-probe/x86_64-unknown-linux-gnu/debug/probe-talker
14586607 … Oct 15 01:45 probe-talker
$ ls -li build/metadata-probe/x86_64-unknown-linux-gnu/debug/deps/ | grep -E 'probe_talker-[0-9a-f]+$'
  9 distinct probe_talker-<hash> binaries (01:38 … 01:45); inode 14586607 = probe_talker-53057068817c09a7
```

cargo does not hash the uplifted name, so all nine share
`debug/probe-talker`, which holds whichever harness linked last. This is the
collision issue 0522 renamed the bin to avoid ("two components would write the
same `<target>/<host>/<profile>/probe` and the second `cargo run` could execute
the first one's binary"). The slug fixed it within one workspace. It did not
fix it across workspaces that share the target dir.

`check-action-client-arena-budget` already reports this dir, from the fixture
build log: `build/metadata-probe/x86_64-unknown-linux-gnu/debug: 29 programs
share this profile dir's one generated config (last build script wins)`
(issues 1869/1885 class, not examined there).

**2. Keyed Corrosion dirs span workspaces.** c, mixed, realtime-c, three
`examples/templates/*` and `nros-tests/fixtures/image_config_ws` all symlink
`cargo` to `build/corrosion-cargo/metadata-probe/ddb2119461cd`. The `.key`
text has no workspace or checkout field:

```
features=metadata-mode,platform-posix,rmw-zenoh,ros-humble,std|rmw=zenoh|board=|caps=|profile=nros-relwithdebinfo|target=x86_64-unknown-linux-gnu
```

Issue 1901 measured one result of this: a sibling's build makes every other
sharer's mirror read stale.

**3. Corrosion's per-checkout subdir.** Inside a keyed dir, Corrosion writes
`<workspace-folder>_<hash-of-manifest-path>`. Two checkouts give two names:
`pissues_5aa99` here, and `p500w2_06501` in issue 1901's evidence (worktree
`p500w2`).

## Not measured

- **The two-checkout case was NOT run.** It needs a second worktree with its
  submodules and its own CLI. What follows is deduced from the code and from
  facts 1–3:
  - **C/C++:** with `NROS_BUILD_ROOT` shared, a keyed dir holds
    `<A>_<h>/` and `<B>_<h>/`. `mirror-generated-header.sh` copies the
    NEWEST `cargo/*/<gen>/nros/<name>` (issue 0987's rule, the self-test case
    "the newest shared copy wins"). So checkout A's mirror can copy checkout
    B's header, against an archive built from A's sources. The issue 1593 guard
    (`check_probe_root`) compares the build dir's `nano_ros_DIR` only, so it
    does not see this.
  - **Rust:** both checkouts' harnesses share `<root>/metadata-probe`. Each
    reaches `nros` by its own absolute path, so every shared crate gets two
    units (issue 0616's shape). The `probe-<component>` uplift slot from fact 1
    becomes shared across checkouts too.
- **The concurrent race was NOT measured.** `cargo run` builds under the
  target-dir lock and then executes the uplifted path. As far as I know (not
  checked against cargo's source), the lock is released before the exec. If so,
  two syncs with the same component name in parallel (two workspaces in one
  fixture build, or two checkouts) can run each other's `probe-<component>`,
  and a sidecar then describes the wrong component. Run one after another,
  cargo re-links the slot, so nothing goes wrong in that case.
- Whether any sidecar on this host was ever written by the wrong binary.

## Fix direction

1. Make the harness bin name unique per harness, not per component: put the
   workspace (or a hash of the harness dir) in the `[[bin]]` name. Or run the
   binary from `deps/` through cargo's `--message-format=json` artifact path
   instead of the uplifted name.
2. Put the checkout in both shared roots under `NROS_BUILD_ROOT`
   (`<root>/<checkout-id>/metadata-probe`, or add a checkout field to the
   corrosion key). An explicit cross-checkout cache then becomes a deliberate
   opt-in, not a side effect.
3. `mirror-generated-header.sh` should resolve its source from the build dir's
   OWN Corrosion subdir (the one this configure's manifest path hashes to), not
   from the newest of `cargo/*/`.

Gates for the class:

- A check over every shared target dir: no two harness `Cargo.toml` files that
  can reach one target dir may declare the same `[[bin]]` name. This is
  checkable from the generated harness dirs, or statically from `probe_slug`'s
  inputs.
- A self-test in `mirror-generated-header.sh`: two Corrosion subdirs in one
  `cargo/`, where the build dir's own subdir is the older one. The resolver must
  pick its own subdir.
- The two-checkout measurement this issue did not run: two worktrees, one
  `NROS_BUILD_ROOT`, the same workspace synced in each with a header-changing
  edit in one. Then compare each probe dir's mirror with its own checkout's
  `nros-c-generated` header.

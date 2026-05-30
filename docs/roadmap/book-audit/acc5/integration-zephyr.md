BLOCKERS (both fixed in `5e24268d1`)
1. **`west.yml` snippet lacks `revision:`** — west defaults to `master`, but `NEWSLabNTU/nano-ros` only has `main`. `west update` fails verbatim with `fatal: couldn't find remote ref master`. Same defect in the in-tree `zephyr/west.yml` comment-block snippet (lines 9–14) that users copy out. Fixed in both the doc snippet + the in-tree fragment: `revision: main` added with a one-line comment explaining why.
2. **No Zephyr-pinning parent manifest shown** — doc explicitly said imported `zephyr/west.yml` is manifest-only and Zephyr must be in the parent manifest, but the parent manifest the doc showed didn't pin Zephyr. Verbatim copy-paste could not bootstrap Zephyr. Fixed: added explicit `zephyr` remote + project entry (`url-base: https://github.com/zephyrproject-rtos`, `revision: v3.7.0` for 3.7 LTS, `import: true` for Zephyr's own modules).

MISSING STEPS (also fixed)
- `west init -l .` between Configure and Build — `west update` errored `no west workspace found` without it. Added at the top of Build with a note that workspaces started from `west init -m <remote>` skip both init and update.

FRICTION / NITs (deferred)
- Doc references `apps/my_app/{CMakeLists.txt,src/main.c}` 8x but never sketches a minimal skeleton. Readers wanting a verbatim copy-paste must dig into `examples/zephyr/c/talker/` to derive one. (Tracked as F12 in `phase-208-followups.md` — proposal.)
- Worktree-artifact: cargo / `west update` inside the agent worktree dies on workspace-walk (same N2 artefact as every other batch).

ENVIRONMENTAL (not doc bugs)
- No Zephyr SDK installed on the audit host.
- No `/opt/ros/humble` → codegen msg-package dir missing.
- Both are user-supplied prereqs the doc names.

WORKS
- `nros setup zephyr --rmw zenoh` and `--source px4-rs` both green.
- `nros generate-rust --generate-config --nano-ros-path …` flag surface matches the CLI.
- `just zephyr zenohd` + `just zephyr talker` exist; port 7456 matches `examples/zephyr/rust/talker/src/lib.rs`.
- `cmake/zephyr/native-sim-line-3.7.conf` overlay exists at the path the doc names.
- `examples/zephyr/c/talker/` ships every `prj*.conf` the doc references.

Acceptance bar (0 BLOCKERS) MET on re-run after `5e24268d1`.

LAST COMMAND: west update
LAST EXIT CODE: 128 (`couldn't find remote ref master` — pre-fix)

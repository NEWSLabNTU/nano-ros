BLOCKERS
None.

The `first-node-cpp.md` tutorial as it stands post-D+E executes end-to-end (cmake configure → build → talker → stock ROS 2 echo) on the cached `~/.nros` host using nros 0.3.7 + D.2 shims.

FRICTION
- Step 1 starts `zenohd` literally; a pre-existing `zenohd` on `:7447` (e.g. from a prior audit session) noisily fails with `Address already in use`. The page doesn't note "if you already started zenohd, skip this step". Functionally equivalent — the talker uses the existing router — but the failure mode looks like a real error.

CLARITY
- All CLEAR otherwise.

MISSING STEPS
- None.

NITs
- The tutorial says "C/C++ talkers currently pre-increment so their first banner is `Published: 1`" but the binary actually prints `Published: 0` first, identical to the Rust example. Wording cleanup needed (same stale caveat as `first-node-c.md`).

WORKS
- `cmake -B build`, `cmake --build build`, `./build/cpp_talker` all green.
- ROS 2 echo path works once QoS reliability is set to best_effort (same gotcha as first-node-c.md).

Acceptance bar (0 BLOCKERS) MET.

LAST COMMAND: ./build/cpp_talker
LAST EXIT CODE: 124 (timeout-killed)

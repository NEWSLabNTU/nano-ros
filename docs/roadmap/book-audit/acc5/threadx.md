BLOCKERS
None.

Doc tutorial path (zenoh) passes **0 BLOCKERS** on both threadx-linux and threadx-riscv64 from a regular clone. Both `nros setup` calls succeed, `source ./setup.bash` is clean, `just threadx_linux talker` produces the documented `Published: 0..9` output, and `just threadx_riscv64 talker` boots QEMU and publishes via Slirp 10.0.2.2.

FRICTION
- L11–13: claims nros-cpp doesn't target ThreadX, but `build-fixtures` builds `threadx_cpp_*` and `riscv64_threadx_cpp_*` on both flavours.
- L117–119: claims `nros setup threadx-linux` creates `tap-tx0`. It doesn't — talker still runs via loopback fallback.
- L170–173: "Published: 0 within 3 seconds" assumes warm cache; cold run compiles ~80 s first.
- `just threadx_riscv64 build-fixtures` fails on the C cyclonedds CMake regenerate sub-step (zenoh artifacts build fine).

CLARITY
- All CLEAR.

MISSING STEPS
- No cold-build timing caveat.

NITs
- Worktree-only artifact: cargo builds inside the agent worktree fail with workspace-membership errors because the root `Cargo.toml` exclude list paths point at the real-clone tree. Not a doc bug.

WORKS
- nros setup threadx-linux + qemu-riscv64-threadx both reach "ready".
- `source ./setup.bash` clean.
- `just threadx_linux talker` → documented `Published: 0..9`.
- `just threadx_riscv64 talker` boots QEMU + publishes via Slirp 10.0.2.2.

Acceptance bar (0 BLOCKERS) MET.

LAST COMMAND: pgrep -af zenohd; pgrep -af qemu-system; echo done
LAST EXIT CODE: 0

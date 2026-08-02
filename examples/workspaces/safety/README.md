# `safety` — E2E message integrity, one workspace

Phase-331 W4 folded `ws-safety-{c,cpp,rust}` into this one, for the reason
RFC-0066 gives for the `features/` consolidation: coverage was being expressed
as DIRECTORIES — one workspace per language — which multiplied build cost
without multiplying what was actually covered.

The axis here is the **launch file**, not the directory. `demo_bringup/launch/`
carries one talker and one listener entry per language, and each resolves to its
own model.

## What it demonstrates

End-to-end message integrity: the talker attaches a CRC to each `/chatter`
frame, and the listener validates it and counts only the frames that pass. The
capability is declared once, in the bringup:

```toml
# src/demo_bringup/system.toml
[system]
features = ["safety"]
```

and lowers to `NANO_ROS_SAFETY_E2E` for the C/C++ build and the `safety-e2e`
cargo feature for the runtime. It is zenoh-only — the CRC path lives in that
backend.

## Layout

```
src/
  demo_bringup/                    one launch file per (role × language)
  {c,cpp,rust}_safety_talker_pkg/  the node packages
  {c,cpp,rust}_safety_listener_pkg/
  native_<lang>_safety_<role>_entry/   native entries, six of them
  zephyr_rust_safety_entry/            the embedded entry
```

Languages sit side by side so the language seam is exercised inside one
workspace rather than across three copies of it.

## Building

One platform per configure, like every workspace here (RFC-0026):

```sh
cmake -S . -B build -DNANO_ROS_PLATFORM=posix
cmake --build build
```

Each entry's model comes from its launch file; `nros sync` resolves one per
entry.

Do NOT pass `-DNANO_ROS_SAFETY_E2E=ON` by hand. The declaration above is the
source of truth and reaches the build on its own (phase-323); forcing the knob
was the workaround that outlived its own issue, and the SSoT gate now rejects
it.

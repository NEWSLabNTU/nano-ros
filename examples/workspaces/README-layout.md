# Workspace example layout

`examples/workspaces/{rust,c,cpp}` are read **side by side** — they express the
same system in three languages, so a reader can compare them directly. Their
structure is therefore kept parallel on purpose (phase-331 W2b, RFC-0066).

## Naming rules

| rule | why |
|---|---|
| **No language prefix** in a single-language workspace (`talker_pkg`, not `c_talker_pkg`) | the directory already names the language. Prefixes are kept in `mixed` and `features`, where languages coexist and the prefix carries information |
| **Roles, not payloads** — `service_server_pkg`, not `add_server_pkg` | AddTwoInts is what the demo sends; the ROLE is what is being compared across languages |
| **One platform vocabulary** for entries — `freertos_entry`, `nuttx_entry`, `threadx_entry`, `zephyr_entry`, `esp32_entry` | no `qemu_` or `native_` qualifiers; the board is named once, the same way everywhere |
| **Node names and executables are NOT normalised** | a node name is the ROS wire identity: it appears in `ros2 node list`, in resolved models, and in test expectations. `add_server` / `fib_server` stay, and c and cpp already agree on them |

Check the invariant with:

```sh
diff <(ls examples/workspaces/c/src) <(ls examples/workspaces/rust/src)
```

Only genuine coverage differences should appear — never a naming difference.

## Known coverage differences (2026-08-02)

These are real gaps, not naming drift. Each is a candidate for closing:

| present in | missing from | what it is |
|---|---|---|
| rust | c, cpp | `esp32_entry` — the ESP32-C3 board build |
| rust | c, cpp | `native_service_inprocess_entry` — same-process service round-trip |
| rust | c, cpp | `native_showcase_entry` — the combined showcase launch |
| rust | c, cpp | `zephyr_entry_robot1` — per-host Zephyr multi-host entry |
| c | cpp | `nuttx_entry` — the NuttX board build |

`mixed` is deliberately different: it is the language SEAM (one entry, components
from several languages), so it keeps language prefixes and does not mirror the
node set.

`features` holds the capability demos (params, lifecycle, QoS, custom messages,
remap) for all three languages and is **native only** — `param_services` and
`lifecycle` are alloc-gated, and an embedded image must opt into them
explicitly, so keeping them here leaves the language workspaces' embedded
entries clean.

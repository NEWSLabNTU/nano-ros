#!/usr/bin/env bash
# scripts/zephyr/ctf-app-marker-patch.sh <workspace-dir> <event-id>
#
# Issue 2766 -- the CTF `app_marker` event, so nano-ros's callback and
# application trace markers reach a Zephyr CTF stream with no consumer patch.
#
# nano-ros's Zephyr trace sink, `nros_zephyr_trace_marker`
# (`zephyr/nros_platform_zephyr_shims.c`), forwards every callback-trace and
# `rclc_trace_marker` / `rclcpp::trace_marker` / `emit_app_marker` event to
# `sys_trace_app_marker` through a WEAK reference. Stock Zephyr defines no such
# function, so without this patch the reference is null, the sink skips the
# call, and a CTF image records no marker at all, with no warning. The event
# came from a patch the Autoware safety island carried
# (`patches/zephyr/0002-ctf-app-marker-event.patch`); this is that patch, now
# applied by nano-ros's own patch set for every consumer.
#
# What it adds, in `subsys/tracing/ctf/`:
#   ctf_top.h      CTF_EVENT_APP_MARKER = <event-id> and ctf_top_app_marker()
#   ctf_top.c      void sys_trace_app_marker(uint32_t marker_id, uint32_t arg)
#   tsdl/metadata  event { name = app_marker; fields: marker_id, arg }
#
# The event NAME and FIELDS are the same on every line, so a reader that keys
# on the metadata (babeltrace, or the island's decoder) reads both. The ID is
# the caller's, because the id space differs by line: 3.7 has an 8-bit id and
# the island's id 0x70 is free there; 4.4 widened it to 16 bits and gave 0x70
# to `gpio_port_set_bits_raw_exit`. The id is refused if the tree already
# uses it, so a pin move that claims it fails here, not as two events
# sharing one id in a trace.
#
# A plain exported function rather than a SYS_PORT_TRACING hook: the caller
# is nano-ros's C shim, which declares it itself and must not include Zephyr's
# tracing internals. 4.3+ has `sys_trace_named_event`, which a later change
# could route this through; it is not used here so that both lines emit the
# same event.
#
# Idempotent (a tree carrying the marker is reported as applied). Inert unless
# an image enables CONFIG_TRACING_CTF: these files compile only then.
set -euo pipefail

WORKSPACE="${1:?usage: ctf-app-marker-patch.sh <workspace-dir> <event-id>}"
EVENT_ID="${2:?usage: ctf-app-marker-patch.sh <workspace-dir> <event-id>}"
CTF="$WORKSPACE/zephyr/subsys/tracing/ctf"
for f in "$CTF/ctf_top.h" "$CTF/ctf_top.c" "$CTF/tsdl/metadata"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f missing" >&2
        exit 1
    fi
done

python3 - "$CTF" "$EVENT_ID" <<'PYEOF'
import re
import sys

ctf, raw_id = sys.argv[1], sys.argv[2]
MARK = "nano-ros: CTF app_marker event"
hdr_p, src_p, meta_p = f"{ctf}/ctf_top.h", f"{ctf}/ctf_top.c", f"{ctf}/tsdl/metadata"
hdr, src, meta = (open(p).read() for p in (hdr_p, src_p, meta_p))

carried = [MARK in t for t in (hdr, src, meta)]
if all(carried):
    print("[ctf-app-marker-patch] already applied")
    sys.exit(0)
if any(carried):
    sys.stderr.write(
        "ERROR: the app_marker event is in some of ctf_top.h, ctf_top.c and "
        "tsdl/metadata but not all; restore the three files and re-run\n")
    sys.exit(1)

try:
    event_id = int(raw_id, 0)
except ValueError:
    sys.stderr.write(f"ERROR: event id {raw_id!r} is not a number\n")
    sys.exit(1)

# The id width this tree's events use: every CTF_EVENT names its literal type.
widths = set(re.findall(r"CTF_LITERAL\((uint8_t|uint16_t), CTF_EVENT_", hdr))
if len(widths) != 1:
    sys.stderr.write(f"ERROR: cannot tell the CTF event id width (saw {sorted(widths)})\n")
    sys.exit(1)
id_type = widths.pop()
limit = 0xFF if id_type == "uint8_t" else 0xFFFF
if not 0 < event_id <= limit:
    sys.stderr.write(f"ERROR: event id {event_id:#x} does not fit this tree's {id_type} id\n")
    sys.exit(1)

used = {int(v, 16) for v in re.findall(r"CTF_EVENT_\w+\s*=\s*(0x[0-9A-Fa-f]+)", hdr)}
used |= {int(v, 16) for v in re.findall(r"\bid\s*=\s*(0x[0-9A-Fa-f]+);", meta)}
if event_id in used:
    sys.stderr.write(
        f"ERROR: CTF event id {event_id:#x} is already used in this Zephyr tree; "
        "pick a free one in scripts/zephyr/patches/<line>.sh\n")
    sys.exit(1)

enum_end = "\n} ctf_event_t;"
guard_end = "#endif /* SUBSYS_DEBUG_TRACING_CTF_TOP_H */"
for anchor in (enum_end, guard_end):
    if hdr.count(anchor) != 1:
        sys.stderr.write(f"ERROR: anchor not found once in {hdr_p}: {anchor.strip()}\n")
        sys.exit(1)

hdr = hdr.replace(
    enum_end,
    f"\t/* {MARK} (issue 2766): a generic application marker. */\n"
    f"\tCTF_EVENT_APP_MARKER = {event_id:#x},\n" + enum_end,
    1,
)
hdr = hdr.replace(
    guard_end,
    f"/* {MARK} (issue 2766). */\n"
    "static inline void ctf_top_app_marker(uint32_t marker_id, uint32_t arg)\n"
    "{\n"
    f"\tCTF_EVENT(CTF_LITERAL({id_type}, CTF_EVENT_APP_MARKER), marker_id, arg);\n"
    "}\n\n" + guard_end,
    1,
)
src = src.rstrip("\n") + (
    "\n\n"
    f"/* {MARK} (issue 2766).\n"
    " *\n"
    " * A plain exported function: nano-ros's trace sink declares it itself\n"
    " * (weak) rather than including the tracing internals. `marker_id` names\n"
    " * the marker site, `arg` is site-defined.\n"
    " */\n"
    "void sys_trace_app_marker(uint32_t marker_id, uint32_t arg);\n"
    "void sys_trace_app_marker(uint32_t marker_id, uint32_t arg)\n"
    "{\n"
    "\tctf_top_app_marker(marker_id, arg);\n"
    "}\n"
)
meta = meta.rstrip("\n") + (
    "\n\n"
    f"/* {MARK} (issue 2766): a generic application marker, in the same\n"
    " * stream as the scheduling events. `marker_id` names the marker site,\n"
    " * `arg` is site-defined.\n"
    " */\n"
    "event {\n"
    "\tname = app_marker;\n"
    f"\tid = {event_id:#x};\n"
    "\tfields := struct {\n"
    "\t\tuint32_t marker_id;\n"
    "\t\tuint32_t arg;\n"
    "\t};\n"
    "};\n"
)

for p, t in ((hdr_p, hdr), (src_p, src), (meta_p, meta)):
    open(p, "w").write(t)
print(f"[ctf-app-marker-patch] patched {ctf} (app_marker, id {event_id:#x}, {id_type})")
PYEOF

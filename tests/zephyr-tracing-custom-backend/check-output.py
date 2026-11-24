#!/usr/bin/env python3
"""Check what `tests/zephyr-tracing-custom-backend` printed (issue 2878).

Usage: check-output.py <run.log> <zephyr-line>

Passes when the application's own backend (`nros_test_ring`) was initialised
once and received a CTF stream holding the `thread_name_set` event main()
caused: the main thread's id (u32, little-endian) followed by its new name,
NUL-terminated. That is the payload layout of the stock CTF event on both
lines (`ctf_top_thread_name_set(thread_id, name)`), so the bytes come from the
CTF formatter: the stream went through this backend, not merely past it.
"""

import sys

NAME = "nros_test_ring"
THREAD_NAME = b"nros-2878\0"


def main() -> int:
    log, line = sys.argv[1], sys.argv[2]
    text = open(log, errors="replace").read()

    def fail(msg: str) -> int:
        sys.stderr.write(f"[FAIL] tracing-custom-backend ({line}): {msg}\n--- {log}\n{text}\n")
        return 1

    fields, hexdump = {}, []
    for raw in text.splitlines():
        key, sep, val = raw.partition(": ")
        if key == "tracing-hex":
            hexdump.append(val.strip())
        elif sep and key.startswith("tracing-"):
            fields[key] = val.strip()
    if "tracing-done" not in text:
        return fail("the image did not reach tracing-done")
    if fields.get("tracing-backend") != NAME:
        return fail(f"backend name {fields.get('tracing-backend')!r}, want {NAME!r}")
    if fields.get("tracing-init-calls") != "1":
        return fail(f"backend init() ran {fields.get('tracing-init-calls')} times, want 1"
                    " (tracing_core.c did not select it)")
    stream = bytes.fromhex("".join(hexdump))
    if len(stream) == 0 or str(len(stream)) != fields.get("tracing-bytes"):
        return fail(f"stream holds {len(stream)} bytes, image reported {fields.get('tracing-bytes')}")
    try:
        thread = int(fields["tracing-thread"], 16).to_bytes(4, "little")
    except (KeyError, ValueError):
        return fail("no tracing-thread line")
    payload = thread + THREAD_NAME
    if payload not in stream:
        return fail(f"no thread_name_set payload ({payload.hex()}) in the {len(stream)}-byte stream")
    print(f"[PASS] tracing-custom-backend ({line}): {NAME} selected by Kconfig, "
          f"{len(stream)} CTF bytes, thread_name_set {payload.hex()} present")
    return 0


if __name__ == "__main__":
    sys.exit(main())

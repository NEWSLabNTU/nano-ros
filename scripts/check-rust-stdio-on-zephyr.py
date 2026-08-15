#!/usr/bin/env python3
"""Issue 0589 — no raw Rust std stdio in crates Zephyr links.

`std::println!` / `std::eprintln!` are FATAL on Zephyr `native_sim`. Rust std
stdio goes through the POSIX device-io fdtable, fds 0/1/2 all carry
`stdinout_fd_op_vtable`, and its write method under `CONFIG_BOARD_NATIVE_POSIX`
is

    return zvfs_write(1, buffer, count);      /* called FROM zvfs_write(1, …) */

with no termination. `k_mutex` is recursive so it never deadlocks — it exhausts
the stack and SIGSEGVs the image (observed `lock_count = 104756`). C/C++
`printf` misses this entirely because picolibc uses the console hook, which is
why it stayed latent until a Rust diagnostic was added to an error path that
actually ran: issue 0557's fix routed a `NodeError` through a mapper carrying
`eprintln!("nros: NodeError::…")`, and the print killed the guest it was there
to explain.

The failure mode is what makes a gate worth having. The offending line is
correct-looking, ordinary Rust, added on an error path that is rarely taken —
so it lands in review, ships, and detonates months later on the one platform
where the return code was the only diagnostic anyone had.

Crates in scope link into Zephyr images. They must go through `cpp_diag!` (or an
equivalent platform-aware sink), never std stdio directly.

Run: python3 scripts/check-rust-stdio-on-zephyr.py
"""

from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent

# Crates whose code is compiled into Zephyr images. Adding one here is cheap;
# missing one is how this reappears.
SCOPED = [
    "packages/api/nros-cpp/src",
    "packages/api/nros-c/src",
]

# `std::println!`, `eprintln!`, `print!`, `eprint!` — bare or `std::`-qualified.
STDIO = re.compile(r"(?<![A-Za-z_])(?:std::)?(?:e?print(?:ln)?)\s*!")

# The macro definition itself legitimately names the forbidden macro in its
# expansion, and doc comments quote it to explain the hazard.
ALLOW_MARK = "nros-allow-std-stdio"


def main() -> int:
    failures: list[str] = []
    scanned = 0

    for rel in SCOPED:
        root = REPO / rel
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*.rs")):
            scanned += 1
            in_macro_def = False
            for lineno, line in enumerate(path.read_text().splitlines(), 1):
                stripped = line.lstrip()
                if stripped.startswith("//"):
                    continue  # a comment explaining the hazard is not the hazard
                # The sanctioned wrapper: its non-Zephyr arm IS `std::eprintln!`.
                if "macro_rules! cpp_diag" in line:
                    in_macro_def = True
                    continue
                if in_macro_def:
                    if stripped.startswith("}"):
                        in_macro_def = False
                    continue
                if ALLOW_MARK in line:
                    continue
                if STDIO.search(line):
                    failures.append(
                        f"  {path.relative_to(REPO)}:{lineno}\n"
                        f"    {stripped}\n"
                        f"    -> use `crate::cpp_diag!(…)`; std stdio SIGSEGVs a "
                        f"Zephyr native_sim image (issue 0589)"
                    )

    if failures:
        print("check-rust-stdio-on-zephyr: FAILED — raw std stdio in a Zephyr-linked crate\n")
        print("\n".join(failures))
        print(
            "\n  On Zephyr native_sim this does not print — it recurses in `zvfs_write`\n"
            "  until the stack is gone. A diagnostic that can kill the image it is\n"
            "  diagnosing is worse than no diagnostic. See docs/issues/0589-*.md.\n"
            f"  Genuinely unavoidable? mark the line `{ALLOW_MARK}` and say why."
        )
        return 1

    print(f"check-rust-stdio-on-zephyr: OK ({scanned} file(s) in {len(SCOPED)} crate(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

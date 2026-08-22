#!/usr/bin/env python3
"""issue 0636 option 3 — every tier spin loop reaches a scheduling point.

`boot_tier_index` stopped the starvation by making the session owner the tier
that outranks nothing, and that is what fixed the issue. The guarantee it gives,
though, holds only while the priority ORDER is right: any tier that outranks
another and then spins WITHOUT BLOCKING owns a uniprocessor forever, because
under SCHED_FIFO a thread yields the CPU only by blocking.

Whether a spin blocks is a property of the transport, not of the tier —
`Executor::spin_once` drives I/O with a ZERO timeout whenever a wake already
fired, so under sustained arrival the loop never blocks, exactly when the system
is busiest. `nros_platform::board::tier::TierSpinGap` (Rust) and
`nros_tier_spin_gap_step` (C, the same implementation) make the gap structural.

This gate keeps the rule from decaying the way the tier-priority marker did: it
was enforced on NuttX and in Rust only, so FreeRTOS was silently exempt for two
phases. FreeRTOS, symmetrically, was the ONLY kernel that had solved the gap —
by an unconditional `vTaskDelay(1)` costing a tick on every iteration, including
the ones that already blocked — while the three other kernels running the
identical loop had nothing. One rule, one implementation, checked in every file
that runs a tier.

What it checks: a file that drives a tier's spin in a loop must also name the
gap helper. Comments are stripped first (issue 0719's trap: a mechanical grep
that reads prose reports a clean sweep over a site it never examined).

Run: python3 scripts/check-tier-spin-gap.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Drives a tier's executor in a loop — the C runners and the Rust boards.
SPINS = re.compile(r"\b(nros_cpp_spin_once|spin_once_counted|spin_once)\s*[(:]")
# The shared rule, in either language's spelling.
GAP = re.compile(r"\b(nros_tier_spin_gap_step|TierSpinGap)\b")
# A loop around it. Both languages.
LOOPS = re.compile(r"\b(for\s*\(\s*;\s*;\s*\)|while\s*\(\s*1\s*\)|loop\s*\{)")

# Files that spin an executor but are NOT a tier loop, with the reason.
EXEMPT = {
    # Single-tier / bare-metal entries: one executor, no other tier to starve.
    # The gap's whole subject is a tier that outranks another one.
    "packages/boards/nros-board-mps2-an385/src/entry.rs": "single-tier bare-metal entry",
    "packages/boards/nros-board-mps2-an385/src/rtic.rs": "RTIC idle task, no tiers",
    "packages/boards/nros-board-esp32-qemu/src/board_entry.rs": "single-tier entry",
    "packages/platform/nros-platform/src/board/rtic_entry.rs": "RTIC seam, no tiers",
    "packages/platform/nros-platform/src/board/runtime.rs": "the trait, not a loop",
    # The executor itself, and the API surface over it.
    "packages/core/nros-node/src/executor/spin.rs": "the executor being driven",
    "packages/api/nros/src/node_runtime.rs": "the runtime wrapper",
    # The rule's own implementation.
    "packages/platform/nros-platform/src/board/tier.rs": "the helper itself",
    # ---- Loops that ALREADY block unconditionally, every iteration ----
    #
    # These satisfy the rule more strongly than the gap does, and converting
    # them would REMOVE pacing rather than add a guarantee: their sleep is the
    # whole reason the loop does not burn a core. `nros_board_native_run_tiers`
    # ends every iteration with `platform_sleep_us(boot_tier.spin_period_us)`,
    # so its boot thread reaches a scheduling point once per declared period
    # come what may. Swapping that for a 1 ms gap per 10 ms window would make a
    # 100 ms tier spin ~100x hotter on a hosted target.
    "packages/api/nros-cpp/src/lib.rs": "native run_tiers sleeps its full period every iteration",
    # The `nros::main!` emitters drive ONE executor — a single-tier image has no
    # second tier to starve, which is the gap's entire subject.
    "packages/core/nros-macros/src/main_macro.rs": "single-executor entry shapes, no tiers",
}


def strip_comments(text, c_like):
    if c_like:
        text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
        return re.sub(r"(?m)//.*$", "", text)
    return re.sub(r"(?m)//.*$", "", text)


def tracked():
    out = subprocess.run(
        ["git", "ls-files", "-z", "packages/*.rs", "packages/*.c"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    )
    return [p for p in out.stdout.split("\0") if p.endswith((".rs", ".c"))]


def offenders():
    bad = []
    for rel in tracked():
        if rel in EXEMPT:
            continue
        path = os.path.join(ROOT, rel)
        try:
            raw = open(path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        # Only files that actually run tiers. `run_tiers` / `spin_tier` name the
        # shape; keying on "spins in a loop" alone would sweep in every example.
        if "run_tiers" not in raw and "spin_tier" not in raw:
            continue
        body = strip_comments(raw, rel.endswith(".c"))
        if not (SPINS.search(body) and LOOPS.search(body)):
            continue
        if GAP.search(body):
            continue
        bad.append(rel)
    return sorted(bad)


def self_test():
    cases = [
        ("for (;;) { nros_cpp_spin_once(x, p); } run_tiers", True, "C tier loop with no gap"),
        ("for (;;) { nros_cpp_spin_once(x, p);\n s = nros_tier_spin_gap_step(s,a,b,c); } run_tiers",
         False, "C tier loop with the gap"),
        ("fn run_tiers() { loop { crt.spin_once(p); } }", True, "Rust tier loop with no gap"),
        ("fn run_tiers() { let mut g = TierSpinGap::new(p); loop { crt.spin_once(p); } }",
         False, "Rust tier loop with the gap"),
        # THE trap (issue 0719): the name in prose must not satisfy the rule.
        ("/* uses nros_tier_spin_gap_step */\nfor (;;) { nros_cpp_spin_once(x, p); } run_tiers",
         True, "gap named only in a COMMENT"),
        ("fn helper() { loop { crt.spin_once(p); } }", False, "not a tier runner"),
    ]
    bad = []
    for body, should_flag, label in cases:
        c_like = "/*" in body or ";" in body and "fn " not in body
        stripped = strip_comments(body, c_like)
        runs_tiers = "run_tiers" in body or "spin_tier" in body
        flagged = bool(
            runs_tiers
            and SPINS.search(stripped)
            and LOOPS.search(stripped)
            and not GAP.search(stripped)
        )
        if flagged != should_flag:
            bad.append(f"self-test: {label!r} -> flagged={flagged}, expected {should_flag}")
    if bad:
        for b in bad:
            sys.stderr.write(b + "\n")
        sys.exit(2)
    print(f"check-tier-spin-gap --self-test: OK ({len(cases)} case(s))")


def main():
    self_test()
    bad = offenders()
    if bad:
        sys.stderr.write("check-tier-spin-gap: FAILED — tier loop(s) with no scheduled gap:\n\n")
        for rel in bad:
            sys.stderr.write(f"  {rel}\n")
        sys.stderr.write(
            "\n  Each drives a tier's executor in a loop without the shared gap\n"
            "  (issue 0636 option 3). The executor skips its own wait whenever a\n"
            "  wake already fired, so under load such a loop never blocks, and\n"
            "  under SCHED_FIFO a thread that never blocks never lets a\n"
            "  lower-priority tier run. Add:\n\n"
            "      Rust: let mut gap = nros_platform::TierSpinGap::new(tier.spin_period_us);\n"
            "            loop { let it = gap.mark(); ...; gap.after_spin(it); }\n"
            "      C:    uint64_t s = 0;\n"
            "            for (;;) { uint64_t it = nros_platform_clock_ns(); ...;\n"
            "                       s = nros_tier_spin_gap_step(s, it,\n"
            "                               nros_platform_clock_ns(), period_us); }\n\n"
            "  A loop with no second tier to starve goes in EXEMPT with its reason.\n"
        )
        return 1
    print("tier spin gap: OK (every tier loop reaches a scheduling point)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

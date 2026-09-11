#!/usr/bin/env python3
"""Every `nros` verb CMAKE invokes is exempt from the workspace check.

phase-413 W2. Phase-431 W1 added a second question to the CLI's guard: is this
binary FOREIGN to the checkout my cwd is in? For a verb a human types that is a
real question. For a verb the BUILD SYSTEM runs it has no meaning — cmake
invokes the tool path it was configured with (`-D_NANO_ROS_CODEGEN_TOOL`) from
a build OUTPUT directory, which on a CI runner may sit under an entirely
different nano-ros tree.

That is what took the tier-2 lane red: `nros codegen resolve-deps` refused a
correct, freshly built binary because the zephyr west workspace lives under
a second nano-ros tree on the runner while the binary came from the checkout
GitHub Actions cloned.
The fix exempted `codegen`, `codegen-system` and `generate-rust` — the three
verbs whose symptom was visible — and left FIVE more of the same class:
`ws providers`, `ws order`, `ws entity-inventory`, `ws board-facts` and
`ws entity-facts`, all invoked by cmake, none setting `WORKING_DIRECTORY`, all
measured to hit the same refusal from a foreign checkout.

TWO OF THEM FAIL SILENTLY, which is why a gate and not just a fix.
`nros_resolve_board_facts` and `nros_read_entity_facts` degrade a refusal to
`message(STATUS …)` + `return()` and let the build continue with an EMPTY facts
environment. A red lane is a verdict; that is no verdict at all, and it is the
0460 class — a knob that reaches one lane and not the other.

HOW IT DECIDES

  invoked  — `COMMAND "${TOOL}" <verb> [<sub>]` in `cmake/**/*.cmake`, plus the
             `set(_args <verb> <sub> …)` indirection `NanoRosBoardFacts.cmake`
             uses, which a `COMMAND`-anchored regex misses. Comment lines are
             dropped: `ws model-dims` appears only in prose, and so did
             `ws check-board-projections` for as long as that verb existed
             (phase-445 W6 retired it) — a gate that cannot tell a command from
             a sentence about a command is worse than no gate
             (`check-workflow-repo-env`'s rule).

  exempt   — read from `ws_cmd_name`'s match arms and
             `workspace_check_applies`'s in the CLI sources. NEVER a second
             list here: an allow-list beside the rule is the drift this repo
             keeps paying for.

Run:  python3 scripts/check-build-tool-verbs-exempt.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB_RS = os.path.join("packages", "cli", "nros-cli-core", "src", "lib.rs")
GUARD_RS = os.path.join("packages", "cli", "nros-cli-core", "src", "stale_guard.rs")

# `Sub::Providers(_)` -> the verb a user types. The enum is CamelCase, the verb
# is kebab-case; clap's default rename is exactly that mapping.
def kebab(camel):
    return re.sub(r"(?<!^)(?=[A-Z])", "-", camel).lower()


def exempt_ws_subs(text):
    """The `ws` subcommands `ws_cmd_name` maps to the exempt bucket."""
    m = re.search(r"fn ws_cmd_name\([^)]*\)[^{]*\{(.*?)\n\}", text, re.S)
    if not m:
        return set()
    body = m.group(1)
    arm = re.search(r"((?:\s*\|?\s*cmd::ws::Sub::\w+\(_\))+)\s*=>\s*\"ws-build\"", body)
    if not arm:
        return set()
    return {kebab(n) for n in re.findall(r"Sub::(\w+)\(_\)", arm.group(1))}


def exempt_top_verbs(text):
    """Top-level verbs `workspace_check_applies` excludes."""
    m = re.search(r"fn workspace_check_applies\([^)]*\)[^{]*\{(.*?)\n\}", text, re.S)
    if not m:
        return set()
    body = m.group(1)
    inner = re.search(r"!matches!\s*\(\s*name\s*,(.*?)\)", body, re.S)
    if not inner:
        return set()
    return set(re.findall(r'"([a-z0-9-]+)"', inner.group(1)))


COMMENT = re.compile(r"^\s*#")
# `COMMAND "${X}" verb sub` and `set(_args verb sub …)`.
INVOKE = re.compile(r'(?:COMMAND\s+"\$\{[^}]+\}"|set\(\s*\w+)\s+([a-z][a-z0-9-]*)(?:\s+([a-z][a-z0-9-]*))?')


def invoked(root):
    """{(verb, sub_or_None)} that cmake actually runs."""
    files = subprocess.run(
        ["git", "ls-files", "cmake/*.cmake", "cmake/**/*.cmake"],
        cwd=root, capture_output=True, text=True,
    ).stdout.split()
    out = set()
    for rel in files:
        try:
            with open(os.path.join(root, rel), encoding="utf8") as fh:
                lines = fh.read().split("\n")
        except OSError:
            continue
        for line in lines:
            if COMMENT.match(line):
                continue
            for m in INVOKE.finditer(line):
                out.add((m.group(1), m.group(2) or "", rel))
    return out


def self_test():
    assert kebab("BoardFacts") == "board-facts"
    assert kebab("Order") == "order"
    assert kebab("EntityInventory") == "entity-inventory"
    src = ('fn ws_cmd_name(a: &X) -> &str {\n    match a.command {\n'
           '        cmd::ws::Sub::Providers(_)\n        | cmd::ws::Sub::BoardFacts(_) '
           '=> "ws-build",\n        _ => "ws",\n    }\n}\n')
    assert exempt_ws_subs(src) == {"providers", "board-facts"}, exempt_ws_subs(src)
    g = ('fn workspace_check_applies(name: &str) -> bool {\n'
         '    !matches!(name, "codegen" | "ws-build")\n}\n')
    assert exempt_top_verbs(g) == {"codegen", "ws-build"}, exempt_top_verbs(g)
    # A comment line naming a verb is not an invocation.
    assert not COMMENT.match('  COMMAND "${T}" ws order')
    assert COMMENT.match('# Same role as `nros ws model-dims`')
    sys.stdout.write("check-build-tool-verbs-exempt self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    try:
        with open(os.path.join(ROOT, LIB_RS), encoding="utf8") as fh:
            lib = fh.read()
        with open(os.path.join(ROOT, GUARD_RS), encoding="utf8") as fh:
            guard = fh.read()
    except OSError as e:
        sys.stderr.write(f"error: cannot read the CLI sources: {e}\n")
        return 1

    ws_exempt = exempt_ws_subs(lib)
    top_exempt = exempt_top_verbs(guard)
    if not ws_exempt or not top_exempt:
        sys.stderr.write(
            "error: could not read the exempt sets from `ws_cmd_name` /\n"
            "`workspace_check_applies`. This gate would then accept anything.\n"
        )
        return 1
    if "ws-build" not in top_exempt:
        sys.stderr.write(
            "error: `workspace_check_applies` does not exempt `ws-build`, so the\n"
            "`ws_cmd_name` bucket is inert and every cmake `ws` verb is checked.\n"
        )
        return 1

    problems = []
    seen = 0
    for verb, sub, rel in sorted(invoked(ROOT)):
        if verb != "ws":
            continue
        if not sub:
            continue
        seen += 1
        if sub not in ws_exempt:
            problems.append(
                f"  {rel}: cmake invokes `nros ws {sub}`, which is NOT in\n"
                f"      `ws_cmd_name`'s exempt bucket. From a build directory under a\n"
                f"      different checkout it is refused with `this nros does not belong\n"
                f"      to the checkout it is being run against` — a red lane at best,\n"
                f"      and silently empty facts where the caller swallows it."
            )

    if problems:
        sys.stderr.write(
            "check-build-tool-verbs-exempt: %d problem(s)\n\n" % len(problems)
        )
        for p in problems:
            sys.stderr.write(p + "\n\n")
        sys.stderr.write(
            "  Add the Sub variant to `ws_cmd_name`'s `\"ws-build\"` arm in\n"
            "  packages/cli/nros-cli-core/src/lib.rs. It stays STALENESS-guarded;\n"
            "  only the meaningless cwd question is dropped.\n"
        )
        return 1

    print(
        f"check-build-tool-verbs-exempt: OK — {seen} cmake `ws` invocation(s), "
        f"all {len(ws_exempt)} exempt subcommand(s) "
        f"({', '.join(sorted(ws_exempt))}); top-level exempt: "
        f"{', '.join(sorted(top_exempt))}."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

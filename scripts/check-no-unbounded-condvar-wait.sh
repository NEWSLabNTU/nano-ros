#!/usr/bin/env bash
# phase-436 W5 (issue 1196) — the UNBOUNDED condvar wait stays confined to the
# shim that bridges an upstream API which is itself unbounded.
#
# `nros_platform_condvar_wait` blocks forever by construction: every port
# implements it with that port's forever spelling (`TX_WAIT_FOREVER`,
# `portMAX_DELAY`, `K_FOREVER`). A bounded `nros_platform_condvar_wait_until`
# sits directly beside it, and the executor's own wait already uses the bounded
# `nros_platform_wake_wait_ms`.
#
# ONE exemption, and it is not a grandfathering: `platform_aliases.c` implements
# zenoh-pico's `_z_condvar_wait`, whose CONTRACT is an unbounded wait. Bridging
# an unbounded upstream primitive to an unbounded platform primitive is the
# correct mapping; narrowing it there would silently change zenoh-pico's
# semantics. The exemption is by path, so a second such call anywhere else
# still has to be argued for.
#
# The risk this guards is the next backend or port: `condvar_wait` is the
# obvious spelling, so it gets reached for and the bound is lost silently and
# without review. A marked header states the rule; only a gate keeps it.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# phase-472 W5 — the population is every tracked Rust and C/C++ file, by KIND
# (`scripts/lib/file_kinds.py`), not `packages/{core,rmw,api}`: "the next
# backend or port" is this gate's stated risk, and ports live in
# `packages/platform/` and `packages/boards/`, which it never read. Reading
# them means telling a CALL from the DEFINITIONS every port carries, so the
# classification moved into Python below; exemptions stay keyed on the path.
exec python3 - "$@" <<'PY'
import os, re, sys
sys.path.insert(0, os.path.join("scripts", "lib"))
import file_kinds, comments

NAME = "nros_platform_condvar_wait"
# path -> why a CALL there is correct.
ALLOWED = {
    "packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c":
        "implements zenoh-pico's `_z_condvar_wait`, whose CONTRACT is unbounded",
    "packages/platform/nros-platform-cffi/src/lib.rs":
        "the Rust face of the primitive itself: `condvar_wait` forwards to the C symbol",
}
USE = re.compile(rf"\b{NAME}\s*\(")
# A definition or declaration: a type (or `fn`) directly before the name.
DEF_PREFIX = re.compile(r"(?:\bfn\s+|^\s*(?:(?:static|inline|extern|const|unsigned|signed)\s+)*"
                        r"[A-Za-z_][\w:]*[\s*]+)$")


def calls(text, lang):
    code = comments.strip_comments(text, lang, strings=True)
    out = []
    for m in USE.finditer(code):
        bol = code.rfind("\n", 0, m.start()) + 1
        prefix = code[bol:m.start()]
        if DEF_PREFIX.search(prefix) and prefix.strip() not in ("return", "else"):
            continue
        out.append((code.count("\n", 0, m.start()) + 1, text.splitlines()[code.count("\n", 0, m.start())].strip()))
    return out


def self_test():
    c = ("int8_t nros_platform_condvar_wait(void *cv, void *m);\n"
         "int8_t nros_platform_condvar_wait(void *cv, void *m) { return 0; }\n"
         "static int f(void *c, void *m) { return nros_platform_condvar_wait(c, m); }\n"
         "  rc = nros_platform_condvar_wait(c, m);\n"
         "  /* nros_platform_condvar_wait(c, m); */\n")
    got = [l for l, _t in calls(c, "c")]
    assert got == [3, 4], got
    rs = ("pub extern \"C\" fn nros_platform_condvar_wait(cv: *mut c_void) -> i8 { 0 }\n"
          "unsafe { nros_platform_condvar_wait(cv, m) }\n")
    assert [l for l, _t in calls(rs, "rust")] == [2], calls(rs, "rust")
    file_kinds.self_test()


self_test()
files = file_kinds.files_of_kind("rust", "c-family")
print(f"check-no-unbounded-condvar-wait: examined {len(files)} Rust/C/C++ file(s)")
if not files:
    sys.exit("check-no-unbounded-condvar-wait: empty population")
hits, seen = [], set()
for rel in files:
    try:
        text = open(rel, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    if NAME not in text:
        continue
    lang = "rust" if rel.endswith(".rs") else "c"
    for line, src in calls(text, lang):
        if rel in ALLOWED:
            seen.add(rel)
            continue
        hits.append(f"  {rel}:{line}: {src}")
stale = sorted(set(ALLOWED) - seen)
if hits or stale:
    if hits:
        print("check-no-unbounded-condvar-wait: the UNBOUNDED condvar wait is called outside its sanctioned bridges:", file=sys.stderr)
        print("\n".join(hits), file=sys.stderr)
        print("\n  nros_platform_condvar_wait has no deadline. Use\n"
              "  nros_platform_condvar_wait_until (absolute ms deadline), or the\n"
              "  executor's nros_platform_wake_wait_ms. See issue 1196, phase-436 W5.", file=sys.stderr)
    for s_ in stale:
        print(f"check-no-unbounded-condvar-wait: STALE exemption {s_} — it calls nothing; delete it.", file=sys.stderr)
    sys.exit(1)
print("check-no-unbounded-condvar-wait OK (confined to its sanctioned bridges).")
PY

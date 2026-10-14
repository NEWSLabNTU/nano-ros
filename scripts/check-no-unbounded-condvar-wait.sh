#!/usr/bin/env bash
# Issue 1196 — the UNBOUNDED condvar wait is gone from the platform ABI, and
# this gate keeps it gone.
#
# History. phase-436 W5 marked `nros_platform_condvar_wait` NOT REAL-TIME and
# confined its callers, by path, to the zenoh-pico alias shim, whose
# `_z_condvar_wait` implements an upstream API that is itself unbounded.
# phase-436 D2 then bridged that last caller through a deadline the shim owns
# (bounded slices; an expired slice is a spurious wakeup, which every caller
# already absorbs by re-testing its predicate) and deleted the symbol from
# `nros/platform.h`, every C port, the cffi export macro and the Rust
# `PlatformThreading` trait. `nros_platform_condvar_wait_until` is now the only
# condvar wait.
#
# What this refuses: the symbol coming back in ANY form — a definition, a
# declaration or a call of `nros_platform_condvar_wait`, or a Rust
# `fn condvar_wait(` (the trait method's spelling). The risk was always the
# next port or backend: `condvar_wait` is the obvious name, so it gets reached
# for and the bound is lost silently and without review. There is no
# exemption list; an unbounded wait has to be argued for by changing this
# gate, in review.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# phase-472 W5 — the population is every tracked Rust and C/C++ file, by KIND
# (`scripts/lib/file_kinds.py`): ports live in `packages/platform/` and
# `packages/boards/`, and examples carry ports too (`custom-platform`).
exec python3 - "$@" <<'PY'
import os, re, sys
sys.path.insert(0, os.path.join("scripts", "lib"))
import file_kinds, comments

C_NAME = re.compile(r"\bnros_platform_condvar_wait\s*\(")
RS_FN = re.compile(r"\bfn\s+condvar_wait\s*\(")


def hits_in(text, lang):
    code = comments.strip_comments(text, lang, strings=True)
    pats = [C_NAME] + ([RS_FN] if lang == "rust" else [])
    out = []
    for pat in pats:
        for m in pat.finditer(code):
            ln = code.count("\n", 0, m.start())
            out.append((ln + 1, text.splitlines()[ln].strip()))
    return sorted(out)


def self_test():
    c = ("int8_t nros_platform_condvar_wait(void *cv, void *m);\n"
         "int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t t);\n"
         "  rc = nros_platform_condvar_wait(c, m);\n"
         "  /* nros_platform_condvar_wait(c, m); */\n"
         "  rc = nros_platform_condvar_wait_until(c, m, t);\n")
    got = [l for l, _t in hits_in(c, "c")]
    assert got == [1, 3], got
    rs = ("pub extern \"C\" fn nros_platform_condvar_wait(cv: *mut c_void) -> i8 { 0 }\n"
          "    fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;\n"
          "    fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, t: u64) -> i8;\n"
          "    // fn condvar_wait(\n")
    got = [l for l, _t in hits_in(rs, "rust")]
    assert got == [1, 2], got
    file_kinds.self_test()


self_test()
files = file_kinds.files_of_kind("rust", "c-family")
print(f"check-no-unbounded-condvar-wait: examined {len(files)} Rust/C/C++ file(s)")
if not files:
    sys.exit("check-no-unbounded-condvar-wait: empty population")
hits = []
for rel in files:
    try:
        text = open(rel, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    if "condvar_wait" not in text:
        continue
    lang = "rust" if rel.endswith(".rs") else "c"
    for line, src in hits_in(text, lang):
        hits.append(f"  {rel}:{line}: {src}")
if hits:
    print("check-no-unbounded-condvar-wait: the UNBOUNDED condvar wait is back:", file=sys.stderr)
    print("\n".join(hits), file=sys.stderr)
    print("\n  The platform ABI has no unbounded condvar wait (issue 1196,\n"
          "  phase-436 D2). Use nros_platform_condvar_wait_until with a deadline\n"
          "  you own; a caller whose contract is unbounded loops over bounded\n"
          "  waits and re-tests its predicate (see `_z_condvar_wait` in\n"
          "  packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c).", file=sys.stderr)
    sys.exit(1)
print("check-no-unbounded-condvar-wait OK (the symbol is absent from the ABI).")
PY

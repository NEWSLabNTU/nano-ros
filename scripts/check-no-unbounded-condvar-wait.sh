#!/usr/bin/env bash
# Issue 1196 / phase-502 W4 — no code we own waits on a condition, or on a
# signal, without a deadline (RFC-0076 C6).
#
# History. phase-436 D2 deleted `nros_platform_condvar_wait` from the platform
# ABI, and this gate began as one symbol name keeping it deleted. That reach was
# narrower than the rule (issue 0196's shape): three forever-waits sat outside
# it, none spelled with that name — Zephyr's zenoh `_z_condvar_wait`
# (`pthread_cond_wait`), FreeRTOS's vendored one (`portMAX_DELAY`), and
# `zpico_get` calling `_z_condvar_wait` directly. phase-502 bounded or deleted
# all three and widened this gate to the CLASS.
#
# What this refuses, in every tracked Rust and C/C++ file outside
# `third-party/` (submodules are not tracked files, so zenoh-pico's own sources
# are not read; our patch to its FreeRTOS file lives on the fork's branch):
#   - a signalling wait whose timeout argument is the platform's FOREVER
#     (`K_FOREVER`, `TX_WAIT_FOREVER`, `portMAX_DELAY`), read across line breaks;
#   - a wait that has no timeout at all (`pthread_cond_wait`, `sem_wait`);
#   - any call of `_z_condvar_wait` — zenoh-pico's contract for it has no
#     deadline, so a caller of it is `zpico_get`'s shape;
#   - the retired ABI symbol `nros_platform_condvar_wait`, in any form, and a
#     Rust `fn condvar_wait(`.
#
# Exempt by RULE, not by row (RFC-0076 C6): mutex acquisition (the mutex calls
# are not in WAITS at all, and an `xSemaphoreTake` whose handle names a mutex is
# recognised), and thread join (a hit inside a function whose name ends in
# `join`). Anything else is a row in EXEMPT with a reason, and a row that no
# longer matches anything fails, so the table cannot outlive its subject.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

exec python3 - "$@" <<'PY'
import os, re, sys
sys.path.insert(0, os.path.join("scripts", "lib"))
import file_kinds, comments

# call name -> the token that makes its wait unbounded (None: no timeout
# argument exists, so every call is unbounded).
WAITS = {
    "pthread_cond_wait": None,
    "sem_wait": None,
    "_z_condvar_wait": None,
    "nros_platform_condvar_wait": None,
    "k_condvar_wait": "K_FOREVER",
    "k_sem_take": "K_FOREVER",
    "k_event_wait": "K_FOREVER",
    "k_event_wait_all": "K_FOREVER",
    "k_msgq_get": "K_FOREVER",
    "k_queue_get": "K_FOREVER",
    "k_fifo_get": "K_FOREVER",
    "k_lifo_get": "K_FOREVER",
    "k_poll": "K_FOREVER",
    "tx_semaphore_get": "TX_WAIT_FOREVER",
    "tx_event_flags_get": "TX_WAIT_FOREVER",
    "tx_queue_receive": "TX_WAIT_FOREVER",
    "xSemaphoreTake": "portMAX_DELAY",
    "xQueueReceive": "portMAX_DELAY",
    "xEventGroupWaitBits": "portMAX_DELAY",
    "ulTaskNotifyTake": "portMAX_DELAY",
    "xTaskNotifyWait": "portMAX_DELAY",
}
CALL = re.compile(r"\b(" + "|".join(map(re.escape, WAITS)) + r")\s*\(")
RS_FN = re.compile(r"\bfn\s+condvar_wait\s*\(")
# The mutex category, for the one wait call FreeRTOS shares between a mutex and
# a signalling semaphore: the handle's own name says which it is.
MUTEX_HANDLE = re.compile(r"(?i)mutex|mtx|lock")
# A C function definition starting a line; the last one before a hit encloses it.
C_FN = re.compile(r"^[A-Za-z_][\w \t\*]*?\b(\w+)\s*\([^;{}]*\)\s*\{", re.M)

# (path, call, reason). Every row must match at least one hit.
EXEMPT = [
    ("packages/rmw/zenoh/nros-rmw-zenoh/tests/condvar_shim_codes.rs", "_z_condvar_wait",
     "a host test that calls the alias shim to MEASURE its slice; the call is the subject"),
    ("tests/zephyr/thread_slot_release_test.c", "pthread_cond_wait",
     "host test harness: workers park on a gate the test itself opens; a hang is the test's own timeout"),
    ("packages/testing/nros-condvar-contract/src/lib.rs", "_z_condvar_wait",
     "the FreeRTOS image's check of the patched zenoh wait (phase-502 W2); the call is the subject"),
]


def args_of(code, open_paren):
    depth, i = 0, open_paren
    while i < len(code):
        c = code[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return code[open_paren + 1:i]
        i += 1
    return code[open_paren + 1:]


def is_definition(code, start):
    """`int f(` / `fn f(` / `z_result_t f(`: the text since the previous
    statement boundary is a bare type, with no operator, paren or `return`."""
    j = max(code.rfind(";", 0, start), code.rfind("{", 0, start), code.rfind("}", 0, start))
    prefix = "\n".join(l for l in code[j + 1:start].splitlines()
                        if not l.lstrip().startswith("#")).strip()
    if not prefix or not re.fullmatch(r"[\w\s\*:<>]+", prefix):
        return False
    return prefix.split()[-1] not in ("return", "else", "do")


def enclosing_fn(code, pos):
    name = None
    for m in C_FN.finditer(code, 0, pos):
        name = m.group(1)
    return name


def hits_in(text, lang):
    code = comments.strip_comments(text, lang, strings=True)
    lines = text.splitlines()
    out = []
    for m in CALL.finditer(code):
        name = m.group(1)
        if name != "nros_platform_condvar_wait" and is_definition(code, m.start()):
            continue
        forever = WAITS[name]
        args = args_of(code, m.end() - 1)
        if forever is not None and not re.search(r"\b" + forever + r"\b", args):
            continue
        if name == "xSemaphoreTake" and MUTEX_HANDLE.search(args.split(",")[0]):
            continue  # mutex category
        fn = enclosing_fn(code, m.start()) if lang == "c" else None
        if fn and fn.endswith("join"):
            continue  # join category
        ln = code.count("\n", 0, m.start())
        out.append((ln + 1, name, lines[ln].strip()))
    if lang == "rust":
        for m in RS_FN.finditer(code):
            ln = code.count("\n", 0, m.start())
            out.append((ln + 1, "fn condvar_wait", lines[ln].strip()))
    return sorted(out)


def self_test():
    c = (
        "int8_t nros_platform_condvar_wait(void *cv, void *m);\n"            # 1 retired symbol
        "z_result_t _z_condvar_wait(_z_condvar_t *cv, _z_mutex_t *m) {\n"     # 2 definition: ok
        "    return pthread_cond_wait(cv, m) == 0 ? 0 : -1;\n"                 # 3 no timeout
        "}\n"
        "int32_t zpico_get(void) {\n"
        "    while (!ctx.done) {\n"
        "        _z_condvar_wait(&ctx.cond, &ctx.mutex);\n"                    # 7 zpico_get shape
        "    }\n"
        "    xSemaphoreTake(cv->sem,\n"                                        # 9 multi-line
        "                   portMAX_DELAY);\n"
        "    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);\n"  # mutex: ok
        "    xSemaphoreTake(cv->sem, ticks);\n"                                # bounded: ok
        "    (void) k_sem_take(&s, K_FOREVER);\n"                              # 13
        "    k_sem_take(&s, K_MSEC(10));\n"                                    # bounded: ok
        "    tx_semaphore_get(&s, TX_WAIT_FOREVER);\n"                         # 15
        "    /* pthread_cond_wait(cv, m); */\n"                                # comment: ok
        "    k_mutex_lock(&m, K_FOREVER);\n"                                   # mutex: ok
        "}\n"
        "z_result_t _z_task_join(_z_task_t *task) {\n"
        "    tx_event_flags_get(&t->f, 1, TX_OR_CLEAR, &a, TX_WAIT_FOREVER);\n"  # join: ok
        "}\n"
        "#include \"x.h\"\n"
        "int8_t _z_condvar_wait(void *cv) {\n"                                           # definition after #include: ok
        "}\n"
        "void f(void) {\n"
        "    tx_event_flags_get(&t->f, 1, TX_OR_CLEAR, &a, TX_WAIT_FOREVER);\n"  # 23
        "    if (sem_wait(&s) != 0) return;\n"                                 # 24
        "}\n"
    )
    got = [l for l, _n, _t in hits_in(c, "c")]
    assert got == [1, 3, 7, 9, 13, 15, 26, 27], got
    rs = ("pub extern \"C\" fn nros_platform_condvar_wait(cv: *mut c_void) -> i8 { 0 }\n"
          "    fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;\n"
          "    fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, t: u64) -> i8;\n"
          "    fn _z_condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;\n"
          "    let rc = unsafe { _z_condvar_wait(cv, m) };\n"
          "    // fn condvar_wait(\n")
    got = [l for l, _n, _t in hits_in(rs, "rust")]
    assert got == [1, 2, 5], got
    file_kinds.self_test()


self_test()
files = file_kinds.files_of_kind("rust", "c-family")
print(f"check-no-unbounded-condvar-wait: examined {len(files)} Rust/C/C++ file(s)")
if not files:
    sys.exit("check-no-unbounded-condvar-wait: empty population")
used = set()
bad = []
for rel in files:
    try:
        text = open(rel, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    if not CALL.search(text) and "condvar_wait" not in text:
        continue
    lang = "rust" if rel.endswith(".rs") else "c"
    for line, name, src in hits_in(text, lang):
        row = next((i for i, (p, n, _r) in enumerate(EXEMPT) if p == rel and n == name), None)
        if row is not None:
            used.add(row)
            continue
        bad.append(f"  {rel}:{line}: {src}")
stale = [f"  {p} ({n})" for i, (p, n, _r) in enumerate(EXEMPT) if i not in used]
rc = 0
if bad:
    print("check-no-unbounded-condvar-wait: a wait with no deadline:", file=sys.stderr)
    print("\n".join(bad), file=sys.stderr)
    print("\n  Code we own never waits on a condition or a signal without a deadline\n"
          "  (RFC-0076 C6, issue 1196). Wait with a deadline; a caller whose contract\n"
          "  is unbounded loops over bounded waits and re-tests its predicate (see\n"
          "  `_z_condvar_wait` in zephyr/nros_zenoh_zephyr_system.c and the slice in\n"
          "  packages/rmw/zenoh/zpico-sys/c/zpico/nros_zp_condvar_slice.h). Mutex\n"
          "  acquisition and thread join are exempt by rule; anything else needs a\n"
          "  row in EXEMPT with a reason, argued in review.", file=sys.stderr)
    rc = 1
if stale:
    print("check-no-unbounded-condvar-wait: stale EXEMPT row(s), matching nothing:", file=sys.stderr)
    print("\n".join(stale), file=sys.stderr)
    rc = 1
if rc:
    sys.exit(rc)
print(f"check-no-unbounded-condvar-wait OK (no unbounded wait; {len(EXEMPT)} exempt row(s), all live).")
PY

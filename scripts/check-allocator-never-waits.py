#!/usr/bin/env python3
"""Issue 1717 — an allocator never waits.

`nros_platform_alloc` on ThreadX called
`tx_byte_allocate(pool, &p, size, TX_WAIT_FOREVER)`. A wait option on an
allocation means "if the pool cannot satisfy this, SUSPEND the caller until
another thread frees enough" — so an exhausted pool parked the calling thread
(typically the executor's) instead of returning NULL, and every
fallible-allocation path built on top of it (issue 1551's `try_box`, issue
1706's `try_box_uninit` and the parameter store's named refusal) was
unreachable on that port. The image went silent with no diagnostic. Measured
on the ThreadX linux port (`just threadx_linux test-c-port`, whose smoke now
exhausts its pool): before, the 14th 16 KiB request hung until `timeout`
killed it; after, it returns NULL in 0.25 s.

It was one site among eight `tx_byte_allocate` calls, and the seven others
already said `TX_NO_WAIT` — i.e. the rule was a convention that one site
missed, which is what a gate is for. THE RULE: every call of an RTOS
allocation primitive that TAKES a wait option passes that kernel's no-wait
spelling. The primitives are the ones with a timeout parameter:

    ThreadX   tx_byte_allocate / tx_block_allocate          TX_NO_WAIT
    NetX Duo  nx_packet_allocate                            NX_NO_WAIT
    Zephyr    k_heap_alloc / k_heap_aligned_alloc /
              k_heap_calloc / k_heap_realloc / k_mem_slab_alloc   K_NO_WAIT

`malloc`, `pvPortMalloc`, `kmm_malloc`, `k_malloc` and the rlsf arena take no
wait option and cannot block on exhaustion, so they are not in the table.

The ARGUMENT is checked, not the presence of a forever token anywhere in the
file: the same file legitimately waits forever on a mutex (`tx_mutex_get(…,
TX_WAIT_FOREVER)`), and a numeric timeout (`tx_byte_allocate(…, 100)`) is a
wait too. A spelling the gate cannot read as the no-wait token FAILS.

Population: every tracked C/C++ and Rust file (`file_kinds`, which excludes
vendored `third-party/` and `generated/`; submodule contents are never listed).
No exemptions table: none is needed today, and the first one has to be argued
for here, in the open.
"""

from __future__ import annotations

import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
import comments  # noqa: E402
import file_kinds  # noqa: E402

# primitive -> (0-based index of the wait argument, the no-wait spelling)
ALLOCATORS = {
    "tx_byte_allocate": (3, "TX_NO_WAIT"),
    "tx_block_allocate": (2, "TX_NO_WAIT"),
    "nx_packet_allocate": (3, "NX_NO_WAIT"),
    "k_heap_alloc": (2, "K_NO_WAIT"),
    "k_heap_aligned_alloc": (3, "K_NO_WAIT"),
    "k_heap_calloc": (3, "K_NO_WAIT"),
    "k_heap_realloc": (3, "K_NO_WAIT"),
    "k_mem_slab_alloc": (2, "K_NO_WAIT"),
}
CALL = re.compile(r"\b(" + "|".join(ALLOCATORS) + r")\s*\(")
# A definition or declaration: a type (or Rust `fn`) directly before the name.
DEF_PREFIX = re.compile(r"(?:\bfn\s+|^\s*(?:(?:static|inline|extern|const|unsigned|signed|UINT)\s+)*"
                        r"[A-Za-z_][\w:]*[\s*]+)$")


def split_args(code: str, open_paren: int):
    """Top-level arguments of the call whose `(` is at `open_paren`, or None
    if the parentheses never close (a truncated file — reported, not guessed)."""
    depth, start, args = 0, open_paren + 1, []
    for i in range(open_paren, len(code)):
        ch = code[i]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                args.append(code[start:i])
                return [a.strip() for a in args]
        elif ch == "," and depth == 1:
            args.append(code[start:i])
            start = i + 1
    return None


def violations(text: str, lang: str):
    """[(line, primitive, the wait argument as written)] for every call that
    does not pass its kernel's no-wait spelling."""
    code = comments.strip_comments(text, lang, strings=True)
    out = []
    for m in CALL.finditer(code):
        bol = code.rfind("\n", 0, m.start()) + 1
        prefix = code[bol:m.start()]
        if DEF_PREFIX.search(prefix) and prefix.strip() not in ("return", "else"):
            continue
        name = m.group(1)
        idx, token = ALLOCATORS[name]
        args = split_args(code, m.end() - 1)
        line = code.count("\n", 0, m.start()) + 1
        if args is None or len(args) <= idx:
            out.append((line, name, "<unreadable argument list>"))
            continue
        # A Rust FFI call may cast (`TX_NO_WAIT as ULONG`); the token is the subject.
        wait = re.sub(r"\s+as\s+\w+$", "", args[idx]).strip().strip("()").strip()
        if wait != token:
            out.append((line, name, args[idx]))
    return out


def self_test():
    c = (
        "UINT tx_byte_allocate(TX_BYTE_POOL *p, VOID **m, ULONG n, ULONG w);\n"   # 1 decl
        "x = tx_byte_allocate(pool, &p, (ULONG) size, TX_NO_WAIT);\n"              # 2 ok
        "x = tx_byte_allocate(pool, &p, (ULONG) size, TX_WAIT_FOREVER);\n"         # 3 BAD
        "x = tx_byte_allocate(pool, &p, f(a, b), 100);\n"                          # 4 BAD numeric
        "/* tx_byte_allocate(pool, &p, n, TX_WAIT_FOREVER); */\n"                  # 5 comment
        "s = nx_packet_allocate(pool, &pk, NX_RECEIVE_PACKET,\n"
        "                       NX_WAIT_FOREVER);\n"                               # 6 BAD, wrapped
        "v = k_heap_alloc(&h, 64, K_FOREVER);\n"                                   # 8 BAD
        "v = k_heap_alloc(&h, 64, K_NO_WAIT);\n"                                   # 9 ok
        "if (tx_block_allocate(&bp, &b, TX_NO_WAIT) != TX_SUCCESS) {}\n"           # 10 ok
        "r = k_mem_slab_alloc(&s, &b, K_MSEC(5));\n"                               # 11 BAD timed
        "x = tx_byte_allocate(pool, &p,\n"                                         # 12 BAD truncated
    )
    got = [(line, name) for line, name, _w in violations(c, "c")]
    want = [(3, "tx_byte_allocate"), (4, "tx_byte_allocate"), (6, "nx_packet_allocate"),
            (8, "k_heap_alloc"), (11, "k_mem_slab_alloc"), (12, "tx_byte_allocate")]
    assert got == want, got
    rs = (
        "unsafe extern \"C\" { fn tx_byte_allocate(p: *mut c_void, m: *mut *mut c_void, n: u32, w: u32) -> u32; }\n"
        "let rc = unsafe { tx_byte_allocate(pool, &mut p, n, TX_NO_WAIT as u32) };\n"
        "let rc = unsafe { tx_byte_allocate(pool, &mut p, n, TX_WAIT_FOREVER) };\n"
    )
    assert [line for line, _n, _w in violations(rs, "rust")] == [3], violations(rs, "rust")
    file_kinds.self_test()


def main() -> int:
    self_test()
    files = file_kinds.files_of_kind("rust", "c-family")
    print(f"check-allocator-never-waits: examined {len(files)} Rust/C/C++ file(s)")
    if not files:
        print("check-allocator-never-waits: empty population", file=sys.stderr)
        return 1
    hits, calls = [], 0
    for rel in files:
        try:
            text = open(rel, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        if not CALL.search(text):
            continue
        lang = "rust" if rel.endswith(".rs") else "c"
        calls += len(CALL.findall(comments.strip_comments(text, lang, strings=True)))
        for line, name, wait in violations(text, lang):
            idx, token = ALLOCATORS[name]
            hits.append(f"  {rel}:{line}: {name}(…) wait argument is `{wait}`, not `{token}`")
    if hits:
        print("check-allocator-never-waits: an allocation primitive is given a WAIT option:",
              file=sys.stderr)
        print("\n".join(hits), file=sys.stderr)
        print("\n  An exhausted pool must answer NULL, not suspend the caller until another\n"
              "  thread frees memory: a waiting allocator makes every fallible-allocation\n"
              "  path (try_box, the parameter store's refusal) unreachable and turns heap\n"
              "  exhaustion into a silent hang. Pass the no-wait token. See issue 1717.",
              file=sys.stderr)
        return 1
    if calls == 0:
        # The population scan found no allocator call at all: the rule would
        # pass vacuously, which is what a broken population looks like.
        print("check-allocator-never-waits: found no allocation primitive call anywhere — "
              "the population is wrong, not the tree clean", file=sys.stderr)
        return 1
    print(f"check-allocator-never-waits OK ({calls} allocation call(s), all no-wait).")
    return 0


if __name__ == "__main__":
    sys.exit(main())

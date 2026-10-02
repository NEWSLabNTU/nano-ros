#!/usr/bin/env python3
"""The RTOS kernel-allocator API, as a SHAPE per kernel — issue 1616 (W7).

Two gates ask "does this reach a kernel allocator directly?":
`check-no-direct-kernel-alloc.sh` over source, `check-no-alloc-image.py` over a
linked image. Each carried its OWN authored name list, and each list stopped
short of the API: the source gate knew `k_malloc`/`k_free` but not `k_calloc`,
the image gate knew `k_calloc` but not `k_realloc`. A list of names is the
population going stale in the quiet direction (phase-472 W7).

So the population is the kernel's allocator NAMING SCHEME, one regex per
kernel, and both gates read it from here:

* Zephyr   `k_malloc` `k_calloc` `k_realloc` `k_free` `k_aligned_alloc`,
           `k_heap_*alloc` / `k_heap_free` / `k_heap_realloc`, `sys_heap_*`
* FreeRTOS `pvPortMalloc` `pvPortCalloc` `vPortFree` (+ `Stack` variants)
* ThreadX  `tx_byte_allocate` / `tx_byte_release` (+ `_tx_`, `_txe_`)
* NuttX    `kmm_malloc` `kmm_calloc` `kmm_realloc` `kmm_zalloc` `kmm_memalign` `kmm_free`
* ESP-IDF  `heap_caps_*malloc|calloc|realloc|free`

`python3 scripts/lib/kernel_alloc.py --ere` prints a grep -E pattern for shell.
"""
from __future__ import annotations

import re
import sys

_ALT = (
    r"k_(?:heap_)?(?:aligned_)?(?:malloc|calloc|realloc|alloc|free)",
    r"sys_heap_(?:aligned_)?(?:alloc|realloc|free)",
    r"pvPort(?:Malloc|Calloc)(?:Stack)?",
    r"vPortFree(?:Stack)?",
    r"_?txe?_byte_(?:allocate|release)",
    r"kmm_(?:malloc|calloc|realloc|zalloc|memalign|free)",
    r"heap_caps_(?:aligned_)?(?:malloc|calloc|realloc|free|alloc)(?:_prefer)?",
)
NAME_RE = re.compile(r"^(?:%s)$" % "|".join(_ALT))
SOURCE_RE = re.compile(r"\b(?:%s)\b" % "|".join(_ALT))


def is_kernel_alloc(name: str) -> bool:
    return bool(NAME_RE.match(name))


def ere() -> str:
    """The same alternation for `grep -E` (no non-capturing groups there)."""
    return r"\b(" + "|".join(a.replace("(?:", "(") for a in _ALT) + r")\b"


def self_test() -> None:
    for yes in ("k_malloc", "k_calloc", "k_realloc", "k_free", "k_aligned_alloc",
                "k_heap_alloc", "k_heap_aligned_alloc", "k_heap_free", "sys_heap_alloc",
                "pvPortMalloc", "pvPortCalloc", "vPortFree", "pvPortMallocStack",
                "tx_byte_allocate", "_tx_byte_release", "_txe_byte_allocate",
                "kmm_zalloc", "heap_caps_realloc", "heap_caps_malloc"):
        assert is_kernel_alloc(yes), yes
    for no in ("k_sleep", "task_free", "zsock_freeaddrinfo", "malloc", "k_mutex_lock",
               "nros_platform_alloc", "tx_thread_create"):
        assert not is_kernel_alloc(no), no
    assert SOURCE_RE.search("#define X(n) pvPortMalloc(n)")
    assert not SOURCE_RE.search("my_k_malloc_wrapper(")
    r = re.compile(ere().replace(r"\b", r"\b"))
    assert r.search("return k_realloc(p, 8);") and not r.search("k_sleep(1)")


if __name__ == "__main__":
    self_test()
    if "--ere" in sys.argv:
        print(ere())
    else:
        print("kernel_alloc self-test: OK")

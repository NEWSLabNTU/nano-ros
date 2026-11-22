#!/usr/bin/env python3
"""check-ffi-executor-derive — every way the C and C++ APIs reach an executor
is one a reader can name (phase-573 W3, RFC-0108 D9).

A C or C++ handle reaches its executor through a stored pointer. The two
crates each have ONE derive that takes the executor's lock before a reference
exists (`get_executor` / `get_executor_from_ptr` in nros-c, `cpp_ctx_checked`
/ `cpp_ctx_locked` in nros-cpp), and ONE refusal for a call that must not run
inside a callback (`refuse_in_dispatch` in nros-c; `RawDispatchGuard` and
`CtxRef::refuse_in_dispatch` in nros-cpp, which reaches the flag through
`in_dispatch_ptr`). Phase-573 W3 measured nine entries
that cast the pointer themselves and took no lock: a cast is the shape of the
defect, so this gate holds the casts.

Three rules, over non-test code in `packages/api/nros-{c,cpp}/src`:

1. A cast to the executor itself (`CExecutor`, `CppContext`, `CppExecutor`)
   is allowed only in a function on `EXECUTOR_CASTS`, each with its reason.
2. A cast to the C executor HEADER (`rclc_executor_t`) from a stored pointer
   is allowed only in a function on `HEADER_CASTS`. A body-scoped header
   reference is sound (RFC-0108 D9 rule 3); a spin that kept one across its
   callbacks would not be, and a new spinning entry is a new function, so it
   lands here first.
3. The `in_dispatch` flag is read or written only in a function on
   `FLAG_ACCESS`: a call refuses through the one refusal, not by reading the
   flag.

Every spelling counts: `as *mut T`, `as *const T`, `.cast::<T>()` and
`ptr::from_mut::<T>` / `ptr::from_ref::<T>` (the issue-0196 reach rule: a gate
that matched one spelling would pass the next). An allowlist entry that no
longer matches anything fails too, so the lists cannot outlive the code.

    python3 scripts/check-ffi-executor-derive.py              # check
    python3 scripts/check-ffi-executor-derive.py --self-test  # negative controls
    python3 scripts/check-ffi-executor-derive.py --list       # every site, by function
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATES = ("packages/api/nros-c/src", "packages/api/nros-cpp/src")

EXEC_TYPES = r"(?:CExecutor|CppContext|CppExecutor|Executor(?:<[^>]*>)?|ExecutorInlineStorage)"
HEADER_TYPE = r"rclc_executor_t"


def cast_patterns(types: str) -> list[re.Pattern]:
    t = r"(?:[\w:]+::)?" + types + r"\b"
    return [
        re.compile(r"\bas\s*\*\s*(?:mut|const)\s+" + t),
        re.compile(r"\.cast(?:_mut|_const)?::<\s*" + t + r"\s*>"),
        re.compile(r"\bptr::from_(?:mut|ref)::<\s*" + t + r"\s*>"),
        # An inferred cast: `let e: *mut T = p.cast();` / `= p as _;`.
        re.compile(r":\s*\*\s*(?:mut|const)\s+" + t + r"(?:<[^>]*>)?\s*=\s*[^;]*(?:\.cast\w*\(\)|\bas\s+_|transmute)"),
    ]


EXEC_CASTS_RE = cast_patterns(EXEC_TYPES)
HEADER_CASTS_RE = cast_patterns(HEADER_TYPE)
FLAG_RE = re.compile(r"\bin_dispatch\b")

# (crate-relative file, function) -> reason. Keep each reason to what makes
# the cast sound without the shared derive.
EXECUTOR_CASTS = {
    ("nros-c/src/executor.rs", "get_executor"): "the derive itself",
    ("nros-c/src/executor.rs", "get_executor_from_ptr"): "the derive itself",
    ("nros-c/src/executor.rs", "executor_init_on_support"): "writes the executor in place at init",
    ("nros-c/src/executor.rs", "rclc_executor_fini"): "drops the executor in place at fini",
    ("nros-c/src/executor.rs", "host_executor_init"): "returns the inline storage it just wrote, at init",
    ("nros-cpp/src/lib.rs", "inline_backing"): "the inline storage's backing slice, at init",
    ("nros-cpp/src/lib.rs", "cpp_ctx_checked"): "the derive itself",
    ("nros-cpp/src/lib.rs", "cpp_ctx_locked"): "the derive itself",
    ("nros-cpp/src/lib.rs", "write_context"): "writes the context in place at init",
    ("nros-cpp/src/lib.rs", "nros_cpp_executor_inner"): "hands out the executor pointer itself",
    ("nros-cpp/src/lib.rs", "nros_app_register_backends"): "boot, before a second thread exists",
    ("nros-cpp/src/lib.rs", "nros_cpp_context_is_live"): "reads the tag only",
    ("nros-cpp/src/lib.rs", "nros_cpp_fini"): "drops the context in place at fini",
    ("nros-cpp/src/lib.rs", "nros_cpp_spin_once"): "tag check, then the lock through a raw read",
    ("nros-cpp/src/lib.rs", "nros_cpp_spin_some"): "tag check, then the lock through a raw read",
    ("nros-cpp/src/lib.rs", "nros_cpp_entry_spin_budget_ms"): "tag check, then a lock-free read",
    ("nros-cpp/src/lib.rs", "open_over_session"): "writes a sched task's context at open",
    ("nros-cpp/src/lib.rs", "nros_cpp_level_spin_once"): "the scheduler owner's context, tag-checked",
    ("nros-cpp/src/lib.rs", "nros_board_native_run_sched_tasks_in"): "boot, reads the owner",
    ("nros-cpp/src/lib.rs", "native_sched_task_trampoline"): "a sched task's own context",
    ("nros-cpp/src/lib.rs", "native_wake_all_levels"): "tag check, then a lock-free wake",
}

HEADER_CASTS = {
    ("nros-c/src/event.rs", "subscription_set"): "body-scoped header read",
    ("nros-c/src/executor.rs", "bind_host_node"): "body-scoped header read",
    ("nros-c/src/executor.rs", "detach_client"): "body-scoped, after the refusal",
    ("nros-c/src/executor.rs", "detach_service"): "body-scoped, after the refusal",
    ("nros-c/src/executor.rs", "detach_subscription"): "body-scoped, after the refusal",
    ("nros-c/src/executor.rs", "executor_of_opaque"): "the opaque pointer's inverse",
    ("nros-c/src/executor.rs", "rclc_executor_node_init"): "body-scoped header read",
    ("nros-c/src/executor.rs", "trigger_bridge"): "body-scoped header read",
    ("nros-c/src/guard_condition.rs", "rclc_node_create_guard_condition"): "body-scoped header read",
    ("nros-c/src/guard_condition.rs", "rclc_node_get_graph_guard_condition"): "body-scoped header read",
    ("nros-c/src/node.rs", "rclc_node_resolve_name"): "body-scoped header read",
    ("nros-c/src/node.rs", "resolve_entity_name_on_node"): "body-scoped header read",
    ("nros-c/src/node.rs", "resolve_session_and_domain"): "body-scoped header read",
    ("nros-c/src/publisher.rs", "rclc_publisher_init_with_qos"): "body-scoped header read, released before the session lock",
    ("nros-c/src/service.rs", "blocking_response_cb"): "body-scoped header read",
    ("nros-c/src/service.rs", "client_entry_callback"): "body-scoped header read",
    ("nros-c/src/service.rs", "client_granted_qos"): "body-scoped header read",
    ("nros-c/src/service.rs", "rcl_client_fini"): "body-scoped header read",
    ("nros-c/src/service.rs", "rcl_service_fini"): "body-scoped header read",
    ("nros-c/src/service.rs", "rcl_service_server_is_available"): "body-scoped header read",
    ("nros-c/src/service.rs", "rclc_client_call"): "after the refusal; spins through the raw pointer",
    ("nros-c/src/service.rs", "rclc_client_server_available"): "body-scoped header read",
    ("nros-c/src/service.rs", "rclc_client_service_is_ready"): "body-scoped header read",
    ("nros-c/src/service.rs", "rclc_client_take_response"): "body-scoped header read",
    ("nros-c/src/service.rs", "rclc_client_wait_for_service"): "after the refusal; spins through the raw pointer",
    ("nros-c/src/service.rs", "send_request_async_inner"): "body-scoped header read",
    ("nros-c/src/service.rs", "service_granted_qos"): "body-scoped header read",
    ("nros-c/src/subscription.rs", "rcl_subscription_fini"): "body-scoped header read",
    ("nros-c/src/subscription.rs", "rclc_subscription_get_actual_qos"): "body-scoped header read",
    ("nros-cpp/src/metadata_hooks.rs", "a_c_component_gets_the_hosts_executor_and_node"): "census, before any spin",
}

FLAG_ACCESS = {
    ("nros-c/src/executor.rs", "refuse_in_dispatch"): "the refusal",
    ("nros-c/src/executor.rs", "host_executor_init"): "init writes it false",
    ("nros-c/src/executor.rs", "rclc_executor_spin_some"): "the spin sets and clears it",
    ("nros-cpp/src/lib.rs", "in_dispatch_ptr"): "the flag's one place",
    ("nros-cpp/src/lib.rs", "nros_cpp_spin_once"): "its RawDispatchGuard's flag",
    ("nros-cpp/src/lib.rs", "nros_cpp_spin_some"): "its RawDispatchGuard's flag",
}

FN_RE = re.compile(
    r"^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:const\s+)?(?:unsafe\s+)?"
    r"(?:extern\s+\"C\"\s+)?fn\s+(\w+)"
)
TEST_ATTR = re.compile(r"^\s*#\[cfg\((?:all\()?(?:test|kani)\b")
MOD_OPEN = re.compile(r"^\s*(?:pub(?:\([\w:]+\))?\s+)?mod\s+\w+\s*\{")


def strip_comment(line: str) -> str:
    # Good enough for this tree: no `//` inside the string literals that
    # carry a cast.
    i = line.find("//")
    return line if i < 0 else line[:i]


def scan_text(text: str):
    """Yield (line, function, rule, matched) for every site outside test mods."""
    cur = "?"
    depth = 0
    skip_until = None
    pending_test = False
    for n, raw in enumerate(text.splitlines(), 1):
        code = strip_comment(raw)
        stripped = code.strip()
        if TEST_ATTR.match(raw):
            pending_test = True
        elif pending_test and skip_until is None and MOD_OPEN.match(raw):
            skip_until = depth
            pending_test = False
        elif pending_test and stripped and not stripped.startswith("#["):
            pending_test = False
        if skip_until is None:
            m = FN_RE.match(raw)
            if m:
                cur = m.group(1)
            for p in EXEC_CASTS_RE:
                for mm in p.finditer(code):
                    yield n, cur, "executor", mm.group(0)
            for p in HEADER_CASTS_RE:
                for mm in p.finditer(code):
                    yield n, cur, "header", mm.group(0)
            for mm in FLAG_RE.finditer(code):
                # A struct field declaration and a struct literal are the
                # flag's definition, not an access.
                if re.match(r"\s*(?:pub\s+)?in_dispatch\s*:", code):
                    continue
                yield n, cur, "flag", mm.group(0)
        depth += code.count("{") - code.count("}")
        if skip_until is not None and depth <= skip_until:
            skip_until = None


RULES = {
    "executor": (EXECUTOR_CASTS, "casts to the executor; take it through the crate's derive "
                 "(`get_executor` / `get_executor_from_ptr`, `cpp_ctx_checked` / `cpp_ctx_locked`)"),
    "header": (HEADER_CASTS, "casts a stored pointer to the C executor header; if it is body-scoped, "
               "add it to HEADER_CASTS with its reason, and never hold it across a spin"),
    "flag": (FLAG_ACCESS, "reads or writes `in_dispatch`; refuse through `refuse_in_dispatch` "
             "(nros-c) or `RawDispatchGuard` / `CtxRef::refuse_in_dispatch` (nros-cpp)"),
}


def rust_files(root: str) -> list[str]:
    """The tracked `.rs` files of both crates; the self-test's tree is not a
    repository, so it lists its own plants."""
    if root == ROOT:
        out = subprocess.run(
            ["git", "-C", root, "ls-files", "--", *(f"{c}/*.rs" for c in CRATES)],
            check=True, capture_output=True, text=True).stdout.split()
        return sorted(out)
    return sorted(
        f"{c}/{f}"
        for c in CRATES for f in os.listdir(os.path.join(root, c)) if f.endswith(".rs"))


TEST_MOD_RE = re.compile(
    r"#\[cfg\(((?:[^()]|\([^()]*\))*)\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;")


def cfg_is_test_only(pred: str) -> bool:
    """True when the cfg compiles only under test or kani: `test`, `kani`, or
    an `all(..)` with one of them as a top-level term. `any(test, feature =
    "std")` is compiled into production with that feature, so it is not."""
    pred = pred.strip()
    if pred in ("test", "kani"):
        return True
    if pred.startswith("all(") and pred.endswith(")"):
        terms, depth, cur = [], 0, ""
        for ch in pred[4:-1]:
            if ch == "," and depth == 0:
                terms.append(cur.strip())
                cur = ""
                continue
            depth += ch == "("
            depth -= ch == ")"
            cur += ch
        terms.append(cur.strip())
        return any(t in ("test", "kani") for t in terms)
    return False


def test_only_files(root: str, files: list[str]) -> set[str]:
    """Files that are whole test modules: `#[cfg(test)] mod name;` in a
    sibling file names `name.rs`. Their bodies are skipped like an inline
    `#[cfg(test)] mod`."""
    skip = set()
    for path in files:
        with open(os.path.join(root, path), encoding="utf8") as fh:
            for pred, name in TEST_MOD_RE.findall(fh.read()):
                if cfg_is_test_only(pred):
                    skip.add(os.path.join(os.path.dirname(path), name + ".rs"))
    return skip


def collect(root: str):
    sites = []
    files = rust_files(root)
    skip = test_only_files(root, files)
    for path in files:
        if path in skip:
            continue
        rel = os.path.relpath(path, "packages/api")
        with open(os.path.join(root, path), encoding="utf8") as fh:
            for n, fn, rule, text in scan_text(fh.read()):
                sites.append((rel, n, fn, rule, text))
    return sites


def check(root: str, quiet: bool = False) -> int:
    sites = collect(root)
    bad = []
    used = {rule: set() for rule in RULES}
    for rel, n, fn, rule, text in sites:
        allow, _ = RULES[rule]
        if (rel, fn) in allow:
            used[rule].add((rel, fn))
        else:
            bad.append((rel, n, fn, rule, text))
    stale = [(rule, key) for rule, (allow, _) in RULES.items() for key in allow
             if key not in used[rule]]
    if not bad and not stale:
        if not quiet:
            print(f"check-ffi-executor-derive: OK ({len(sites)} sites, every one in a named function)")
        return 0
    for rel, n, fn, rule, text in bad:
        print(f"check-ffi-executor-derive: packages/api/{rel}:{n} (fn {fn}) {RULES[rule][1]}: `{text}`")
    for rule, (rel, fn) in stale:
        print(f"check-ffi-executor-derive: the {rule} list names {rel} fn {fn}, which no longer "
              "has one; remove the entry")
    return 1


PLANTS = {
    "executor `as`": "let e = &mut *(p as *mut CExecutor);",
    "executor `.cast`": "let e = p.cast::<CppContext>();",
    "executor `from_mut`": "let e = core::ptr::from_mut::<CExecutor>(x);",
    "header `as`": "let h = &mut *(p as *mut rclc_executor_t);",
    "flag read": "if h.in_dispatch { return 1; }",
    "executor by its own name": "let e = &mut *(p as *mut nros_node::Executor<'static>);",
    "inferred `.cast`": "let e: *mut CppContext = p.cast();",
}


def self_test() -> int:
    failures = 0
    for pred, want in (("test", True), ("all(test, feature = \"rmw-cffi\", unix)", True),
                       ("kani", True), ("any(test, feature = \"std\")", False),
                       ("feature = \"test\"", False), ("not(test)", False)):
        if cfg_is_test_only(pred) != want:
            print(f"check-ffi-executor-derive self-test: cfg({pred}) read as "
                  f"{'test-only' if not want else 'production'}", file=sys.stderr)
            failures += 1
    with tempfile.TemporaryDirectory() as tmp:
        for name, line in PLANTS.items():
            for crate in CRATES:
                os.makedirs(os.path.join(tmp, crate), exist_ok=True)
            path = os.path.join(tmp, CRATES[0], "planted.rs")
            with open(path, "w", encoding="utf8") as fh:
                fh.write(f"pub unsafe fn planted() {{\n    {line}\n}}\n"
                         "#[cfg(test)]\nmod tests {\n    fn t() { " + line + " }\n}\n")
            found = [s for s in collect(tmp) if s[2] == "planted"]
            in_test = [s for s in collect(tmp) if s[2] == "t"]
            if len(found) != 1 or in_test:
                print(f"self-test FAIL: {name}: found {found}, in test {in_test}")
                failures += 1
            os.remove(path)
    if failures:
        return 1
    return 0


def main(argv) -> int:
    if "--self-test" in argv:
        rc = self_test()
        if rc == 0:
            print(f"check-ffi-executor-derive self-test: OK ({len(PLANTS)} plants)")
        return rc
    if "--list" in argv:
        for rel, n, fn, rule, text in collect(ROOT):
            print(f"{rule:8} packages/api/{rel}:{n} {fn} `{text}`")
        return 0
    rc = self_test()
    if rc:
        return rc
    return check(ROOT)


if __name__ == "__main__":
    sys.exit(main(sys.argv))

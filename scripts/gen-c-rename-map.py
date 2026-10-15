#!/usr/bin/env python3
"""phase-492 W1 — the C API rename map: every public `nros_` / `NROS_` name the
`nros-c` headers declare, and what it becomes.

RFC-0089 amendment 2026-10-12 ("`nros_` is phased out of the C API; ours-only
names take `rclc_`"). The rule, per name:

  adopt    upstream has the name and the contract conforms (`same` /
           `systematic` in `scripts/api-parity.py --lang c`) -> upstream's
           exact spelling.
  alias    the upstream spelling already names it through a compat alias
           (`typedef struct nros_node_t rcl_node_t;`) -> the definition takes
           the upstream name and the alias goes.
  ours     no upstream counterpart, or one whose contract differs (`differs`,
           `ours-only`) -> `rclc_` + the stem (`RCLC_` for constants). A
           `differs` row keeps a DISTINCT name so a ported call fails to
           compile rather than binding to a different contract.
  ret      `NROS_RET_<X>` -> `RCL_RET_<X>` where rcl names that code, else
           `RCLC_RET_<X>`; values stay ours (the 2026-09-04 ruling).

and what is OUT of scope, by class:

  seam     defined by a port seam's header (RMW vtable, platform, board) or a
           C++ FFI name (`nros_cpp_*`): written against nano-ros, not ported.
  hook     a function the APPLICATION or the generated entry defines and the
           runtime calls (`nros_app_main`, `nros_app_register_backends`): a
           link-time contract with board startup code, not an API call.
  build    a build or configuration fact (knobs, generated sizes, codegen
           version, entry config, visibility, include guards): RFC-0103's
           one-owner-per-fact names, not API.

Writes `docs/reference/c-api-rename-map.tsv` (old, new, class, why). It is
REVIEWED and committed; `scripts/check-c-api-nros-names.py` reads it.

Run: python3 scripts/gen-c-rename-map.py [--check]
"""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "reference" / "c-api-rename-map.tsv"
C_INCLUDE = ROOT / "packages" / "api" / "nros-c" / "include"
SEAM_HEADERS = [
    ROOT / "packages" / "core" / "nros-rmw-abi" / "include",
    ROOT / "packages" / "platform" / "nros-platform-api" / "include",
    ROOT / "packages" / "boards" / "nros-board-cffi" / "include",
]
TOKEN = re.compile(r"\b(nros_[a-z0-9_]+|NROS_[A-Z0-9_]+)\b")

# Reviewed decisions the rule above cannot make, with the reason each needs one.
OVERRIDES = {
    # `differs` rows whose `rclc_` spelling IS upstream's: rclc's lifecycle
    # verbs take an `rclc_lifecycle_node_t*` and a context-less `int (*)(void)`
    # callback, ours a state machine and a context-carrying callback. Taking
    # rclc's name would compile a ported call against a different contract.
    "nros_lifecycle_change_state": "rclc_lifecycle_state_machine_change_state",
    "nros_lifecycle_register_on_activate": "rclc_lifecycle_state_machine_register_on_activate",
    "nros_lifecycle_register_on_cleanup": "rclc_lifecycle_state_machine_register_on_cleanup",
    "nros_lifecycle_register_on_configure": "rclc_lifecycle_state_machine_register_on_configure",
    "nros_lifecycle_register_on_deactivate": "rclc_lifecycle_state_machine_register_on_deactivate",
    # phase-492 W1.b — parity's `systematic` rows are a SPELLING match, not a
    # contract match, and W1 adopted them all; these took upstream's name over
    # a different contract. `check-c-api-upstream-signatures` measures that now.
    # Each value is (new, class, why).
    "nros_timer_init": ("rclc_timer_init_with_context", "ours", "differs from `rcl_timer_init`: our callback gets the caller's context (rcl_compat.h §4) (phase-492 W1.b)"),
    "nros_timer_callback_t": ("rclc_timer_callback_with_context_t", "ours", "differs from `rcl_timer_callback_t`: context, not last_call_time (phase-492 W1.b)"),
    "nros_clock_get_now": ("rclc_clock_get_now_time", "ours", "differs from `rcl_clock_get_now`: writes an rclc_time_t (phase-492 W1.b)"),
    "nros_clock_get_now_ns": ("rcl_clock_get_now", "adopt", "conforms to upstream `rcl_clock_get_now` (measured) (phase-492 W1.b)"),
    "nros_node_resolve_name": ("rclc_node_resolve_name", "ours", "differs from `rcl_node_resolve_name`: caller buffer, no allocator (phase-492 W1.b)"),
    "nros_action_server_init": ("rclc_action_server_init_with_callbacks", "ours", "differs from `rcl_action_server_init`: binds the callbacks (phase-492 W1.b)"),
    "nros_action_publish_feedback": ("rclc_action_publish_feedback_raw", "ours", "differs from `rclc_action_publish_feedback`: CDR bytes (phase-492 W1.b)"),
    "nros_action_server_goal_exists": ("rclc_action_server_goal_exists", "ours", "differs from `rcl_action_server_goal_exists`: our goal handle (phase-492 W1.b)"),
    "nros_subscription_callback_t": ("rclc_subscription_raw_callback_t", "ours", "differs from `rclc_subscription_callback_t`: CDR bytes + context (phase-492 W1.b)"),
    "nros_service_callback_t": ("rclc_service_raw_callback_t", "ours", "differs from `rclc_service_callback_t`: CDR bytes + context (phase-492 W1.b)"),
    "nros_executor_add_client": ("rclc_executor_attach_client", "ours", "differs from `rclc_executor_add_client`: no message/callback args (phase-492 W1.b)"),
    "nros_executor_add_action_client": ("rclc_executor_attach_action_client", "ours", "differs from `rclc_executor_add_action_client`: callbacks bound at init (phase-492 W1.b)"),
    "nros_executor_add_action_server": ("rclc_executor_attach_action_server", "ours", "differs from `rclc_executor_add_action_server`: callbacks bound at init (phase-492 W1.b)"),
    "nros_support_init": ("rclc_support_init_with_locator", "ours", "the (locator, domain) form; `rclc_support_init` now takes rclc's arguments (phase-492 W1.b)"),
    "nros_typed_subscription_callback_t": ("rclc_subscription_callback_with_context_t", "adopt", "identical to rclc's (the alias is gone) (phase-492 W1.b)"),
}

# phase-492 W1.b — `systematic` rows whose contract was MADE to conform
# (rcl's arity and types), so they adopt upstream's name on a measured basis.
SIGNATURE_CONFORMED = {
    "nros_publisher_fini", "nros_subscription_fini", "nros_service_fini", "nros_client_fini",
    "nros_action_client_fini", "nros_action_server_fini", "nros_clock_init",
    "nros_executor_init", "nros_action_client_init", "nros_node_get_domain_id",
    "nros_timer_get_time_since_last_call", "nros_executor_remove_client",
    "nros_executor_remove_service", "nros_executor_remove_subscription",
    "nros_executor_remove_timer",
}

# Headers that hold the ENTRY / BOOT conventions: how an image starts, which
# board startup code and the generated entry implement. Names declared only
# here are a link-time contract with startup code, like the hooks below, not a
# ROS API, so they keep their spelling.
ENTRY_HEADERS = {
    "app_main.h", "app_config.h", "boot_config.h", "entry_config.h", "main.h",
    "tier_task_memory_freertos.h", "tier_task_memory_zephyr.h",
}

# phase-492 W1.b — reached only through a `<rclc/detail/baked_rmw.h>` macro, which
# forwards the build's baked RMW selector (`NROS_ENTRY_RMW`). A build fact's
# carrier, not an API call.
BAKED_SEAMS = {"nros_c_support_init_baked"}

HOOKS = {
    "nros_app_main",
    "nros_app_register_backends",
    "nros_app_exit_code",
    "nros_app_main_returned",
}

BUILD_UPPER = re.compile(
    r"^NROS_("
    r"_.*"  # NROS__ internal helpers
    r"|.*_H_?"  # include guards
    r"|MAX_.*|HAS_.*|CODEGEN_.*|EMITTED_CODEGEN_.*|CONFIG_.*|ENTRY_.*"
    r"|DECLARED_.*|ASSERT_DECLARED_.*|FEATURES|RMW|RMW_CYCLONEDDS|PUBLIC|LOCAL"
    r"|BUILDING_DLL|USING_DLL|SERIALIZATION_FORMAT.*|MSG_FORMAT_ID_.*"
    r"|ASSERT_MESSAGE_FORMAT|PLATFORM_.*|HOST_.*|NUTTX_.*|SAFETY_E2E|PARAM_SERVICES"
    r"|EXECUTOR_MAX_.*|EXECUTOR_ARENA_SIZE|LOG_BUFFER_SIZE|LOG_FMT_BUFFER_SIZE"
    r"|LOG_MAX_LEVEL|LOG_DYNAMIC_LOGGERS|TIER_TASK_.*|ZEPHYR_.*|CPP_.*"
    r"|C_[A-Z_]*_STORAGE_SIZE|.*_SIZE|.*_U64S|BOOT_CONFIG_MAGIC|BOOT_CONFIG_VERSION"
    r"|TRANSPORT_OPS_ABI_.*|NO_DEPRECATED_.*|PKG_NAME|NODE_NAME|NODE_NAMESPACE"
    r"|LOCATOR|STATIC_ASSERT.*|WARN_UNUSED|RCL_COMPAT_ASSERT|PUBLISHER_GID_SIZE"
    r"|DOMAIN_ID_.*"
    r")$"
)


def load_parity():
    sys.path.insert(0, str(ROOT / "scripts"))
    spec = importlib.util.spec_from_file_location("ap", ROOT / "scripts" / "api-parity.py")
    assert spec is not None and spec.loader is not None
    ap = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ap)
    with tempfile.TemporaryDirectory() as td:
        rows, _, _ = ap.run_lang("c", td)
    by_ours = {}
    for r in rows:
        o = r.get("ours")
        if not o:
            continue
        t = r.get("theirs")
        by_ours[o["qual"]] = (r["bucket"], t["qual"] if t else None)
    theirs_names = {r["theirs"]["qual"] for r in rows if r.get("theirs")}
    return by_ours, theirs_names


COMMENT = re.compile(r"/\*.*?\*/|//[^\n]*", re.S)
STRING = re.compile(r'"(?:\\.|[^"\\])*"')


def code_of(text):
    """The header with comments and string literals blanked: a name a comment
    MENTIONS (a retired spelling, a cross-reference) is not one it declares."""
    return STRING.sub('""', COMMENT.sub(" ", text))


def tracked_headers(base):
    """The tracked `.h` files under `base` -- an index lookup, not a walk."""
    rel = Path(base).relative_to(ROOT)
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--", f"{rel}/*.h", f"{rel}/**/*.h"],
        capture_output=True,
        check=True,
    ).stdout
    return sorted({ROOT / p.decode() for p in out.split(b"\0") if p})


def tokens_in(paths):
    out = set()
    for base in paths:
        for f in tracked_headers(base):
            out |= set(TOKEN.findall(code_of(f.read_text(errors="replace"))))
    return out


def defined_upstream_names_in_nros_c():
    """Upstream-spelled names nros-c already DEFINES (the twins' targets)."""
    names = set()
    for f in tracked_headers(C_INCLUDE):
        text = code_of(f.read_text(errors="replace"))
        names |= set(re.findall(r"\b(rclc?_[a-z0-9_]+)\s*\(", text))
        names |= set(re.findall(r"\b(rcl(?:c|_action)?_[a-z0-9_]+_t)\b", text))
    return names


def rcl_ret_names():
    text = (C_INCLUDE / "rclc" / "detail" / "rcl_compat.h").read_text()
    return set(re.findall(r"#define (RCL_RET_[A-Z_]+)\b", text))


def stem(name):
    return name[len("nros_"):] if name.startswith("nros_") else name[len("NROS_"):]


def entry_only_tokens():
    entry, rest = set(), set()
    for f in tracked_headers(C_INCLUDE):
        toks = set(TOKEN.findall(code_of(f.read_text(errors="replace"))))
        (entry if f.name in ENTRY_HEADERS else rest).update(toks)
    return entry - rest


def build_map():
    by_ours, theirs = load_parity()
    ours_tokens = tokens_in([C_INCLUDE])
    entry_only = entry_only_tokens()
    seam_tokens = tokens_in(SEAM_HEADERS)
    existing = defined_upstream_names_in_nros_c()
    rcl_rets = rcl_ret_names()
    rows = []
    for name in sorted(ours_tokens):
        if name in seam_tokens or name.startswith(("nros_rmw_", "nros_platform_", "nros_board_", "nros_cpp_", "NROS_RMW_")):
            rows.append((name, name, "seam", "declared by a port seam or the C++ FFI"))
            continue
        if name in BAKED_SEAMS:
            rows.append((name, name, "build", "carries the baked RMW selector from a header macro"))
            continue
        if name in HOOKS or name in entry_only or name.startswith(("nros_app_", "NROS_APP_")):
            rows.append((name, name, "hook", "defined by the application or entry, called by startup code"))
            continue
        if name.startswith("nros_config_variant"):
            rows.append((name, name, "build", "config-variant link anchor"))
            continue
        if name.isupper() or name.startswith("NROS_"):
            if BUILD_UPPER.match(name):
                rows.append((name, name, "build", "build or configuration fact"))
                continue
            if name.startswith("NROS_RET_") and name != "NROS_RET_":
                rcl = "RCL_RET_" + name[len("NROS_RET_"):]
                if rcl in rcl_rets:
                    rows.append((name, rcl, "ret", "rcl names this code; value stays ours"))
                else:
                    rows.append((name, "RCLC_RET_" + name[len("NROS_RET_"):], "ret", "no rcl code of this name"))
                continue
            rows.append((name, "RCLC_" + stem(name), "ours", "API constant with no upstream spelling"))
            continue
        if name in OVERRIDES:
            v = OVERRIDES[name]
            if isinstance(v, tuple):
                rows.append((name, *v))
            else:
                rows.append((name, v, "ours", "reviewed: its rclc_ spelling is upstream's, with a different contract"))
            continue
        bucket, their = by_ours.get(name, (None, None))
        if bucket == "systematic" and name not in SIGNATURE_CONFORMED:
            # A spelling match whose contract nobody checked: never adopt on
            # that alone (phase-492 W1.b). Review it, then list it above.
            bucket = "differs"
        if bucket in ("same", "systematic") and their:
            if their in existing:
                rows.append((name, their, "alias", f"`{their}` already names it (a compat alias); the definition takes it"))
            else:
                rows.append((name, their, "adopt", f"conforms to upstream `{their}` ({bucket})"))
            continue
        new = "rclc_" + stem(name)
        why = "ours-only" if bucket in (None, "ours-only") else f"{bucket}: keeps a distinct name"
        if new in theirs:
            why += f"; WARNING `{new}` is an upstream name"
        rows.append((name, new, "ours", why))
    return rows


def render(rows):
    lines = ["# phase-492 W1 — generated by scripts/gen-c-rename-map.py; reviewed, then committed.",
             "# old\tnew\tclass\twhy"]
    lines += ["\t".join(r) for r in rows]
    return "\n".join(lines) + "\n"


def main():
    text = render(build_map())
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            print(f"gen-c-rename-map: {OUT.relative_to(ROOT)} is stale; regenerate it", file=sys.stderr)
            return 1
        print("gen-c-rename-map: OK")
        return 0
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

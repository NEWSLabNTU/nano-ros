#!/usr/bin/env python3
"""Regenerate the committed snapshot of one upstream ROS 2 EDITION's API.

The parity tools (`rmw-api-parity.py`, `rmw-abi-shape.py`, `api-parity.py`)
compare our C / C++ / RMW surfaces against upstream's. Humble's side was
recorded from a real install. A second edition — Jazzy, which needs Ubuntu
noble and so cannot be installed beside Humble on the jammy box — is recorded
from the upstream SOURCES at the commits `docs/reference/editions/<ed>/pins.toml`
names, and COMMITTED, so every gate that reads it runs on the fast line with no
Jazzy anywhere: the same shape as committed bindgen output, with
`scripts/check-upstream-edition.py` as the staleness check.

What is derived, and from what
==============================

  rmw-implementation-contract.txt
      The symbols an rmw implementation must DEFINE. Humble's was measured
      from three `librmw_*_cpp.so` (`rmw-api-parity.py --contract`). Without a
      library, the source that states the same fact is `rmw_implementation`'s
      dispatch layer: every symbol it forwards to the loaded implementation is
      one the implementation must export — the `RMW_INTERFACE_FN(...)` table
      plus the one it spells out by hand (`rmw_init`). The method is VALIDATED,
      not assumed: run on the Humble branch it yields exactly the 88 symbols
      the measured contract holds (`--validate-against-humble`).
  rmw-implementation-signatures.txt
      `rmw-api-inventory.py --signatures` over the pinned rmw headers — the
      extractor Humble's file came from, so the two are comparable byte-wise.
  rmw-type-surface.txt
      Enumerators, struct fields and QoS-profile constants of the rmw headers.
      Three of the four Jazzy-only capabilities the parity question is about
      (matched events, discovery options, type hashes) changed TYPES, not
      function signatures, so a function-only snapshot would report "no
      change" over all three. Recorded for BOTH editions (Humble from the
      installed headers) so the delta is a set difference of two committed
      files.
  ../../api-surface/<ed>/{rclc,rclcpp,rclrs}.json
      `api-parity.py --refresh --edition <ed>` against a synthetic install
      prefix assembled here (see below); rclrs from the pinned crate with
      ROS_DISTRO=<ed>, which selects the rcl bindings it re-exports.

The synthetic prefix — and the one thing in it that is not Jazzy
================================================================

clang parses rclcpp's public headers only with every include resolved, and
`extract_cxx` refuses a partial AST (correctly: a missing declaration reads as
a gap). So the prefix holds the pinned Jazzy include trees verbatim for every
client-library and runtime package, the four template-generated headers
expanded from their Jazzy `.em` / `.in` sources with the parameters the
upstream CMake passes, and the interface packages (rcl_interfaces,
type_description_interfaces, service_msgs, …) GENERATED from their pinned
Jazzy `.msg`/`.srv` sources by the box's HUMBLE rosidl. That last part is the
honest caveat: the generated C/C++ message headers lack Jazzy-generator-only
additions (per-type hash constants, `_Event` service messages). Those are not
in the compared surface — `public_surface.py` drops generated accessors by
file — so the client-library surface is Jazzy's own. Anything not overridden
falls back to the Humble install (`--fallback-prefix`), and the provenance
records which packages did.

Usage (in the ROS 2 distrobox, Humble sourced):
    scripts/gen-upstream-edition.py --edition jazzy --src tmp/jazzy-src fetch
    scripts/gen-upstream-edition.py --edition jazzy --src tmp/jazzy-src interfaces
    scripts/gen-upstream-edition.py --edition jazzy --src tmp/jazzy-src prefix
    scripts/gen-upstream-edition.py --edition jazzy --src tmp/jazzy-src rmw
    scripts/gen-upstream-edition.py --edition jazzy --src tmp/jazzy-src surfaces
    scripts/gen-upstream-edition.py --edition jazzy --src tmp/jazzy-src all
    scripts/gen-upstream-edition.py --edition humble type-surface   # installed headers
    scripts/gen-upstream-edition.py --validate-against-humble <rmw_implementation humble checkout>
    scripts/gen-upstream-edition.py --self-test
"""

import argparse
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "lib"))

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # the ros:humble image is 3.10
    import tomli as tomllib

EDITIONS_DIR = os.path.join(ROOT, "docs", "reference", "editions")
HUMBLE_CONTRACT = os.path.join(ROOT, "docs", "reference", "rmw-implementation-contract.txt")

# Interface packages the client-library headers include, by source repo.
INTERFACES = {
    "rcl_interfaces": (
        "action_msgs", "builtin_interfaces", "lifecycle_msgs", "rcl_interfaces",
        "rosgraph_msgs", "service_msgs", "statistics_msgs",
        "type_description_interfaces",
    ),
    "unique_identifier_msgs": (".",),
}

# `<prefix>/include/<pkg>` -> `<repo>/<subdir>` (a directory that holds `<pkg>/`).
SOURCE_INCLUDES = {
    "rmw": ("rmw", "rmw/include"),
    "rcl": ("rcl", "rcl/include"),
    "rcl_action": ("rcl", "rcl_action/include"),
    "rcl_lifecycle": ("rcl", "rcl_lifecycle/include"),
    "rcl_yaml_param_parser": ("rcl", "rcl_yaml_param_parser/include"),
    "rclcpp_action": ("rclcpp", "rclcpp_action/include"),
    "rclcpp_lifecycle": ("rclcpp", "rclcpp_lifecycle/include"),
    "rclcpp_components": ("rclcpp", "rclcpp_components/include"),
    "rcpputils": ("rcpputils", "include"),
    "rosidl_runtime_c": ("rosidl", "rosidl_runtime_c/include"),
    "rosidl_runtime_cpp": ("rosidl", "rosidl_runtime_cpp/include"),
    "rosidl_typesupport_interface": ("rosidl", "rosidl_typesupport_interface/include"),
    "rosidl_typesupport_introspection_c": ("rosidl", "rosidl_typesupport_introspection_c/include"),
    "rosidl_typesupport_introspection_cpp": ("rosidl", "rosidl_typesupport_introspection_cpp/include"),
    "rosidl_dynamic_typesupport": ("rosidl_dynamic_typesupport", "include"),
    "rosidl_typesupport_c": ("rosidl_typesupport", "rosidl_typesupport_c/include"),
    "rosidl_typesupport_cpp": ("rosidl_typesupport", "rosidl_typesupport_cpp/include"),
    "rcl_logging_interface": ("rcl_logging", "rcl_logging_interface/include"),
    "libstatistics_collector": ("libstatistics_collector", "include"),
}
# Packages whose include tree needs one or more GENERATED files beside the
# verbatim sources: copied, then expanded in place by `_expand_templates`.
TEMPLATED = {
    "rcutils": ("rcutils", "include"),
    "rclcpp": ("rclcpp", "rclcpp/include"),
    "tracetools": ("ros2_tracing", "tracetools/include"),
}


def load_pins(edition):
    path = os.path.join(EDITIONS_DIR, edition, "pins.toml")
    with open(path, "rb") as fh:
        pins = tomllib.load(fh)
    if pins.get("edition") != edition:
        raise SystemExit(f"{path}: edition = {pins.get('edition')!r}, expected {edition!r}")
    return pins["repo"]


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, text=True, **kw)


def git_head(path):
    return subprocess.run(["git", "-C", path, "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()


# ---------------------------------------------------------------------------
# fetch
# ---------------------------------------------------------------------------

def fetch(edition, src):
    os.makedirs(src, exist_ok=True)
    for name, pin in sorted(load_pins(edition).items()):
        dest = os.path.join(src, name)
        if not os.path.isdir(dest):
            run(["git", "clone", "-q", "--depth", "1", "--branch", pin["branch"], pin["url"], dest])
        if git_head(dest) != pin["commit"]:
            # A pin behind its branch tip (ros2_rust): fetch the commit itself.
            run(["git", "-C", dest, "fetch", "-q", "--depth", "1", "origin", pin["commit"]])
            run(["git", "-C", dest, "checkout", "-q", "--detach", pin["commit"]])
        print(f"  {name:28s} {git_head(dest)}")


def verify_heads(edition, src):
    bad = []
    for name, pin in sorted(load_pins(edition).items()):
        head = git_head(os.path.join(src, name))
        if head != pin["commit"]:
            bad.append(f"{name}: checkout at {head or '(missing)'}, pin {pin['commit']}")
    if bad:
        raise SystemExit("sources do not match pins.toml — run `fetch`:\n  " + "\n  ".join(bad))


# ---------------------------------------------------------------------------
# interfaces — Jazzy .msg/.srv, Humble rosidl
# ---------------------------------------------------------------------------

def interfaces(src):
    ws = os.path.join(src, "ifaces-ws")
    srcdir = os.path.join(ws, "src")
    if os.path.isdir(srcdir):
        shutil.rmtree(srcdir)
    os.makedirs(srcdir)
    for repo, pkgs in INTERFACES.items():
        for pkg in pkgs:
            from_ = os.path.normpath(os.path.join(src, repo, pkg))
            shutil.copytree(from_, os.path.join(srcdir, os.path.basename(from_)),
                            ignore=shutil.ignore_patterns(".git"))
    # Iron renamed the generator umbrella; Humble has only the old name, and
    # the two generate the same C/C++ for these packages.
    for dirpath, _dirs, files in os.walk(srcdir):  # walk-ok: an untracked colcon workspace under tmp/, not tracked files
        for f in files:
            if f in ("CMakeLists.txt", "package.xml"):
                p = os.path.join(dirpath, f)
                text = open(p, encoding="utf-8").read()
                new = (text.replace("rosidl_core_generators", "rosidl_default_generators")
                           .replace("rosidl_core_runtime", "rosidl_default_runtime"))
                if new != text:
                    open(p, "w", encoding="utf-8").write(new)
    run(["colcon", "build", "--merge-install", "--cmake-args", "-DBUILD_TESTING=OFF"], cwd=ws)


# ---------------------------------------------------------------------------
# prefix
# ---------------------------------------------------------------------------

def _em(template, out, defines=()):
    args = []
    for d in defines:
        args += ["-D", d]
    os.makedirs(os.path.dirname(out), exist_ok=True)
    run([sys.executable, "-c",
         "import em, sys; em.invoke(sys.argv[1:])", *args, "-o", out, template])


def _expand_templates(src, inc):
    rcutils = os.path.join(src, "rcutils")
    _em(os.path.join(rcutils, "resource", "logging_macros.h.em"),
        os.path.join(inc, "rcutils", "rcutils", "logging_macros.h"),
        [f'rcutils_module_path="{rcutils}"'])
    rclcpp = os.path.join(src, "rclcpp", "rclcpp")
    _em(os.path.join(rclcpp, "resource", "logging.hpp.em"),
        os.path.join(inc, "rclcpp", "rclcpp", "logging.hpp"))
    ni = os.path.join(rclcpp, "include", "rclcpp", "node_interfaces")
    for f in sorted(os.listdir(ni)):
        m = re.fullmatch(r"(node_\w+_interface)\.hpp", f)
        if not m:
            continue
        name = m.group(1)
        out_dir = os.path.join(inc, "rclcpp", "rclcpp", "node_interfaces")
        _em(os.path.join(rclcpp, "resource", "interface_traits.hpp.em"),
            os.path.join(out_dir, f"{name}_traits.hpp"), [f"interface_name = '{name}'"])
        _em(os.path.join(rclcpp, "resource", "get_interface.hpp.em"),
            os.path.join(out_dir, f"get_{name}.hpp"), [f"interface_name = '{name}'"])
    # tracetools/config.h.in: tracing compiled OUT, the configuration a
    # non-Linux / non-LTTng build gets, and the one an API surface wants (no
    # LTTng headers needed).
    cfg_in = os.path.join(inc, "tracetools", "tracetools", "config.h.in")
    text = open(cfg_in, encoding="utf-8").read()
    text = re.sub(r"#cmakedefine\s+(\w+).*", r"/* #undef \1 */", text)
    open(cfg_in[:-3], "w", encoding="utf-8").write(text)


def prefix(edition, src, fallback):
    verify_heads(edition, src)
    pre = os.path.join(src, "prefix")
    inc = os.path.join(pre, "include")
    if os.path.isdir(pre):
        shutil.rmtree(pre)
    os.makedirs(inc)
    ours = set()
    for pkg, (repo, sub) in SOURCE_INCLUDES.items():
        target = os.path.join(src, repo, sub)
        if not os.path.isdir(os.path.join(target, pkg)):
            raise SystemExit(f"{target} holds no {pkg}/ — layout moved upstream")
        os.symlink(os.path.abspath(target), os.path.join(inc, pkg))
        ours.add(pkg)
    for pkg, (repo, sub) in TEMPLATED.items():
        shutil.copytree(os.path.join(src, repo, sub), os.path.join(inc, pkg))
        ours.add(pkg)
    _expand_templates(src, inc)
    iface_inc = os.path.join(src, "ifaces-ws", "install", "include")
    for pkg in sorted(os.listdir(iface_inc)):
        os.symlink(os.path.abspath(os.path.join(iface_inc, pkg)), os.path.join(inc, pkg))
        ours.add(pkg)
    fell_back = []
    finc = os.path.join(fallback, "include")
    for pkg in sorted(os.listdir(finc)):
        if pkg in ours or not os.path.isdir(os.path.join(finc, pkg)):
            continue
        os.symlink(os.path.join(finc, pkg), os.path.join(inc, pkg))
        fell_back.append(pkg)
    with open(os.path.join(pre, "fallback.txt"), "w") as fh:
        fh.write("\n".join(fell_back) + "\n")
    print(f"prefix: {len(ours)} package(s) from the {edition} pins, "
          f"{len(fell_back)} from {fallback}")
    return pre


# ---------------------------------------------------------------------------
# rmw — contract, signatures, type surface
# ---------------------------------------------------------------------------

def contract_from_dispatch(functions_cpp):
    """Symbols `rmw_implementation` forwards: the implementation contract."""
    text = open(functions_cpp, encoding="utf-8").read()
    table = set(re.findall(r"RMW_INTERFACE_FN\(\s*(rmw_\w+)", text))
    by_hand = set(re.findall(r"(?m)^(rmw_\w+)\(", text))
    return sorted(table | by_hand)


def read_list(path):
    return [l.strip() for l in open(path, encoding="utf-8")
            if l.strip() and not l.startswith("#")]


def validate_against_humble(rmw_impl_humble):
    derived = contract_from_dispatch(
        os.path.join(rmw_impl_humble, "rmw_implementation", "src", "functions.cpp"))
    measured = read_list(HUMBLE_CONTRACT)
    if derived != measured:
        print("dispatch-derived Humble contract DISAGREES with the measured one:")
        for s in sorted(set(derived) - set(measured)):
            print(f"  + {s}")
        for s in sorted(set(measured) - set(derived)):
            print(f"  - {s}")
        return 1
    print(f"dispatch-derived Humble contract == measured contract ({len(measured)} symbols)"
          f" — rmw_implementation {git_head(rmw_impl_humble)}")
    return 0


def _strip(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    text = re.sub(r"(?m)//.*$", " ", text)
    return text


def type_surface(include_dir):
    """`kind<TAB>name` lines for the rmw headers' TYPE surface.

    enum      `RMW_EVENT_PUBLICATION_MATCHED` (qualified by its enum typedef)
    field     `rmw_init_options_s.discovery_options` (by struct tag)
    type      every typedef name
    profile   every `static const rmw_qos_profile_t` constant
    macro     every object-like `#define RMW_*` that is not an include guard
    """
    out = set()
    headers = []
    for dirpath, _dirs, files in os.walk(include_dir):  # walk-ok: an upstream checkout under tmp/, not this tree
        headers += [os.path.join(dirpath, f) for f in files if f.endswith(".h")]
    for path in sorted(headers):
        raw = open(path, encoding="utf-8").read()
        for m in re.finditer(r"(?m)^\s*#\s*define\s+(RMW_[A-Z0-9_]+)\b(?!\()", raw):
            name = m.group(1)
            if not name.endswith("_H_") and not name.endswith("_H"):
                out.add(("macro", name))
        text = _strip(raw)
        text = re.sub(r"(?m)^\s*#.*$", " ", text)
        for m in re.finditer(r"typedef\s+enum\s+(?:RMW_PUBLIC_TYPE\s+)?(\w+)?\s*\{(.*?)\}\s*(\w+)\s*;", text, re.S):
            tname = m.group(3)
            out.add(("type", tname))
            for item in m.group(2).split(","):
                item = item.strip()
                name = re.match(r"([A-Za-z_]\w*)", item)
                if name and not name.group(1).startswith("RMW_DECLARE_DEPRECATED"):
                    out.add(("enum", f"{tname}.{name.group(1)}"))
        for m in re.finditer(r"(typedef\s+)?struct\s+(?:RMW_PUBLIC_TYPE\s+)?(\w+)\s*\{", text):
            tag = m.group(2)
            # Walk the balanced body.
            i, depth = m.end(), 1
            while i < len(text) and depth:
                depth += {"{": 1, "}": -1}.get(text[i], 0)
                i += 1
            body = text[m.end():i - 1]
            if m.group(1):
                alias = re.match(r"\s*(\w+)\s*;", text[i:])
                if alias:
                    out.add(("type", alias.group(1)))
            body = re.sub(r"\{[^{}]*\}", " ", body)  # nested anonymous unions
            for decl in body.split(";"):
                decl = " ".join(decl.split())
                if not decl:
                    continue
                f = re.search(r"\(\s*\*\s*(\w+)\s*\)\s*\(", decl) or re.search(
                    r"(\w+)\s*(\[[^\]]*\])*\s*$", decl)
                if f:
                    out.add(("field", f"{tag}.{f.group(1)}"))
        for m in re.finditer(r"typedef\s+(?:struct|union|enum)?\s*[\w\s\*]*?\b(\w+)\s*;", text):
            out.add(("type", m.group(1)))
        for m in re.finditer(r"typedef\s+[^;{]*\(\s*\*\s*(\w+)\s*\)", text):
            out.add(("type", m.group(1)))
        for m in re.finditer(r"static\s+const\s+rmw_qos_profile_t\s+(\w+)", text):
            out.add(("profile", m.group(1)))
    return sorted(out)


def write_snapshot(path, header_lines, body_lines):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        for l in header_lines:
            fh.write(("# " + l).rstrip() + "\n")
        for l in body_lines:
            fh.write(l + "\n")
    print(f"wrote {os.path.relpath(path, ROOT)} ({len(body_lines)} line(s))")


def pin_line(edition, *names):
    pins = load_pins(edition)
    return [f"pin {n}: {pins[n]['url']} @ {pins[n]['commit']} ({pins[n]['branch']})" for n in names]


def rmw(edition, src):
    verify_heads(edition, src)
    d = os.path.join(EDITIONS_DIR, edition)
    contract = contract_from_dispatch(
        os.path.join(src, "rmw_implementation", "rmw_implementation", "src", "functions.cpp"))
    write_snapshot(
        os.path.join(d, "rmw-implementation-contract.txt"),
        [f"The rmw IMPLEMENTATION contract — ROS 2 {edition.capitalize()}.", "",
         "Derived from rmw_implementation's dispatch table (every symbol it forwards",
         "to the loaded implementation), not from a library: no Jazzy install exists",
         "on the jammy box. Validated: the same derivation over the Humble branch",
         "yields exactly the measured 88 of ../../rmw-implementation-contract.txt.",
         "Generated by scripts/gen-upstream-edition.py; do not hand-edit.",
         *pin_line(edition, "rmw_implementation")],
        contract)
    inv = subprocess.run(
        [sys.executable, os.path.join(HERE, "rmw-api-inventory.py"), "--signatures",
         "--include", os.path.join(src, "rmw", "rmw", "include", "rmw")],
        capture_output=True, text=True, check=True).stdout
    body = [l for l in inv.splitlines() if l.strip() and not l.startswith("#")]
    write_snapshot(
        os.path.join(d, "rmw-implementation-signatures.txt"),
        [f"Signatures of the rmw headers — ROS 2 {edition.capitalize()}.",
         "name<TAB>return<TAB>params<TAB>header. Parameter NAMES ARE KEPT.",
         "Generated by scripts/gen-upstream-edition.py (rmw-api-inventory.py",
         "--signatures over the pinned headers); do not hand-edit.",
         *pin_line(edition, "rmw"),
         "format: 2 (parameter names kept; strip them to compare)"],
        body)
    write_type_surface(edition, os.path.join(src, "rmw", "rmw", "include", "rmw"),
                       pin_line(edition, "rmw"))


def write_type_surface(edition, include_dir, provenance):
    rows = ["\t".join(r) for r in type_surface(include_dir)]
    write_snapshot(
        os.path.join(EDITIONS_DIR, edition, "rmw-type-surface.txt"),
        [f"The rmw TYPE surface — ROS 2 {edition.capitalize()}: enumerators, struct",
         "fields, typedefs, QoS profile constants, object-like RMW_* macros.",
         "kind<TAB>name. Generated by scripts/gen-upstream-edition.py; do not hand-edit.",
         *provenance],
        rows)


def humble_type_surface():
    inc = None
    for p in os.environ.get("AMENT_PREFIX_PATH", "").split(os.pathsep):
        cand = os.path.join(p, "include", "rmw", "rmw")
        if os.path.isdir(cand):
            inc = cand
            break
    if not inc:
        raise SystemExit("no rmw headers via AMENT_PREFIX_PATH — source Humble first")
    share = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(inc))),
                         "share", "rmw", "package.xml")
    ver = "?"
    if os.path.exists(share):
        m = re.search(r"<version>([^<]+)", open(share).read())
        ver = m.group(1) if m else "?"
    write_type_surface("humble", inc, [f"source: installed ros-humble-rmw {ver} headers"])


# ---------------------------------------------------------------------------
# client-library surfaces
# ---------------------------------------------------------------------------

def surfaces(edition, src):
    verify_heads(edition, src)
    pre = os.path.join(src, "prefix")
    if not os.path.isdir(pre):
        raise SystemExit("no prefix — run `prefix` first")
    run([sys.executable, os.path.join(HERE, "api-parity.py"), "--refresh",
         "--edition", edition, "--ros-prefix", pre,
         "--rclc", os.path.join(src, "rclc"),
         "--rclrs", os.path.join(src, "ros2_rust", "rclrs"),
         "--lang", "c", "--lang", "cpp", "--lang", "rust"])


# ---------------------------------------------------------------------------

def self_test():
    import tempfile
    bad = []
    with tempfile.TemporaryDirectory() as td:
        fcpp = os.path.join(td, "functions.cpp")
        open(fcpp, "w").write(
            "RMW_INTERFACE_FN(\n  rmw_take,\n  rmw_ret_t, RMW_RET_ERROR,\n  1, ARG_TYPES(int))\n"
            "RMW_INTERFACE_FN(rmw_publish, rmw_ret_t, RMW_RET_ERROR, 0, ARG_TYPES())\n"
            "rmw_ret_t\nrmw_init(const rmw_init_options_t * o, rmw_context_t * c)\n{\n}\n"
            "  // GET_SYMBOL(rmw_ghost)\n")
        got = contract_from_dispatch(fcpp)
        if got != ["rmw_init", "rmw_publish", "rmw_take"]:
            bad.append(f"contract_from_dispatch: {got}")
        inc = os.path.join(td, "rmw")
        os.makedirs(inc)
        open(os.path.join(inc, "types.h"), "w").write(
            "#ifndef RMW__TYPES_H_\n#define RMW__TYPES_H_\n#define RMW_GID_STORAGE_SIZE 16u\n"
            "typedef enum RMW_PUBLIC_TYPE rmw_event_type_e {\n  RMW_EVENT_A, // c\n  RMW_EVENT_B = 3,\n"
            "  RMW_EVENT_INVALID\n} rmw_event_type_t;\n"
            "typedef struct RMW_PUBLIC_TYPE rmw_x_s {\n  int a;\n  const char * b;\n"
            "  void (*cb)(int);\n  uint8_t data[16];\n  union { int u; } un;\n} rmw_x_t;\n"
            "static const rmw_qos_profile_t rmw_qos_profile_p = {0};\n#endif\n")
        surf = set(type_surface(inc))
        want = {("enum", "rmw_event_type_t.RMW_EVENT_A"), ("enum", "rmw_event_type_t.RMW_EVENT_B"),
                ("field", "rmw_x_s.a"), ("field", "rmw_x_s.b"), ("field", "rmw_x_s.cb"),
                ("field", "rmw_x_s.data"), ("field", "rmw_x_s.un"), ("type", "rmw_x_t"),
                ("type", "rmw_event_type_t"), ("profile", "rmw_qos_profile_p"),
                ("macro", "RMW_GID_STORAGE_SIZE")}
        if want - surf:
            bad.append(f"type_surface missed {sorted(want - surf)}")
        if ("macro", "RMW__TYPES_H_") in surf:
            bad.append("type_surface counted an include guard")
    if bad:
        for b in bad:
            sys.stderr.write("gen-upstream-edition --self-test: " + b + "\n")
        return 2
    print("gen-upstream-edition --self-test: OK (3 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--edition", default="jazzy")
    ap.add_argument("--src", help="working dir for the pinned checkouts (e.g. tmp/jazzy-src)")
    ap.add_argument("--fallback-prefix", default="/opt/ros/humble")
    ap.add_argument("--validate-against-humble", metavar="RMW_IMPLEMENTATION_CHECKOUT")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("step", nargs="?",
                    choices=("fetch", "interfaces", "prefix", "rmw", "surfaces", "all",
                             "type-surface"))
    a = ap.parse_args(argv)
    if a.self_test:
        return self_test()
    if a.validate_against_humble:
        return validate_against_humble(a.validate_against_humble)
    if a.step == "type-surface" and a.edition == "humble":
        humble_type_surface()
        return 0
    if not a.src or not a.step:
        ap.error("--src and a step are required")
    src = os.path.abspath(a.src)
    steps = ["fetch", "interfaces", "prefix", "rmw", "surfaces"] if a.step == "all" else [a.step]
    for s in steps:
        if s == "fetch":
            fetch(a.edition, src)
        elif s == "interfaces":
            interfaces(src)
        elif s == "prefix":
            prefix(a.edition, src, a.fallback_prefix)
        elif s == "rmw":
            rmw(a.edition, src)
        elif s == "surfaces":
            surfaces(a.edition, src)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

"""Read a `[prereq.*]` manager field — the ONE Python spelling (phase-447 D2).

A manager field (`apt`, `dnf`, `pacman`, `brew`) has two shapes since D2:

    apt = ["libssl3"]                                         # every release
    apt = { default = ["libssl3"], noble = ["libssl3t64"] }   # per release

`PrereqDep::manager_packages` / `ManagerPackages` is the Rust half. Every
script that reads the index goes through here rather than `entry.get("apt")`,
because on the table shape that expression returns a DICT and iterating it
yields release names — `noble` read as an apt package, silently.
"""

DEFAULT_KEY = "default"


def for_release(value, release=None):
    """The names that apply on `release`: its override, else the default."""
    if value is None:
        return []
    if isinstance(value, list):
        return list(value)
    if release is not None and release != DEFAULT_KEY and release in value:
        return list(value[release])
    return list(value.get(DEFAULT_KEY, []))


def all_names(value):
    """Every name the field mentions, under any release — for a gate asking
    "is this package indexed at all?"."""
    if value is None:
        return []
    if isinstance(value, list):
        return list(value)
    out = []
    for names in value.values():
        out.extend(names)
    return out


def host_release(path="/etc/os-release"):
    """`VERSION_CODENAME`, else `VERSION_ID` — mirrors `host_os_release`."""
    try:
        with open(path) as fh:
            text = fh.read()
    except OSError:
        return None
    fields = {}
    for line in text.splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            fields[k.strip()] = v.strip().strip('"')
    return fields.get("VERSION_CODENAME") or fields.get("VERSION_ID") or None


# issue 1744 — the mirror every REPACKED dist is published to. A `[tool.*]`
# whose dists point anywhere else ships UPSTREAM's own archive (both Zephyr SDK
# rows, `ninja`, `clang-format`), unmodified, with upstream's host requirements.
MIRROR = "github.com/NEWSLabNTU/nano-ros-sdk/releases/"


def host_key():
    """`<os>-<arch>` — mirrors `sdk_index::host_key` (`aarch64` -> `arm64`)."""
    import platform

    arch = platform.machine().lower()
    arch = {"aarch64": "arm64", "amd64": "x86_64"}.get(arch, arch)
    osname = {"darwin": "macos"}.get(platform.system().lower(), platform.system().lower())
    return f"{osname}-{arch}"


def is_upstream_dist(tool):
    """True when the tool has dists and NONE is on our mirror (not repacked)."""
    urls = [d.get("url", "") for d in (tool.get("dist") or {}).values()]
    return bool(urls) and not any(MIRROR in u for u in urls)


def tool_root(tool):
    """The tool's root inside its prefix: `subdir` with `{version}`, or ''."""
    sub = tool.get("subdir")
    return sub.replace("{version}", tool.get("version", "")) if sub else ""


def run_programs(tool, host=None):
    """The programs nano-ros RUNS from a dist, prefix-relative (issue 1744).

    Read from the `smoke` probes — the declared "these must work" set — taking
    each probe's argv[0], resolved under `subdir`, for the probes that apply on
    `host` (a probe's `hosts`, empty meaning every host). Ordered, de-duplicated.
    """
    host = host or host_key()
    out = []
    for probe in tool.get("smoke") or []:
        hosts = probe.get("hosts") or []
        if hosts and host not in hosts:
            continue
        argv = (probe.get("run") or "").split()
        if not argv:
            continue
        rel = "/".join(x for x in (tool_root(tool), argv[0]) if x)
        if rel not in out:
            out.append(rel)
    return out


def self_test():
    ssl = {"default": ["libssl3"], "noble": ["libssl3t64"]}
    assert for_release(["x"], "noble") == ["x"]
    assert for_release(ssl, "noble") == ["libssl3t64"]
    assert for_release(ssl, "jammy") == ["libssl3"]
    assert for_release(ssl, None) == ["libssl3"]
    assert for_release({"noble": []}, "noble") == []
    assert for_release({"jammy": ["a"]}, "noble") == []
    assert sorted(all_names(ssl)) == ["libssl3", "libssl3t64"]
    assert "noble" not in all_names(ssl), "a release name is not a package"
    assert for_release(None) == [] and all_names(None) == []
    z = {
        "version": "1.0.1",
        "subdir": "zephyr-sdk-{version}",
        "dist": {"linux-x86_64": {"url": "https://github.com/zephyrproject-rtos/sdk-ng/x.tar.xz"}},
        "smoke": [
            {"run": "gnu/a/bin/a-gcc --version", "expect": "a"},
            {"run": "gnu/a/bin/a-gcc -v", "expect": "a"},
            {"run": "hosttools/dtc --version", "expect": "DTC", "hosts": ["linux-x86_64"]},
        ],
    }
    assert is_upstream_dist(z)
    assert not is_upstream_dist({"dist": {"linux-x86_64": {"url": "https://" + MIRROR + "q.tar.zst"}}})
    assert not is_upstream_dist({}), "no dist row is not an upstream dist"
    assert run_programs(z, "linux-x86_64") == [
        "zephyr-sdk-1.0.1/gnu/a/bin/a-gcc", "zephyr-sdk-1.0.1/hosttools/dtc"]
    assert run_programs(z, "macos-arm64") == ["zephyr-sdk-1.0.1/gnu/a/bin/a-gcc"], \
        "a host-scoped probe is not in another host's run set"

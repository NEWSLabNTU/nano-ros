function(nros_zephyr_configure_rmw_xrce)
# RFC-0071 D10 (phase-506 W3) — XRCE's spelling of the transport ROLE: the
# Agent's `host:port`, the bare form `nros-rmw-xrce/session.c`'s
# `parse_host_port` accepts. Issue #174 / phase-286 W3 synthesised it inside
# `entry_config.h`; the backend's own module is where it belongs.
if(DEFINED CONFIG_NROS_XRCE_AGENT_ADDR AND DEFINED CONFIG_NROS_XRCE_AGENT_PORT)
    zephyr_compile_definitions(
        "NROS_ENTRY_LOCATOR=\"${CONFIG_NROS_XRCE_AGENT_ADDR}:${CONFIG_NROS_XRCE_AGENT_PORT}\"")
endif()
# -------------------------------------------------------------------------
# XRCE-DDS platform support (compiled by Zephyr CMake)
# -------------------------------------------------------------------------
# Nothing to compile here anymore.
#
#   - Network readiness  → nros-platform-zephyr (`net_wait.c`,
#     `nros_platform_zephyr_wait_network`), an RMW-independent primitive
#     (Phase 200.1).
#   - Clock symbols (`uxr_millis` / `uxr_nanos`) → routed through the
#     canonical platform ABI (`nros_platform_clock_ms` /
#     `nros_platform_clock_us`) by `nros-rmw-xrce/src/platform_aliases.c`,
#     compiled unconditionally into the cffi staticlib for every target
#     (phase-230 Wave 2). The retired Zephyr-only override that called
#     `k_uptime_get()` directly used to live in `xrce-zephyr/src/xrce_zephyr.c`
#     and shadowed the canonical version on the link line — it is gone.
#
# Kept as an explicit no-op so the call site in zephyr/CMakeLists.txt and
# the RMW-selection plumbing stay symmetrical with the other transports.

endfunction()

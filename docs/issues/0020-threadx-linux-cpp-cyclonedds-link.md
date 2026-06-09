---
id: 20
title: ThreadX-linux C++ CycloneDDS fixtures fail to link (nros_rmw_cffi_register_named undefined)
status: open
type: bug
area: cyclonedds
related: [issue-0011]
---

The ThreadX-linux **C++ + CycloneDDS** examples
(`examples/threadx-linux/cpp/*`, `build-cyclonedds`) fail at link with:

```
undefined reference to `nros_rmw_cffi_register_named'
  (from libnros_rmw_cyclonedds.a)
```

**Pre-existing — not a #11 regression.** Surfaced while converging the
ThreadX-linux C++ examples to `nros_find_interfaces()` (issue #11). Confirmed
by reproducing the identical link failure with the **pristine**
`find_package(std_msgs)` form of the example and the byte-identical
message-lib link line — i.e. the CycloneDDS backend register symbol is
missing regardless of how message deps are resolved. The C++ + CycloneDDS
backend link path on threadx-linux is independently broken.

The zenoh and XRCE variants of the same examples link fine; only CycloneDDS
is affected. `libnros_rmw_cyclonedds.a` references `nros_rmw_cffi_register_named`
(the cffi-vtable registration entry) but the symbol isn't provided in the
threadx-linux C++ link graph — likely a missing/`--gc-sections`-pruned cffi
register TU, or a link-order / whole-archive gap for the cyclonedds backend on
this platform.

**To fix:** ensure `nros_rmw_cffi_register_named` (from `nros-rmw-cffi`) is
provided + retained in the threadx-linux C++ CycloneDDS link (whole-archive
the register TU / fix link order), so `libnros_rmw_cyclonedds.a` resolves it.
Cross-ref the working native + ThreadX-RV64 CycloneDDS C++ link setup.

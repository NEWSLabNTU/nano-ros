/*
 * Phase 171.0.c — Zephyr's minimal C++ runtime defines the nothrow
 * new/delete overloads, but still leaves the `std::nothrow` tag object
 * unresolved on the AArch64 FVP CycloneDDS build.
 */
#include <new>

const std::nothrow_t std::nothrow{};

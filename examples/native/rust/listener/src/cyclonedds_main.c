/// @file cyclonedds_main.c
/// @brief C entry for the Cyclone DDS build path (Phase 170.A). The
/// listener crate is imported by `CMakeLists.txt` as a `staticlib`
/// (`--features rmw-cyclonedds`); this `main` calls `rust_main()` so
/// cmake can link the C++ Cyclone DDS backend + idlc descriptors.

extern int rust_main(void);

int main(void) {
    return rust_main();
}

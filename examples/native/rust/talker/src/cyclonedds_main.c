/// @file cyclonedds_main.c
/// @brief C entry for the Cyclone DDS build path (Phase 170.A). The
/// talker crate is imported by `CMakeLists.txt` as a `staticlib`
/// (`--features rmw-cyclonedds`); this `main` calls the Rust
/// `rust_main()` so the C++ Cyclone DDS backend + idlc descriptors can
/// be linked in by cmake (a pure `cargo build` cannot link Cyclone).

extern int rust_main(void);

int main(void) {
    return rust_main();
}

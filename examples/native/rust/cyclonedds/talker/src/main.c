/// @file main.c
/// @brief Thin C entry that calls the Rust `rust_main()` from the
/// `rustapp` staticlib. Phase 171.C.1.rust — native rust cyclonedds
/// is cmake-driven (corrosion imports `rustapp`; this `main` drives
/// the link with the C++ Cyclone DDS backend + libddsc + stdc++).

extern int rust_main(void);

int main(void) {
    return rust_main();
}

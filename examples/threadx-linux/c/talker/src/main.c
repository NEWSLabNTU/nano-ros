/* ThreadX Linux C talker — entry point (Phase 244 D6).
 *
 * The synthesised `nros_system_main()` (from `nros_threadx_codegen_system`)
 * owns the per-component spawn, executor init, and spin loop; this thin C
 * `main` just calls it. Mirrors the threadx-linux C++ entry shape.
 */

extern int nros_system_main(void);

int main(void) {
    return nros_system_main();
}

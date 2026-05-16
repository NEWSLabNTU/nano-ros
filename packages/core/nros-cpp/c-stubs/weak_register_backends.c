/*
 * Weak default of `nros_app_register_backends` for nros-cpp-only links.
 *
 * nros-cpp's `nros_cpp_init` calls this hook before opening the CFFI
 * RMW session. POSIX-like builds normally register backends from the
 * backend crate's constructor; bare-metal and RTOS build systems can
 * provide a strong per-application definition that calls the selected
 * `nros_rmw_<name>_register` functions explicitly.
 */

#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
void nros_app_register_backends(void) {
    /* Intentionally empty; strong application stubs override this. */
}

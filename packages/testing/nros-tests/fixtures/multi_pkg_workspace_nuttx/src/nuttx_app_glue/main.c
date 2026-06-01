/* Phase 212.H.2 fixture — NuttX bringup glue.
 *
 * NuttX's apps/Application.mk expects an `int <PROGNAME>_main(int,
 * char **)` entry per app. The Phase 212.E `nros codegen-system` host
 * bake emits `nros_system_main()` as the canonical multi-component
 * entry point; this glue forwards the NuttX-side entry into it.
 *
 * Kept intentionally tiny — the bake owns component registration +
 * executor spin; the user contributes only this thin shim.
 */

extern int nros_system_main(int argc, char **argv);

int demo_bringup_main(int argc, char **argv) {
    return nros_system_main(argc, argv);
}

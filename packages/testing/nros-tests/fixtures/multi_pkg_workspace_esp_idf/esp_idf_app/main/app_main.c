/* Phase 212.H.5 fixture — minimal IDF entrypoint. */

#include <stdio.h>

void app_main(void)
{
    /* The nano-ros component contributes the codegen-baked
     * system_main.c (when nros CLI is on PATH) which carries the
     * actual component bring-up; this stub keeps the link satisfied
     * for the Phase 212.E pre-shipped variant. */
    printf("nano-ros 212.H.5 esp-idf fixture: app_main\n");
}

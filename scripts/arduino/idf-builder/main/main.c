// Minimal IDF main. Only exists so `idf.py build` produces the
// per-component static archives we package into `libnanoros.a`. The
// produced ELF is never flashed.

#include <stdio.h>

#include "nros/init.h"

void app_main(void) {
    printf("nano-ros Arduino library builder — placeholder app_main\n");
    // Drag a real `nros_*` symbol in so the linker does not let the
    // unused archives be dead-stripped before we extract them.
    (void)nros_init;
}

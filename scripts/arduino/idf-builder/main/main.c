// Minimal IDF main. Only exists so `idf.py build` produces the
// per-component static archives we package into `libnanoros.a`.
// The produced ELF is never flashed and the link does not need to
// resolve any `nros_*` symbol — the per-component static archives
// `idf.py build` writes under `esp-idf/<component>/lib<component>.a`
// is what `scripts/arduino/build-libnanoros.sh` packages. We
// deliberately do NOT include any nano-ros header here: the C
// surface depends on per-build `nros_config_generated.h` /
// `nros_generated.h` files that nros-c's build.rs writes to
// CORROSION_BUILD_DIR, which is not propagated to user IDF
// components.

#include <stdio.h>

void app_main(void) {
    printf("nano-ros Arduino library builder — placeholder app_main\n");
}

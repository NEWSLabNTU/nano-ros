# Minimal IDF main component. The Arduino library never runs this
# `app_main` — the file exists only so `idf.py build` succeeds and
# produces the per-component static archives we package into
# `libnanoros.a`.
idf_component_register(
    SRCS "main.c"
    INCLUDE_DIRS "."
    REQUIRES nano-ros)

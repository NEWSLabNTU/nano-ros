# Phase 212.H.5 fixture — main IDF component requires `nano-ros`.
idf_component_register(
    SRCS         app_main.c
    INCLUDE_DIRS "."
    REQUIRES     nano-ros)

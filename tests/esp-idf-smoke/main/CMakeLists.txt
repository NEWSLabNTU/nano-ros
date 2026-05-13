idf_component_register(
    SRCS app_main.c
    INCLUDE_DIRS "."
    # Component name follows the EXTRA_COMPONENT_DIRS dir basename
    # (`nros-platform-esp-idf`).
    REQUIRES nros-platform-esp-idf)

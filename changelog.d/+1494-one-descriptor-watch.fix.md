Every build script that reads a platform descriptor now watches the
descriptors through one shared enumerator,
`platform_config::platform_descriptor_files`. The zenoh-pico runner's private
copy is gone, and the FreeRTOS boards' arch-cflags read, which declared none of
the files it reads, now watches them too. A test asserts that resolving a
platform watches every descriptor on the search path.

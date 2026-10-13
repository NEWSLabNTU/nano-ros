C API: `<rclc/rclc.h>` (and every other upstream-path header) now also carries
`RCLC_C_COMPONENT`, so C code needs no `<nros/*.h>` include at all. The
examples, the book and `nros new` include only `<rcl/*.h>` / `<rclc/*.h>`;
`<nros/*.h>` is internal and will be renamed.

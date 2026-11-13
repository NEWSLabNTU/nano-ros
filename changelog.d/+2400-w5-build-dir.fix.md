The FreeRTOS stack-floor check on the cargo road now works for a build that names its own `--target-dir`, and for release-like profiles. Two things broke it before:

- Cargo keeps such a build's intermediate files in a separate build directory. The check now asks cargo where that directory is, instead of looking beside the linked binary.
- A profile without debuginfo stripped the frame information the check reads. It is now kept for that profile. These sections are not loaded on the target, so the image is unchanged.

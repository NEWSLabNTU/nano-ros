# nros-nuttx-depfile.cmake — retarget cargo's dep-info onto the path ninja builds.
#
# Run as a script:
#
#   cmake -DNROS_DEPFILE_IN=<cargo's .d>
#         -DNROS_DEPFILE_OUT=<the .d cmake's DEPFILE names>
#         -DNROS_DEPFILE_TARGET=<the add_custom_command OUTPUT>
#         -P nros-nuttx-depfile.cmake
#
# Why this exists (issue 0820, second round).
#
# A Makefile-format depfile names the artifact it describes, and ninja CHECKS
# that name: `LoadDepFile` compares the rule's target against the edge's first
# output and, when they differ, discards the WHOLE depfile and marks the edge
# `deps_missing_` — permanently dirty. Measured with ninja 1.13.2 against the
# real riscv leaf's depfile:
#
#   ninja explain: expected depfile 'real.d' to mention
#     'nros-nuttx-ffi-out/nros-nuttx-ffi', got
#     '.../corrosion-cargo/nuttx-riscv/<key>/riscv32imac-unknown-nuttx-elf/
#      nros-minsizerel/nros-nuttx-ffi'
#
# That is exactly the shape the NuttX seam acquired when issue 0805 moved the
# artifact out of the SHARED cargo target dir with `--artifact-dir`: cargo still
# writes its dep-info naming its own output, while the add_custom_command OUTPUT
# is now the artifact-dir copy. Copying the file across (which is what this
# replaced) preserves the stale target line, so all 308 dependency paths in it —
# the entire nano-ros Rust closure 0820's fix went to the trouble of capturing —
# are thrown away unread.
#
# The failure is silent in both directions and neither is what the fix intended:
# the edge re-runs cargo on EVERY build (the always-run cost 0820 explicitly
# rejected), and the depfile contributes nothing, so the only thing standing
# between this seam and a museum binary is that accident. Rewriting the target
# is a two-token change to a file cargo already computed exactly.
#
# One rule, one target: cargo writes a single line for a binary target (measured:
# 1 line, 308 prerequisites), and its paths are absolute with no ':' in them, so
# "everything up to the first colon" is the target.

if(NOT NROS_DEPFILE_IN OR NOT NROS_DEPFILE_OUT OR NOT NROS_DEPFILE_TARGET)
    message(FATAL_ERROR
        "nros-nuttx-depfile.cmake: NROS_DEPFILE_IN, NROS_DEPFILE_OUT and "
        "NROS_DEPFILE_TARGET are all required")
endif()

if(NOT EXISTS "${NROS_DEPFILE_IN}")
    # Cargo writes dep-info on every successful build, so an absent one means the
    # rebuild edge is gone. Fail rather than leave a stale `.d` in place: a stale
    # depfile is worse than none, because ninja BELIEVES it.
    message(FATAL_ERROR
        "nros: cargo wrote no dep-info at ${NROS_DEPFILE_IN} — the NuttX seam "
        "would lose its rebuild edge on the nano-ros Rust world (issue 0820)")
endif()

file(READ "${NROS_DEPFILE_IN}" _nros_depfile_content)
string(REGEX REPLACE "^[^:\n]*:" "${NROS_DEPFILE_TARGET}:"
    _nros_depfile_content "${_nros_depfile_content}")
file(WRITE "${NROS_DEPFILE_OUT}" "${_nros_depfile_content}")

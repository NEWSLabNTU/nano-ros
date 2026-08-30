# issue 0896 — a type whose size bound EXISTS, so the corpus covers the
# constant-emitting arm. Without this every corpus message was unbounded or
# unresolved and the `MAX_SERIALIZED_SIZE` path had no golden coverage at all
# (the same gap phase-390 W2 found in the `configured` arm).
#
# Deliberately mixes widths and a bounded string: the XCDR1/XCDR2 numbers must
# DIFFER here, so a regression that emits one value for both is visible in the
# golden diff rather than silently agreeing.
bool      flag
int64     wide
uint8     narrow
float64   d
string<=8 label
int32[4]  fixed

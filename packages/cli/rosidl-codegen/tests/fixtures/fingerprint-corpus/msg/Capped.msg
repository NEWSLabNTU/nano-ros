# phase-403 W0 -- a type whose bound comes from the CODEGEN CONFIG, not from the
# .msg. Neither field carries an IDL bound, so under the `inline` corpus arm
# (empty resolver) this type is unbounded and both members are named; under the
# `configured` arm the two `nros-codegen.toml` entries are inline caps, so it
# gets real TX/RX constants and a BoundedString/BoundedSequence schema.
#
# That A/B is the whole point of the file: it is the only place in the corpus
# where the SAME .msg produces "no bound" and "a bound" purely from config, which
# is the behaviour this wave adds. Shapes.msg cannot cover it -- its caps are
# `heap` and `view`, whose caps are hints rather than limits (RFC-0033 "What each
# mode GUARANTEES"), so they deliberately do NOT bound.
#
# TWO unbounded members, not one, so the `inline` golden also pins the
# all-members diagnostic: naming only `label` would leave the old first-offender
# behaviour passing.
string  label
int64[] samples

# nros-bridge

Cross-RMW bridge primitives for nano-ros (Phase 128.F).

Single-backend binaries do not need this crate. Pull it in when one
process must speak more than one RMW backend (e.g. forward a topic
from a Zenoh field link to a DDS control link).

See the crate-level rustdoc for the wiring pattern.

A copied-out project now builds a byte-identical image from any directory: the
generated cargo entry reaches the project through a relative `_ws` link, and the
project path is remapped out of debug info. A project whose own crates name a
relative path outside it is built as before, and `nros build` says which
crate and path keep it from reproducing.

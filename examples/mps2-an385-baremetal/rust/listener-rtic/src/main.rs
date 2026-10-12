#![no_std]
#![no_main]

// The panic handler is the board's crate-root item (`[board.entry]
// crate_root_extra`), carried by the generated selection package (issue 1305).

nros::main!(panic = "own");

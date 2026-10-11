#![no_std]
#![no_main]
// `own`: the handler is the board's crate-root provider (`panic-semihosting`),
// which the generated selection package links (issue 1305).
nros::main!(panic = "own");

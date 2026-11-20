# graph_query_ws

One image, three components: a C, a C++ and a Rust probe, each asking the ROS
graph the same question through its own language's surface. Each prints one
`GRAPH_QUERY lang=<x> ...` line (node, topic and service counts, and
`/chatter`'s publisher count and type) once its answer settles.
`tests/graph_query_parity.rs` starts the peer
and asserts the three lines agree.

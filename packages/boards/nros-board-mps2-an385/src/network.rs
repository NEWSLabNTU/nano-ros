//! Network poll callback and global state for MPS2-AN385 (LAN9118).

use lan9118_smoltcp::Lan9118;

nros_smoltcp::define_network_state!(NETWORK_STATE: Lan9118, poll = smoltcp_network_poll);

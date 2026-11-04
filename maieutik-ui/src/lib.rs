//! Maieutik UI — client local reliant Maieutik, Laya (System 1) et Odysseus.
//! Logique headless testée sans écran ; interface egui derrière la feature `gui`.

pub mod backend;
pub mod collections;
pub mod daemon;
pub mod egress;
pub mod laya;
pub mod mcp_client;
pub mod mock;
pub mod odysseus;
pub mod pipeline;
pub mod ports;
pub mod status;

#[cfg(feature = "gui")]
pub mod app;

//! Maieutik API — serveur HTTP pour l'intégration frontend et MCP.
//!
//! Architecture :
//! - `ports/` : traits définissant les frontières (FileCatalog, Generator)
//! - `adapters/` : implémentations concrètes qui délègent à maieutik-core
//! - `app.rs` : agrégateur d'état pour axum
//! - `routes/` : handlers HTTP qui utilisent les ports via State<T>

pub mod adapters;
pub mod app;
pub mod ports;
pub mod routes;

// Réexportations utiles pour le type State axum
pub use app::ApiState;
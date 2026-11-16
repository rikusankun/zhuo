//! Domain-service boundary for backend workflows.
//!
//! Keep Axum extractors and HTTP response types in route handlers. Services own
//! domain operations, accept typed input and application state, and return
//! `anyhow::Result` so they can be tested independently of HTTP.
//!
//! Add a domain module here when its first workflow is extracted; avoid empty
//! service wrappers that only forward arguments to existing route code.

pub mod memory_service;
pub mod summarize;

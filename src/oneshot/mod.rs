//! Run a [`Server`](crate::server::Server) over workspace files on disk,
//! without an LSP client or transport.
//!
//! [`workspace_diagnostics()`] drives the same stateful wrapper as the live
//! server path: it initializes a workspace, opens each matched document, and
//! requests diagnostics once — useful for CLI-style batch runs.
//!
//! The [`server`](crate::server) module is the capability layer (implement
//! [`Server`](crate::server::Server)); `oneshot` is a clientless runner
//! driving the same engine.

mod server;
mod workspace_diagnostics;

pub use workspace_diagnostics::{
    DocumentDiagnostics, WorkspaceDiagnosticConfig, WorkspaceDiagnosticReport,
    workspace_diagnostics,
};

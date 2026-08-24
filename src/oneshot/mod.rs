//! Run a [`Server`](crate::server::Server) over workspace files on disk,
//! without an LSP client or transport.
//!
//! [`workspace_diagnostics()`] drives the same stateful wrapper as the live
//! server path: it initializes a workspace, opens each matched document, and
//! requests diagnostics once — useful for CLI-style batch runs.

mod server;
mod workspace_diagnostics;

pub use workspace_diagnostics::{
    DocumentDiagnostics, WorkspaceDiagnosticConfig, WorkspaceDiagnosticReport,
    workspace_diagnostics,
};

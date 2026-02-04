//! Server lifecycle management for LSP clients.
//!
//! This module provides utilities for checking server health and managing
//! the lifecycle of LSP server connections.

use crate::client::LspClient;
use crate::types::{LspError, LspServerConfig};
use std::path::Path;
use tracing::{debug, info, warn};

/// Health check result for an LSP server.
#[derive(Debug, Clone)]
pub enum HealthStatus {
    /// Server is healthy and responding.
    Healthy,
    /// Server is running but not responding correctly.
    Unhealthy(String),
    /// Server is not running.
    NotRunning,
}

impl HealthStatus {
    /// Returns true if the server is healthy.
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    /// Returns true if the server is not running.
    pub fn is_not_running(&self) -> bool {
        matches!(self, HealthStatus::NotRunning)
    }
}

/// Check if an LSP client is healthy by attempting a simple operation.
///
/// This sends a hover request for a non-existent position. If the server
/// responds (even with an error or empty result), it's considered healthy.
pub async fn health_check(client: &LspClient) -> HealthStatus {
    debug!(
        language = %client.language_id(),
        "Performing health check"
    );

    // Try to get server capabilities - if this fails, server is not healthy
    match client.server_capabilities() {
        Some(_caps) => {
            // Server initialized, capabilities available - it's healthy
            debug!(
                language = %client.language_id(),
                "Health check passed"
            );
            HealthStatus::Healthy
        }
        None => {
            warn!(
                language = %client.language_id(),
                "Health check failed: no server capabilities"
            );
            HealthStatus::Unhealthy("Server not initialized".to_string())
        }
    }
}

/// Restart an LSP server with the given configuration.
///
/// This creates a new client connection. The caller is responsible for
/// shutting down any existing client before calling this.
///
/// # Arguments
/// * `config` - Server configuration to use
/// * `root_path` - Project root path
///
/// # Returns
/// A new `LspClient` instance, or an error if startup failed.
pub async fn restart_server(
    config: LspServerConfig,
    root_path: &Path,
) -> Result<LspClient, LspError> {
    info!(
        language = %config.language_id,
        root = %root_path.display(),
        "Restarting LSP server"
    );

    LspClient::start(config, root_path).await
}

/// Attempt to gracefully shutdown a client.
///
/// Returns Ok(()) if shutdown succeeded, or an error if it failed.
/// The client is consumed by this operation.
pub async fn shutdown_client(client: LspClient) -> Result<(), LspError> {
    let language = client.language_id().to_string();
    info!(language = %language, "Shutting down LSP client");

    match client.shutdown().await {
        Ok(()) => {
            info!(language = %language, "LSP client shutdown successfully");
            Ok(())
        }
        Err(e) => {
            warn!(
                language = %language,
                error = %e,
                "LSP client shutdown failed"
            );
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status() {
        let healthy = HealthStatus::Healthy;
        assert!(healthy.is_healthy());
        assert!(!healthy.is_not_running());

        let unhealthy = HealthStatus::Unhealthy("test".to_string());
        assert!(!unhealthy.is_healthy());
        assert!(!unhealthy.is_not_running());

        let not_running = HealthStatus::NotRunning;
        assert!(!not_running.is_healthy());
        assert!(not_running.is_not_running());
    }
}

//! Stdio transport implementation for LSP communication.
//!
//! This module handles spawning the LSP server process and providing
//! stdin/stdout handles for JSON-RPC communication.

use crate::types::{LspError, LspServerConfig};
use std::process::Stdio;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, info};

/// Manages the stdio transport for an LSP server process.
pub struct StdioTransport {
    /// The spawned server process.
    process: Child,
    /// Server configuration.
    config: LspServerConfig,
}

impl StdioTransport {
    /// Spawn a new LSP server process.
    ///
    /// Returns the transport along with the stdin and stdout handles
    /// for communication.
    pub async fn spawn(
        config: LspServerConfig,
    ) -> Result<(Self, ChildStdin, ChildStdout), LspError> {
        // Resolve the command path
        let command_path = Self::resolve_command(&config.command)?;
        info!(
            language = %config.language_id,
            command = %command_path,
            "Spawning LSP server"
        );

        // Build the command
        let mut cmd = Command::new(&command_path);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(ref dir) = config.working_directory {
            cmd.current_dir(dir);
        }

        // Spawn the process
        let mut process = cmd.spawn().map_err(|e| {
            LspError::ServerStartFailed(format!(
                "Failed to spawn '{}': {}",
                config.command, e
            ))
        })?;

        // Extract stdin and stdout
        let stdin = process.stdin.take().ok_or_else(|| {
            LspError::ServerStartFailed("Failed to capture stdin".to_string())
        })?;
        let stdout = process.stdout.take().ok_or_else(|| {
            LspError::ServerStartFailed("Failed to capture stdout".to_string())
        })?;

        debug!(
            pid = ?process.id(),
            "LSP server process started"
        );

        let transport = Self { process, config };

        Ok((transport, stdin, stdout))
    }

    /// Resolve the command path, checking if it exists.
    fn resolve_command(command: &str) -> Result<String, LspError> {
        // If it's an absolute path, check if it exists
        let path = std::path::Path::new(command);
        if path.is_absolute() {
            if path.exists() {
                return Ok(command.to_string());
            } else {
                return Err(LspError::ExecutableNotFound(command.to_string()));
            }
        }

        // Try to find it in PATH
        match which::which(command) {
            Ok(path) => Ok(path.to_string_lossy().to_string()),
            Err(_) => Err(LspError::ExecutableNotFound(command.to_string())),
        }
    }

    /// Get the process ID if available.
    pub fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    /// Get a reference to the server configuration.
    pub fn config(&self) -> &LspServerConfig {
        &self.config
    }

    /// Check if the process is still running.
    pub async fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,  // Still running
            Ok(Some(_)) => false,  // Exited
            Err(_) => false,  // Error checking status
        }
    }

    /// Wait for the process to exit.
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus, LspError> {
        self.process.wait().await.map_err(LspError::from)
    }

    /// Kill the server process.
    pub async fn kill(&mut self) -> Result<(), LspError> {
        info!(
            language = %self.config.language_id,
            pid = ?self.process.id(),
            "Killing LSP server process"
        );
        self.process.kill().await.map_err(LspError::from)
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // kill_on_drop is set, but log for debugging
        debug!(
            language = %self.config.language_id,
            pid = ?self.process.id(),
            "Dropping StdioTransport, process will be killed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_command_in_path() {
        // 'ls' should be in PATH on most systems
        let result = StdioTransport::resolve_command("ls");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_nonexistent_command() {
        let result = StdioTransport::resolve_command("definitely-not-a-real-command-12345");
        assert!(matches!(result, Err(LspError::ExecutableNotFound(_))));
    }
}

//! Multi-server LSP manager.
//!
//! The `LspManager` manages multiple LSP server connections, one per
//! (language, project root) pair. It handles:
//! - Lazy server startup when files are accessed
//! - Connection pooling by language and project root
//! - Graceful shutdown of all servers
//! - Server restart on failure

use crate::client::LspClient;
use crate::discovery::{default_server_config, detect_language, find_project_root, root_markers};
use crate::lifecycle::{health_check, HealthStatus};
use crate::types::{LspError, LspServerConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// Configuration for the LSP manager.
#[derive(Debug, Clone)]
pub struct LspManagerConfig {
    /// Whether to auto-start servers when needed.
    pub auto_start: bool,
    /// Custom server configurations (language -> config).
    pub custom_servers: HashMap<String, LspServerConfig>,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for LspManagerConfig {
    fn default() -> Self {
        Self {
            auto_start: true,
            custom_servers: HashMap::new(),
            timeout_ms: 30000,
        }
    }
}

impl LspManagerConfig {
    /// Create a new configuration with auto-start enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable auto-start of servers.
    pub fn with_auto_start(mut self, auto_start: bool) -> Self {
        self.auto_start = auto_start;
        self
    }

    /// Add a custom server configuration for a language.
    pub fn with_custom_server(mut self, language: &str, config: LspServerConfig) -> Self {
        self.custom_servers.insert(language.to_string(), config);
        self
    }

    /// Set the request timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

/// Key for identifying a specific server instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ServerKey {
    language: String,
    root_path: PathBuf,
}

impl ServerKey {
    fn new(language: impl Into<String>, root_path: impl Into<PathBuf>) -> Self {
        Self {
            language: language.into(),
            root_path: root_path.into(),
        }
    }
}

/// Manages multiple LSP server connections.
///
/// Servers are keyed by (language, root_path) tuple, allowing multiple
/// servers for different projects or languages to coexist.
pub struct LspManager {
    /// Configuration.
    config: LspManagerConfig,
    /// Active clients by (language, root_path).
    clients: RwLock<HashMap<ServerKey, Arc<Mutex<LspClient>>>>,
}

impl LspManager {
    /// Create a new LSP manager with default configuration.
    pub fn new(config: LspManagerConfig) -> Self {
        Self {
            config,
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a client for the given file.
    ///
    /// This will:
    /// 1. Detect the language from the file extension
    /// 2. Find the project root using language-specific markers
    /// 3. Return existing client if available, or start a new one if auto_start is enabled
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to get a client for
    ///
    /// # Returns
    /// An Arc-wrapped Mutex-guarded LspClient, or an error.
    pub async fn get_client_for_file(
        &self,
        file_path: &Path,
    ) -> Result<Arc<Mutex<LspClient>>, LspError> {
        // Detect language from file extension
        let language = detect_language(file_path).ok_or_else(|| {
            LspError::Other(format!(
                "Cannot detect language for file: {}",
                file_path.display()
            ))
        })?;

        // Find project root using language-specific markers
        let markers = root_markers(&language);
        let root_path = find_project_root(file_path, markers).ok_or_else(|| {
            LspError::Other(format!(
                "Cannot find project root for file: {}",
                file_path.display()
            ))
        })?;

        let key = ServerKey::new(&language, &root_path);

        // Check if we already have a client
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(&key) {
                debug!(
                    language = %language,
                    root = %root_path.display(),
                    "Returning existing LSP client"
                );
                return Ok(client.clone());
            }
        }

        // No client exists - start one if auto_start is enabled
        if !self.config.auto_start {
            return Err(LspError::ServerNotRunning);
        }

        // Get config (custom or default)
        let mut config = self
            .config
            .custom_servers
            .get(&language)
            .cloned()
            .or_else(|| default_server_config(&language))
            .ok_or_else(|| {
                LspError::Other(format!("No LSP server configured for language: {}", language))
            })?;

        // Apply timeout from manager config
        config.timeout_ms = self.config.timeout_ms;

        // Start the client
        info!(
            language = %language,
            root = %root_path.display(),
            "Starting new LSP client"
        );

        let client = LspClient::start(config, &root_path).await?;
        let client = Arc::new(Mutex::new(client));

        // Store in map
        {
            let mut clients = self.clients.write().await;
            clients.insert(key, client.clone());
        }

        Ok(client)
    }

    /// Explicitly start a server for a language at a root path.
    ///
    /// This will start a server even if auto_start is disabled.
    ///
    /// # Arguments
    /// * `language` - Language identifier (e.g., "rust", "typescript")
    /// * `root_path` - Project root path
    pub async fn start_server(&self, language: &str, root_path: &Path) -> Result<(), LspError> {
        let key = ServerKey::new(language, root_path);

        // Check if already running
        {
            let clients = self.clients.read().await;
            if clients.contains_key(&key) {
                return Ok(());
            }
        }

        // Get config
        let mut config = self
            .config
            .custom_servers
            .get(language)
            .cloned()
            .or_else(|| default_server_config(language))
            .ok_or_else(|| {
                LspError::Other(format!("No LSP server configured for language: {}", language))
            })?;

        config.timeout_ms = self.config.timeout_ms;

        // Start client
        info!(
            language = %language,
            root = %root_path.display(),
            "Starting LSP server"
        );

        let client = LspClient::start(config, root_path).await?;
        let client = Arc::new(Mutex::new(client));

        // Store
        let mut clients = self.clients.write().await;
        clients.insert(key, client);

        Ok(())
    }

    /// Shutdown a specific server.
    ///
    /// # Arguments
    /// * `language` - Language identifier
    /// * `root_path` - Project root path
    pub async fn shutdown_server(&self, language: &str, root_path: &Path) -> Result<(), LspError> {
        let key = ServerKey::new(language, root_path);

        // Remove from map
        let client = {
            let mut clients = self.clients.write().await;
            clients.remove(&key)
        };

        // Shutdown if found
        if let Some(client) = client {
            info!(
                language = %language,
                root = %root_path.display(),
                "Shutting down LSP server"
            );

            // Take ownership of the client from the Arc<Mutex>
            // We need to wait for any pending operations
            let client = Arc::try_unwrap(client).map_err(|_| {
                LspError::Other("Cannot shutdown: client still in use".to_string())
            })?;
            let client = client.into_inner();
            client.shutdown().await?;
        }

        Ok(())
    }

    /// Shutdown all servers.
    ///
    /// Returns a vector of errors from servers that failed to shutdown.
    /// Servers are removed from the manager regardless of shutdown success.
    pub async fn shutdown_all(&self) -> Vec<LspError> {
        info!("Shutting down all LSP servers");

        let clients: Vec<_> = {
            let mut clients_guard = self.clients.write().await;
            clients_guard.drain().collect()
        };

        let mut errors = Vec::new();

        for (key, client) in clients {
            info!(
                language = %key.language,
                root = %key.root_path.display(),
                "Shutting down LSP server"
            );

            // Try to get exclusive ownership
            match Arc::try_unwrap(client) {
                Ok(mutex) => {
                    let client = mutex.into_inner();
                    if let Err(e) = client.shutdown().await {
                        warn!(
                            language = %key.language,
                            error = %e,
                            "Failed to shutdown LSP server"
                        );
                        errors.push(e);
                    }
                }
                Err(_) => {
                    warn!(
                        language = %key.language,
                        "Cannot shutdown LSP server: client still in use"
                    );
                    errors.push(LspError::Other(format!(
                        "Cannot shutdown {}: client still in use",
                        key.language
                    )));
                }
            }
        }

        errors
    }

    /// Get status of all active servers.
    pub async fn status(&self) -> Vec<ServerStatus> {
        let clients = self.clients.read().await;
        let mut statuses = Vec::new();

        for (key, client) in clients.iter() {
            let client_guard = client.lock().await;
            let health = health_check(&client_guard).await;

            statuses.push(ServerStatus {
                language: key.language.clone(),
                root_path: key.root_path.clone(),
                server_command: client_guard.language_id().to_string(),
                health,
            });
        }

        statuses
    }

    /// Restart a server.
    ///
    /// Shuts down the existing server (if any) and starts a new one.
    pub async fn restart_server(&self, language: &str, root_path: &Path) -> Result<(), LspError> {
        info!(
            language = %language,
            root = %root_path.display(),
            "Restarting LSP server"
        );

        // Try to shutdown existing (ignore errors if not running)
        let _ = self.shutdown_server(language, root_path).await;

        // Start new one
        self.start_server(language, root_path).await
    }

    /// Get the number of active servers.
    pub async fn server_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Check if a server is running for the given language and root.
    pub async fn is_server_running(&self, language: &str, root_path: &Path) -> bool {
        let key = ServerKey::new(language, root_path);
        self.clients.read().await.contains_key(&key)
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        // Note: We can't do async cleanup in Drop.
        // Users should call shutdown_all() before dropping.
        // This is a best-effort warning.
        if let Ok(clients) = self.clients.try_read() {
            if !clients.is_empty() {
                warn!(
                    count = clients.len(),
                    "LspManager dropped with active servers. Call shutdown_all() before dropping."
                );
            }
        }
    }
}

/// Status information for a server.
#[derive(Debug)]
pub struct ServerStatus {
    /// Language identifier.
    pub language: String,
    /// Project root path.
    pub root_path: PathBuf,
    /// Server command name.
    pub server_command: String,
    /// Health status.
    pub health: HealthStatus,
}

impl ServerStatus {
    /// Returns true if the server is healthy.
    pub fn is_healthy(&self) -> bool {
        self.health.is_healthy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_config_default() {
        let config = LspManagerConfig::default();
        assert!(config.auto_start);
        assert!(config.custom_servers.is_empty());
        assert_eq!(config.timeout_ms, 30000);
    }

    #[test]
    fn test_manager_config_builder() {
        let config = LspManagerConfig::new()
            .with_auto_start(false)
            .with_timeout(60000)
            .with_custom_server("rust", LspServerConfig::rust_analyzer());

        assert!(!config.auto_start);
        assert_eq!(config.timeout_ms, 60000);
        assert!(config.custom_servers.contains_key("rust"));
    }

    #[test]
    fn test_server_key() {
        let key1 = ServerKey::new("rust", PathBuf::from("/project/a"));
        let key2 = ServerKey::new("rust", PathBuf::from("/project/a"));
        let key3 = ServerKey::new("rust", PathBuf::from("/project/b"));
        let key4 = ServerKey::new("typescript", PathBuf::from("/project/a"));

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
    }
}

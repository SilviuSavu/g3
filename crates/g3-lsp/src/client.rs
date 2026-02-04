//! LSP client implementation for a single language server connection.
//!
//! This module provides `LspClient` which manages the lifecycle of an LSP server
//! and provides methods for common LSP operations like goto definition, find references, etc.

use crate::transport::StdioTransport;
use crate::types::{path_to_uri, LspError, LspLocation, LspPosition, LspServerConfig};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientCapabilities, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability,
    ImplementationProviderCapability, InitializeParams, InitializedParams, Location,
    OneOf, ReferenceContext, ReferenceParams, ServerCapabilities, SymbolInformation,
    TextDocumentClientCapabilities, TextDocumentIdentifier, TextDocumentPositionParams,
    WindowClientCapabilities, WorkspaceSymbolParams,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, trace, warn};
use url::Url;

/// JSON-RPC request structure.
#[derive(Debug, Serialize)]
struct JsonRpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    id: i64,
    method: &'static str,
    params: T,
}

/// JSON-RPC notification structure (no id).
#[derive(Debug, Serialize)]
struct JsonRpcNotification<T: Serialize> {
    jsonrpc: &'static str,
    method: &'static str,
    params: T,
}

/// JSON-RPC response structure.
#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<i64>,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC error structure.
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Pending request waiting for response.
struct PendingRequest {
    sender: tokio::sync::oneshot::Sender<String>,
}

/// LSP client for a single language server connection.
pub struct LspClient {
    /// Server configuration.
    config: LspServerConfig,
    /// The transport handling the server process.
    transport: StdioTransport,
    /// Stdin for writing to server.
    stdin: Arc<Mutex<ChildStdin>>,
    /// Next request ID.
    next_id: AtomicI64,
    /// Pending requests waiting for responses.
    pending: Arc<RwLock<HashMap<i64, PendingRequest>>>,
    /// Server capabilities received during initialization.
    server_capabilities: Option<ServerCapabilities>,
    /// Root URI of the workspace.
    root_uri: Url,
    /// Handle to the reader task.
    reader_handle: tokio::task::JoinHandle<()>,
}

impl LspClient {
    /// Start a new LSP client with the given configuration.
    ///
    /// This spawns the LSP server process, performs the initialization handshake,
    /// and returns a ready-to-use client.
    pub async fn start(config: LspServerConfig, root_path: &Path) -> Result<Self, LspError> {
        let root_uri = path_to_uri(root_path)?;

        info!(
            language = %config.language_id,
            root = %root_uri,
            "Starting LSP client"
        );

        // Spawn the server process
        let (transport, stdin, stdout) = StdioTransport::spawn(config.clone()).await?;

        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Arc<RwLock<HashMap<i64, PendingRequest>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Spawn reader task
        let pending_clone = pending.clone();
        let reader_handle = tokio::spawn(Self::reader_loop(stdout, pending_clone));

        let mut client = Self {
            config,
            transport,
            stdin,
            next_id: AtomicI64::new(1),
            pending,
            server_capabilities: None,
            root_uri: root_uri.clone(),
            reader_handle,
        };

        // Perform initialization
        let capabilities = client.initialize(&root_uri).await?;
        client.server_capabilities = Some(capabilities);

        Ok(client)
    }

    /// Background task that reads responses from the server.
    async fn reader_loop(
        stdout: ChildStdout,
        pending: Arc<RwLock<HashMap<i64, PendingRequest>>>,
    ) {
        let mut reader = BufReader::new(stdout);
        let mut headers = String::new();

        loop {
            headers.clear();

            // Read headers until empty line
            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        debug!("LSP server closed stdout");
                        return;
                    }
                    Ok(_) => {
                        let line = line.trim();
                        if line.is_empty() {
                            break;
                        }
                        if let Some(len) = line.strip_prefix("Content-Length: ") {
                            content_length = len.parse().ok();
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Error reading from LSP server");
                        return;
                    }
                }
            }

            // Read content
            let content_length = match content_length {
                Some(len) => len,
                None => {
                    warn!("Missing Content-Length header");
                    continue;
                }
            };

            let mut content = vec![0u8; content_length];
            if let Err(e) = tokio::io::AsyncReadExt::read_exact(&mut reader, &mut content).await {
                warn!(error = %e, "Error reading LSP message content");
                return;
            }

            let content = match String::from_utf8(content) {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "Invalid UTF-8 in LSP message");
                    continue;
                }
            };

            trace!(content = %content, "Received LSP message");

            // Parse as generic JSON to extract id
            let json: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "Invalid JSON in LSP message");
                    continue;
                }
            };

            // Check if this is a response (has id) or notification (no id)
            if let Some(id) = json.get("id").and_then(|v| v.as_i64()) {
                // This is a response
                let mut pending_guard = pending.write().await;
                if let Some(request) = pending_guard.remove(&id) {
                    let _ = request.sender.send(content);
                } else {
                    debug!(id = id, "Received response for unknown request");
                }
            } else {
                // This is a notification - log but ignore for now
                if let Some(method) = json.get("method").and_then(|v| v.as_str()) {
                    trace!(method = method, "Received LSP notification");
                }
            }
        }
    }

    /// Send a request and wait for response.
    async fn request<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &'static str,
        params: P,
    ) -> Result<R, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };

        let body = serde_json::to_string(&request)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        trace!(method = method, id = id, "Sending LSP request");

        // Create channel for response
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending.write().await;
            pending.insert(id, PendingRequest { sender: tx });
        }

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(message.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response with timeout
        let timeout = std::time::Duration::from_millis(self.config.timeout_ms);
        let response_str = tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| LspError::RequestTimeout(self.config.timeout_ms))?
            .map_err(|_| LspError::Other("Response channel closed".to_string()))?;

        // Parse response
        let response: JsonRpcResponse<R> = serde_json::from_str(&response_str)?;

        if let Some(error) = response.error {
            return Err(LspError::JsonRpcError {
                code: error.code,
                message: error.message,
            });
        }

        response
            .result
            .ok_or_else(|| LspError::Other("Response missing result".to_string()))
    }

    /// Send a notification (no response expected).
    async fn notify<P: Serialize>(&self, method: &'static str, params: P) -> Result<(), LspError> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method,
            params,
        };

        let body = serde_json::to_string(&notification)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        trace!(method = method, "Sending LSP notification");

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(message.as_bytes()).await?;
        stdin.flush().await?;

        Ok(())
    }

    /// Perform the LSP initialization handshake.
    async fn initialize(&mut self, root_uri: &Url) -> Result<ServerCapabilities, LspError> {
        debug!("Sending initialize request");

        #[allow(deprecated)]
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_path: None,
            root_uri: Some(root_uri.clone()),
            initialization_options: None,
            capabilities: Self::client_capabilities(),
            trace: None,
            workspace_folders: None,
            client_info: Some(lsp_types::ClientInfo {
                name: "g3-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            locale: None,
            work_done_progress_params: Default::default(),
        };

        let result: lsp_types::InitializeResult = self.request("initialize", params).await?;

        debug!("Sending initialized notification");
        self.notify("initialized", InitializedParams {}).await?;

        info!("LSP server initialized successfully");
        debug!(capabilities = ?result.capabilities, "Server capabilities");

        Ok(result.capabilities)
    }

    /// Build client capabilities to send during initialization.
    fn client_capabilities() -> ClientCapabilities {
        ClientCapabilities {
            workspace: None,
            text_document: Some(TextDocumentClientCapabilities {
                synchronization: None,
                completion: None,
                hover: Some(lsp_types::HoverClientCapabilities {
                    dynamic_registration: Some(false),
                    content_format: Some(vec![
                        lsp_types::MarkupKind::Markdown,
                        lsp_types::MarkupKind::PlainText,
                    ]),
                }),
                signature_help: None,
                references: Some(lsp_types::ReferenceClientCapabilities {
                    dynamic_registration: Some(false),
                }),
                document_highlight: None,
                document_symbol: Some(lsp_types::DocumentSymbolClientCapabilities {
                    dynamic_registration: Some(false),
                    symbol_kind: None,
                    hierarchical_document_symbol_support: Some(true),
                    tag_support: None,
                }),
                formatting: None,
                range_formatting: None,
                on_type_formatting: None,
                declaration: Some(lsp_types::GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(false),
                }),
                definition: Some(lsp_types::GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(false),
                }),
                type_definition: Some(lsp_types::GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(false),
                }),
                implementation: Some(lsp_types::GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(false),
                }),
                code_action: None,
                code_lens: None,
                document_link: None,
                color_provider: None,
                rename: None,
                publish_diagnostics: None,
                folding_range: None,
                selection_range: None,
                linked_editing_range: None,
                call_hierarchy: Some(lsp_types::CallHierarchyClientCapabilities {
                    dynamic_registration: Some(false),
                }),
                semantic_tokens: None,
                moniker: None,
                type_hierarchy: None,
                inline_value: None,
                inlay_hint: None,
                diagnostic: None,
            }),
            window: Some(WindowClientCapabilities {
                work_done_progress: Some(false),
                show_message: None,
                show_document: None,
            }),
            general: None,
            experimental: None,
        }
    }

    /// Gracefully shutdown the LSP server.
    pub async fn shutdown(mut self) -> Result<(), LspError> {
        info!(language = %self.config.language_id, "Shutting down LSP client");

        // Send shutdown request
        let _: () = match self.request("shutdown", ()).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Shutdown request failed");
            }
        };

        // Send exit notification
        if let Err(e) = self.notify("exit", ()).await {
            warn!(error = %e, "Exit notification failed");
        }

        // Abort the reader task
        self.reader_handle.abort();

        // Kill the transport if still running
        if self.transport.is_running().await {
            self.transport.kill().await?;
        }

        Ok(())
    }

    /// Get the server capabilities.
    pub fn server_capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_capabilities.as_ref()
    }

    /// Go to the definition of the symbol at the given position.
    ///
    /// Line and character are 1-indexed (user-facing).
    pub async fn goto_definition(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>, LspError> {
        let uri = path_to_uri(file)?;
        let position = LspPosition::new(line, character).to_lsp_position();

        debug!(
            file = %file.display(),
            line = line,
            character = character,
            "Going to definition"
        );

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response: Option<GotoDefinitionResponse> =
            self.request("textDocument/definition", params).await?;

        Self::parse_definition_response(response)
    }

    /// Find all references to the symbol at the given position.
    ///
    /// Line and character are 1-indexed (user-facing).
    pub async fn find_references(
        &self,
        file: &Path,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Result<Vec<LspLocation>, LspError> {
        let uri = path_to_uri(file)?;
        let position = LspPosition::new(line, character).to_lsp_position();

        debug!(
            file = %file.display(),
            line = line,
            character = character,
            include_declaration = include_declaration,
            "Finding references"
        );

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: ReferenceContext {
                include_declaration,
            },
        };

        let response: Option<Vec<Location>> =
            self.request("textDocument/references", params).await?;

        match response {
            Some(locations) => locations
                .into_iter()
                .map(|loc| LspLocation::from_lsp_location(&loc))
                .collect(),
            None => Ok(Vec::new()),
        }
    }

    /// Get hover information for the symbol at the given position.
    ///
    /// Line and character are 1-indexed (user-facing).
    pub async fn hover(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<Hover>, LspError> {
        // Check if server supports hover
        if let Some(ref caps) = self.server_capabilities {
            match caps.hover_provider {
                None => {
                    return Err(LspError::CapabilityNotSupported("hover".to_string()));
                }
                Some(HoverProviderCapability::Simple(false)) => {
                    return Err(LspError::CapabilityNotSupported("hover".to_string()));
                }
                _ => {}
            }
        }

        let uri = path_to_uri(file)?;
        let position = LspPosition::new(line, character).to_lsp_position();

        debug!(
            file = %file.display(),
            line = line,
            character = character,
            "Getting hover info"
        );

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: Default::default(),
        };

        self.request("textDocument/hover", params).await
    }

    /// Get all symbols in a document.
    pub async fn document_symbols(&self, file: &Path) -> Result<Vec<DocumentSymbol>, LspError> {
        let uri = path_to_uri(file)?;

        debug!(file = %file.display(), "Getting document symbols");

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response: Option<DocumentSymbolResponse> =
            self.request("textDocument/documentSymbol", params).await?;

        match response {
            Some(DocumentSymbolResponse::Flat(symbols)) => {
                // Convert SymbolInformation to DocumentSymbol
                #[allow(deprecated)]
                Ok(symbols
                    .into_iter()
                    .map(|si| DocumentSymbol {
                        name: si.name,
                        detail: None,
                        kind: si.kind,
                        tags: si.tags,
                        deprecated: si.deprecated,
                        range: si.location.range,
                        selection_range: si.location.range,
                        children: None,
                    })
                    .collect())
            }
            Some(DocumentSymbolResponse::Nested(symbols)) => Ok(symbols),
            None => Ok(Vec::new()),
        }
    }

    /// Search for symbols across the workspace.
    pub async fn workspace_symbols(&self, query: &str) -> Result<Vec<SymbolInformation>, LspError> {
        debug!(query = query, "Searching workspace symbols");

        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response: Option<lsp_types::WorkspaceSymbolResponse> =
            self.request("workspace/symbol", params).await?;

        match response {
            Some(lsp_types::WorkspaceSymbolResponse::Flat(symbols)) => Ok(symbols),
            Some(lsp_types::WorkspaceSymbolResponse::Nested(symbols)) => {
                // Convert WorkspaceSymbol to SymbolInformation
                Ok(symbols
                    .into_iter()
                    .filter_map(|ws| {
                        let location = match ws.location {
                            OneOf::Left(loc) => loc,
                            OneOf::Right(doc_link) => Location {
                                uri: doc_link.uri,
                                range: lsp_types::Range::default(),
                            },
                        };
                        Some(SymbolInformation {
                            name: ws.name,
                            kind: ws.kind,
                            tags: ws.tags,
                            #[allow(deprecated)]
                            deprecated: None,
                            location,
                            container_name: ws.container_name,
                        })
                    })
                    .collect())
            }
            None => Ok(Vec::new()),
        }
    }

    /// Go to implementation of an interface or abstract method.
    ///
    /// Line and character are 1-indexed (user-facing).
    pub async fn goto_implementation(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>, LspError> {
        // Check if server supports implementation
        if let Some(ref caps) = self.server_capabilities {
            match caps.implementation_provider {
                None => {
                    return Err(LspError::CapabilityNotSupported(
                        "implementation".to_string(),
                    ));
                }
                Some(ImplementationProviderCapability::Simple(false)) => {
                    return Err(LspError::CapabilityNotSupported(
                        "implementation".to_string(),
                    ));
                }
                _ => {}
            }
        }

        let uri = path_to_uri(file)?;
        let position = LspPosition::new(line, character).to_lsp_position();

        debug!(
            file = %file.display(),
            line = line,
            character = character,
            "Going to implementation"
        );

        let params = lsp_types::request::GotoImplementationParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response: Option<GotoDefinitionResponse> =
            self.request("textDocument/implementation", params).await?;

        Self::parse_definition_response(response)
    }

    /// Prepare call hierarchy at a position.
    ///
    /// Line and character are 1-indexed (user-facing).
    pub async fn prepare_call_hierarchy(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<CallHierarchyItem>, LspError> {
        let uri = path_to_uri(file)?;
        let position = LspPosition::new(line, character).to_lsp_position();

        debug!(
            file = %file.display(),
            line = line,
            character = character,
            "Preparing call hierarchy"
        );

        let params = CallHierarchyPrepareParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: Default::default(),
        };

        let response: Option<Vec<CallHierarchyItem>> = self
            .request("textDocument/prepareCallHierarchy", params)
            .await?;

        Ok(response.unwrap_or_default())
    }

    /// Get incoming calls to a call hierarchy item.
    pub async fn incoming_calls(
        &self,
        item: CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyIncomingCall>, LspError> {
        debug!(name = %item.name, "Getting incoming calls");

        let params = CallHierarchyIncomingCallsParams {
            item,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response: Option<Vec<CallHierarchyIncomingCall>> = self
            .request("callHierarchy/incomingCalls", params)
            .await?;

        Ok(response.unwrap_or_default())
    }

    /// Get outgoing calls from a call hierarchy item.
    pub async fn outgoing_calls(
        &self,
        item: CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyOutgoingCall>, LspError> {
        debug!(name = %item.name, "Getting outgoing calls");

        let params = CallHierarchyOutgoingCallsParams {
            item,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response: Option<Vec<CallHierarchyOutgoingCall>> = self
            .request("callHierarchy/outgoingCalls", params)
            .await?;

        Ok(response.unwrap_or_default())
    }

    /// Parse a GotoDefinitionResponse into a list of locations.
    fn parse_definition_response(
        response: Option<GotoDefinitionResponse>,
    ) -> Result<Vec<LspLocation>, LspError> {
        match response {
            Some(GotoDefinitionResponse::Scalar(loc)) => {
                Ok(vec![LspLocation::from_lsp_location(&loc)?])
            }
            Some(GotoDefinitionResponse::Array(locs)) => {
                locs.iter().map(LspLocation::from_lsp_location).collect()
            }
            Some(GotoDefinitionResponse::Link(links)) => links
                .iter()
                .map(|link| {
                    LspLocation::from_lsp_location(&Location {
                        uri: link.target_uri.clone(),
                        range: link.target_selection_range,
                    })
                })
                .collect(),
            None => Ok(Vec::new()),
        }
    }

    /// Get the language ID for this client.
    pub fn language_id(&self) -> &str {
        &self.config.language_id
    }

    /// Get the root URI of the workspace.
    pub fn root_uri(&self) -> &Url {
        &self.root_uri
    }
}

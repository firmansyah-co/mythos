//! # Model Context Protocol Server Engine
//!
//! Handles JSON-RPC 2.0 routing, tool registration, parameter validation,
//! and invocation dispatching adhering to the MCP 2024-11-05 standard.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::context::McpContext;
use crate::protocol::*;
use crate::McpError;

/// Asynchronous tool handler trait for executing registered MCP tools.
#[async_trait::async_trait]
pub trait McpToolHandler: Send + Sync {
    async fn execute(
        &self,
        ctx: &McpContext,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError>;
}

/// Dynamic function wrapper for closure-based MCP tool handlers.
pub struct FnToolHandler<F> {
    func: F,
}

impl<F> FnToolHandler<F> {
    pub fn new(func: F) -> Self {
        Self { func }
    }
}

#[async_trait::async_trait]
impl<F, Fut> McpToolHandler for FnToolHandler<F>
where
    F: Fn(McpContext, serde_json::Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<CallToolResult, McpError>> + Send + 'static,
{
    async fn execute(
        &self,
        ctx: &McpContext,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        (self.func)(ctx.clone(), args).await
    }
}

/// Registered entry containing tool schema metadata and execution callback.
struct RegisteredTool {
    definition: Tool,
    handler: Arc<dyn McpToolHandler>,
}

/// Core Model Context Protocol (MCP) Server.
pub struct McpServer {
    context: McpContext,
    tools: Arc<RwLock<HashMap<String, RegisteredTool>>>,
    server_info: Implementation,
    instructions: Option<String>,
}

impl McpServer {
    /// Create a new MCP server with the specified execution context.
    pub fn new(context: McpContext) -> Self {
        Self {
            context,
            tools: Arc::new(RwLock::new(HashMap::new())),
            server_info: Implementation {
                name: "mythos-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "MythOS Industrial SCADA & Soft-Logic MCP Server. Provides safe tag access, alarm telemetry, and controller operations."
                    .to_string(),
            ),
        }
    }

    /// Access the underlying McpContext.
    pub fn context(&self) -> &McpContext {
        &self.context
    }

    /// Set custom server instructions.
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Register a tool with a schema definition and handler.
    pub async fn register_tool(&self, tool: Tool, handler: Arc<dyn McpToolHandler>) {
        info!("Registering MCP tool: {}", tool.name);
        let mut tools = self.tools.write().await;
        tools.insert(
            tool.name.clone(),
            RegisteredTool {
                definition: tool,
                handler,
            },
        );
    }

    /// Register a functional async tool handler.
    pub async fn register_fn<F, Fut>(&self, tool: Tool, func: F)
    where
        F: Fn(McpContext, serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<CallToolResult, McpError>> + Send + 'static,
    {
        self.register_tool(tool, Arc::new(FnToolHandler::new(func)))
            .await;
    }

    /// Unregister a tool by name.
    pub async fn unregister_tool(&self, name: &str) -> bool {
        let mut tools = self.tools.write().await;
        tools.remove(name).is_some()
    }

    /// List all currently registered tools.
    pub async fn list_tools(&self) -> Vec<Tool> {
        let tools = self.tools.read().await;
        tools.values().map(|r| r.definition.clone()).collect()
    }

    /// Dispatch and process a JSON-RPC request, returning an optional JSON-RPC response.
    /// Notifications (request without ID) return None.
    pub async fn handle_request(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let is_notification = req.is_notification();
        let id = req.id.clone();

        match req.method.as_str() {
            "initialize" => {
                debug!("MCP initialize requested");
                let result = InitializeResult {
                    protocol_version: MCP_PROTOCOL_VERSION.to_string(),
                    capabilities: ServerCapabilities {
                        tools: Some(ToolsCapability {
                            list_changed: Some(true),
                        }),
                        logging: Some(serde_json::json!({})),
                        prompts: None,
                        resources: None,
                    },
                    server_info: self.server_info.clone(),
                    instructions: self.instructions.clone(),
                };
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(result).unwrap_or_default(),
                ))
            }
            "notifications/initialized" => {
                info!("Client initialized notification received");
                None
            }
            "ping" => {
                debug!("MCP ping received");
                Some(JsonRpcResponse::success(id, serde_json::json!({})))
            }
            "tools/list" => {
                debug!("MCP tools/list requested");
                let tool_list = self.list_tools().await;
                let result = ListToolsResult {
                    tools: tool_list,
                    next_cursor: None,
                };
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(result).unwrap_or_default(),
                ))
            }
            "tools/call" => {
                if is_notification {
                    warn!("tools/call sent as notification; ignoring");
                    return None;
                }

                let params: CallToolParams = match req.params {
                    Some(p) => match serde_json::from_value(p) {
                        Ok(call_params) => call_params,
                        Err(e) => {
                            return Some(JsonRpcResponse::invalid_params(
                                id,
                                format!("Invalid tools/call parameters: {}", e),
                            ));
                        }
                    },
                    None => {
                        return Some(JsonRpcResponse::invalid_params(
                            id,
                            "Missing params for tools/call",
                        ));
                    }
                };

                let handler_and_tool = {
                    let tools = self.tools.read().await;
                    tools
                        .get(&params.name)
                        .map(|r| (r.handler.clone(), r.definition.clone()))
                };

                match handler_and_tool {
                    Some((handler, _tool)) => {
                        let args = params
                            .arguments
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        match handler.execute(&self.context, args).await {
                            Ok(tool_result) => Some(JsonRpcResponse::success(
                                id,
                                serde_json::to_value(tool_result).unwrap_or_default(),
                            )),
                            Err(e) => {
                                // In MCP, tool errors can be returned as CallToolResult with isError: true
                                // or as JSON-RPC error. We provide CallToolResult error for agent visibility.
                                let res = CallToolResult::error(format!("{}", e));
                                Some(JsonRpcResponse::success(
                                    id,
                                    serde_json::to_value(res).unwrap_or_default(),
                                ))
                            }
                        }
                    }
                    None => Some(JsonRpcResponse::error(
                        id,
                        JsonRpcError::new(
                            METHOD_NOT_FOUND,
                            format!("Tool '{}' not found", params.name),
                        ),
                    )),
                }
            }
            other => {
                if is_notification {
                    debug!("Ignoring unhandled notification: {}", other);
                    None
                } else {
                    Some(JsonRpcResponse::method_not_found(id, other))
                }
            }
        }
    }

    /// Process a raw string JSON message and return JSON string response if applicable.
    pub async fn handle_raw_json(&self, raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let err_resp = JsonRpcResponse::parse_error(format!("JSON parse error: {}", e));
                return serde_json::to_string(&err_resp).ok();
            }
        };

        let response = self.handle_request(request).await?;
        serde_json::to_string(&response).ok()
    }
}

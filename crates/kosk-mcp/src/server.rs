//! MCP server: embeds egui_mcp UiServer + kosk controller tools over stdio.

use std::sync::Arc;
use std::time::Duration;

use egui_mcp::{Bridge, UiServer};
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, InitializeRequestParams,
        InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo,
        Tool,
    },
    schemars,
    service::{RequestContext, RoleServer},
    tool, tool_router, transport, ErrorData as McpError, ServerHandler, ServiceExt as _,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

const INSTRUCTIONS: &str = r#"kosk-mcp drives a live kosk process: egui UI tools (via egui inspection) plus a virtual controller.

Prerequisites:
- Launch kosk with `EGUI_INSPECTION=1` and MCP-controller mode (`--mcp-controller` or `KOSK_CONTROLLER_MCP=1`).
- Physical/replay input is disabled for that process; only this MCP server controls sticks/pads/buttons.
- Attach UI tools to `127.0.0.1:5719`. Controller tools talk to `127.0.0.1:5720` (override with `KOSK_CONTROLLER_ADDR`).

Controller semantics:
- Sticks and pads are latent holds until `release_*` / `controller_neutral` (or optional `duration_ms` expiry).
- Prefer observe → act → verify (query_tree / get_controller_state / screenshot).
- Keyboard stick targets: wait for Keyboard first frame → `geometry_ready` → `stick_for_key` → `set_stick` / pad or trigger tap. Returned sticks are post-map −1..1 (same space as `set_stick`).
"#;

fn text_error(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AttachArgs {
    /// Host the app's inspection port is on. Defaults to `127.0.0.1`.
    #[serde(default = "default_host")]
    host: String,
    /// Port the app is listening on. Defaults to `5719`.
    #[serde(default = "default_port")]
    port: u16,
    /// How long to keep retrying the connection, in seconds. Defaults to 10.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_port() -> u16 {
    5719
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct EmptyArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
struct StickArgs {
    side: String,
    x: f32,
    y: f32,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SideArgs {
    side: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PadArgs {
    side: String,
    x: f32,
    y: f32,
    touching: bool,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ButtonArgs {
    button: String,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ButtonOnlyArgs {
    button: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TriggerArgs {
    side: String,
    /// `0..=255`, or omit/null to clear.
    #[serde(default)]
    value: Option<u8>,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct LayoutOptArgs {
    #[serde(default)]
    layout: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct KeyQueryArgs {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    row: Option<usize>,
    #[serde(default)]
    col: Option<usize>,
    #[serde(default)]
    layout: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct StickForKeyArgs {
    side: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    row: Option<usize>,
    #[serde(default)]
    col: Option<usize>,
    /// Defaults to true when omitted.
    #[serde(default)]
    include_range: Option<bool>,
    #[serde(default)]
    layout: Option<String>,
}

fn controller_addr() -> String {
    std::env::var("KOSK_CONTROLLER_ADDR").unwrap_or_else(|_| "127.0.0.1:5720".to_owned())
}

async fn controller_rpc(mut body: Value) -> Result<Value, String> {
    if body.get("id").is_none() {
        body.as_object_mut()
            .ok_or_else(|| "controller request must be a JSON object".to_owned())?
            .insert("id".into(), json!(1));
    }
    let addr = controller_addr();
    let mut stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("connect {addr}: {e}"))?;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let line = serde_json::to_string(&body).map_err(|e| e.to_string())? + "\n";
    stream
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("write: {e}"))?;
    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    reader
        .read_line(&mut resp)
        .await
        .map_err(|e| format!("read: {e}"))?;
    serde_json::from_str(resp.trim()).map_err(|e| format!("bad response: {e}"))
}

async fn controller_tool(body: Value) -> Result<CallToolResult, McpError> {
    match controller_rpc(body).await {
        Ok(v) => Ok(CallToolResult::structured(v)),
        Err(e) => Ok(text_error(e)),
    }
}

#[derive(Clone)]
pub struct Server {
    ui: Arc<Mutex<Option<UiServer>>>,
    peer: Arc<Mutex<Option<Value>>>,
    ui_router: ToolRouter<UiServer>,
    lifecycle_router: ToolRouter<Self>,
    controller_router: ToolRouter<Self>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            ui: Arc::new(Mutex::new(None)),
            peer: Arc::new(Mutex::new(None)),
            ui_router: UiServer::router(),
            lifecycle_router: Self::lifecycle_router(),
            controller_router: Self::controller_router(),
        }
    }

    fn tools(&self) -> Vec<Tool> {
        let mut tools = self.lifecycle_router.list_all();
        tools.extend(self.controller_router.list_all());
        tools.extend(self.ui_router.list_all());
        tools
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

/// Serve the MCP tool set on stdio until the client disconnects.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::new();
    let running = server.serve(transport::stdio()).await?;
    let _reason = running.waiting().await?;
    Ok(())
}

#[tool_router(router = lifecycle_router)]
impl Server {
    /// Connect to a running egui app's inspection port. Defaults to 127.0.0.1:5719.
    #[tool]
    async fn attach(
        &self,
        Parameters(args): Parameters<AttachArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut guard = self.ui.lock().await;
        if guard.is_some() {
            return Ok(text_error(
                "already connected — call `disconnect` first before attaching again",
            ));
        }
        let timeout = args.timeout_secs.map(Duration::from_secs);
        let bridge = match Bridge::connect(&args.host, args.port, timeout).await {
            Ok(b) => b,
            Err(e) => return Ok(text_error(format!("attach failed: {e}"))),
        };
        let info = serde_json::to_value(&bridge.peer_info).unwrap_or(json!({}));
        *self.peer.lock().await = Some(info.clone());
        *guard = Some(UiServer::new(bridge));
        Ok(CallToolResult::structured(
            json!({ "ok": true, "attached": info }),
        ))
    }

    /// Disconnect from the attached app.
    #[tool]
    async fn disconnect(&self, _p: Parameters<EmptyArgs>) -> Result<CallToolResult, McpError> {
        *self.peer.lock().await = None;
        if self.ui.lock().await.take().is_some() {
            Ok(CallToolResult::structured(json!({ "ok": true })))
        } else {
            Ok(text_error("no app connected"))
        }
    }

    /// Report whether an app is connected and its peer info.
    #[tool]
    async fn status(&self, _p: Parameters<EmptyArgs>) -> Result<CallToolResult, McpError> {
        let body = match self.peer.lock().await.as_ref() {
            None => json!({ "state": "idle" }),
            Some(peer) => json!({ "state": "connected", "peer": peer }),
        };
        Ok(CallToolResult::structured(body))
    }
}

#[tool_router(router = controller_router)]
impl Server {
    /// Ping the kosk controller port and return current virtual state.
    #[tool]
    async fn controller_status(
        &self,
        _p: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let ping = controller_rpc(json!({"cmd": "ping"})).await;
        match ping {
            Err(e) => Ok(text_error(e)),
            Ok(_) => match controller_rpc(json!({"cmd": "get_state"})).await {
                Ok(state) => Ok(CallToolResult::structured(
                    json!({ "ok": true, "state": state }),
                )),
                Err(e) => Ok(text_error(e)),
            },
        }
    }

    /// Clear all virtual controller inputs.
    #[tool]
    async fn controller_neutral(
        &self,
        _p: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, McpError> {
        controller_tool(json!({"cmd": "neutral"})).await
    }

    /// Latch a stick axis (hold-at until release/neutral/duration).
    #[tool]
    async fn set_stick(
        &self,
        Parameters(args): Parameters<StickArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({
            "cmd": "set_stick",
            "side": args.side,
            "x": args.x,
            "y": args.y,
        });
        if let Some(ms) = args.duration_ms {
            body["duration_ms"] = json!(ms);
        }
        controller_tool(body).await
    }

    /// Release a stick to 0,0.
    #[tool]
    async fn release_stick(
        &self,
        Parameters(args): Parameters<SideArgs>,
    ) -> Result<CallToolResult, McpError> {
        controller_tool(json!({"cmd": "release_stick", "side": args.side})).await
    }

    /// Latch a touchpad (explicit touching + XY). Hold-at until release/neutral/duration.
    #[tool]
    async fn set_pad(
        &self,
        Parameters(args): Parameters<PadArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({
            "cmd": "set_pad",
            "side": args.side,
            "x": args.x,
            "y": args.y,
            "touching": args.touching,
        });
        if let Some(ms) = args.duration_ms {
            body["duration_ms"] = json!(ms);
        }
        controller_tool(body).await
    }

    /// Release a pad (touching=false, 0,0).
    #[tool]
    async fn release_pad(
        &self,
        Parameters(args): Parameters<SideArgs>,
    ) -> Result<CallToolResult, McpError> {
        controller_tool(json!({"cmd": "release_pad", "side": args.side})).await
    }

    /// Press a controller button (latched until release or duration).
    #[tool]
    async fn press_button(
        &self,
        Parameters(args): Parameters<ButtonArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({"cmd": "press", "button": args.button});
        if let Some(ms) = args.duration_ms {
            body["duration_ms"] = json!(ms);
        }
        controller_tool(body).await
    }

    /// Release a controller button.
    #[tool]
    async fn release_button(
        &self,
        Parameters(args): Parameters<ButtonOnlyArgs>,
    ) -> Result<CallToolResult, McpError> {
        controller_tool(json!({"cmd": "release", "button": args.button})).await
    }

    /// Tap a button (press then auto-release; default duration_ms=50).
    #[tool]
    async fn tap_button(
        &self,
        Parameters(args): Parameters<ButtonArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({"cmd": "tap", "button": args.button});
        if let Some(ms) = args.duration_ms {
            body["duration_ms"] = json!(ms);
        }
        controller_tool(body).await
    }

    /// Set a trigger value (`0..=255`) or clear with null.
    #[tool]
    async fn set_trigger(
        &self,
        Parameters(args): Parameters<TriggerArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({
            "cmd": "set_trigger",
            "side": args.side,
            "value": args.value,
        });
        if let Some(ms) = args.duration_ms {
            body["duration_ms"] = json!(ms);
        }
        controller_tool(body).await
    }

    /// Return current virtual controller state from kosk.
    #[tool]
    async fn get_controller_state(
        &self,
        _p: Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, McpError> {
        controller_tool(json!({"cmd": "get_state"})).await
    }

    /// Whether keyboard centres are captured (after first Keyboard draw).
    #[tool]
    async fn geometry_ready(&self, _p: Parameters<EmptyArgs>) -> Result<CallToolResult, McpError> {
        controller_tool(json!({"cmd": "geometry_ready"})).await
    }

    /// List non-Skip keys in the published geometry snapshot.
    #[tool]
    async fn list_keys(
        &self,
        Parameters(args): Parameters<LayoutOptArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({"cmd": "list_keys"});
        if let Some(layout) = args.layout {
            body["layout"] = json!(layout);
        }
        controller_tool(body).await
    }

    /// Fetch one key's centre/hitbox/rest/scale info.
    #[tool]
    async fn get_key(
        &self,
        Parameters(args): Parameters<KeyQueryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({"cmd": "get_key"});
        if let Some(key) = args.key {
            body["key"] = json!(key);
        }
        if let Some(row) = args.row {
            body["row"] = json!(row);
        }
        if let Some(col) = args.col {
            body["col"] = json!(col);
        }
        if let Some(layout) = args.layout {
            body["layout"] = json!(layout);
        }
        controller_tool(body).await
    }

    /// Post-map −1..1 stick target for a key on left|right stick.
    #[tool]
    async fn stick_for_key(
        &self,
        Parameters(args): Parameters<StickForKeyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut body = json!({
            "cmd": "stick_for_key",
            "side": args.side,
        });
        if let Some(key) = args.key {
            body["key"] = json!(key);
        }
        if let Some(row) = args.row {
            body["row"] = json!(row);
        }
        if let Some(col) = args.col {
            body["col"] = json!(col);
        }
        if let Some(include_range) = args.include_range {
            body["include_range"] = json!(include_range);
        }
        if let Some(layout) = args.layout {
            body["layout"] = json!(layout);
        }
        controller_tool(body).await
    }
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("kosk-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if self.lifecycle_router.has_route(&request.name) {
            let tcc = ToolCallContext::new(self, request, context);
            return self.lifecycle_router.call(tcc).await;
        }
        if self.controller_router.has_route(&request.name) {
            let tcc = ToolCallContext::new(self, request, context);
            return self.controller_router.call(tcc).await;
        }
        let guard = self.ui.lock().await;
        let Some(ui) = guard.as_ref() else {
            return Ok(text_error("no app connected — call `attach` first"));
        };
        ui.dispatch(&self.ui_router, request, context).await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        if let Some(tool) = self.lifecycle_router.get(name) {
            return Some(tool.clone());
        }
        if let Some(tool) = self.controller_router.get(name) {
            return Some(tool.clone());
        }
        self.ui_router.get(name).cloned()
    }
}

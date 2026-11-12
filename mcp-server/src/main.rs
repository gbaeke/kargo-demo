//! A demo MCP server with two tools, `current_time` and `magic_8_ball`,
//! served over streamable HTTP at /mcp. It is promoted with Kargo like the agents, and the agents
//! reach it through the AI gateway only.
//!
//! Each tool's answer names the server's version and stage, so it is visible
//! which MCP server an agent ended up talking to.
//!
//! APP_VERSION is baked into the image (the image tag); STAGE comes from the
//! stage's kustomize overlay.

use std::env;

use chrono::{FixedOffset, Utc};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Deserialize;
use tokio::signal::unix::{SignalKind, signal};

fn var(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.into())
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TimeArgs {
    /// Hours ahead of UTC, from -12 to 14, e.g. 1 for Brussels in winter. Defaults to 0 (UTC).
    pub utc_offset_hours: Option<i32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EightBallArgs {
    /// The yes-or-no question to put to the ball.
    pub question: String,
}

/// The twenty answers of the classic toy: ten yes, five vague, five no.
const EIGHT_BALL: [&str; 20] = [
    "It is certain.",
    "It is decidedly so.",
    "Without a doubt.",
    "Yes, definitely.",
    "You may rely on it.",
    "As I see it, yes.",
    "Most likely.",
    "Outlook good.",
    "Yes.",
    "Signs point to yes.",
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Outlook not so good.",
    "Very doubtful.",
];

#[derive(Clone)]
pub struct Toolbox {
    tool_router: ToolRouter<Toolbox>,
}

#[tool_router]
impl Toolbox {
    fn new() -> Self {
        Self { tool_router: Self::tool_router() }
    }

    #[tool(description = "The current date, time and weekday. A model cannot know these by itself. \
                          Takes an optional offset from UTC in whole hours.")]
    fn current_time(&self, Parameters(args): Parameters<TimeArgs>) -> Result<CallToolResult, ErrorData> {
        let hours = args.utc_offset_hours.unwrap_or(0);
        let Some(offset) = (-12..=14).contains(&hours).then(|| FixedOffset::east_opt(hours * 3600)).flatten() else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "utc_offset_hours must be between -12 and 14, not {hours}"
            ))]));
        };
        let now = Utc::now().with_timezone(&offset);
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{} (answered by mcp-server {} in {})",
            now.format("%A %Y-%m-%d %H:%M:%S UTC%:z"),
            var("APP_VERSION", "dev-build"),
            var("STAGE", "local"),
        ))]))
    }

    #[tool(description = "Shake a Magic 8 Ball: a random, non-binding answer to a yes-or-no question. \
                          Use it when the user asks for the 8 ball, or wants a decision left to chance. \
                          Report the ball's answer word for word.")]
    fn magic_8_ball(&self, Parameters(args): Parameters<EightBallArgs>) -> Result<CallToolResult, ErrorData> {
        // The clock's nanoseconds are random enough for a toy.
        let pick = Utc::now().timestamp_subsec_nanos() as usize % EIGHT_BALL.len();
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "Asked: \"{}\". The ball says: {} (answered by mcp-server {} in {})",
            args.question,
            EIGHT_BALL[pick],
            var("APP_VERSION", "dev-build"),
            var("STAGE", "local"),
        ))]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Toolbox {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::LATEST)
            .with_server_info(Implementation::new("kargo-demo-tools", var("APP_VERSION", "dev-build")))
            .with_instructions("Demo tools: the current date and time, and a Magic 8 Ball.")
    }
}

#[tokio::main]
async fn main() {
    // Installed first: as PID 1 in a container the process ignores SIGTERM without a handler.
    let mut term = signal(SignalKind::terminate()).expect("signal handler");
    let addr = var("MCP_ADDR", "127.0.0.1:8081");
    // Host names clients may use to reach the server (DNS-rebinding
    // protection). Loopback only by default; in the cluster it is the Service
    // name the gateway calls.
    let allowed_hosts = var("MCP_ALLOWED_HOSTS", "localhost,127.0.0.1,::1");
    // Stateless: no MCP session is kept between requests, so any replica can
    // serve any request and a restart loses nothing.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(allowed_hosts.split(',').map(str::trim).filter(|host| !host.is_empty()));
    let service = StreamableHttpService::new(|| Ok(Toolbox::new()), LocalSessionManager::default().into(), config);
    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .route("/healthz", axum::routing::get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("mcp-server {} ({}) listening on http://{addr}/mcp", var("APP_VERSION", "dev-build"), var("STAGE", "local"));
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            term.recv().await;
        })
        .await
        .expect("server");
}

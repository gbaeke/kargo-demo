//! The helpdesk agent: answers one question at a time with a model, in the
//! role of a first-line IT helpdesk. No chat history: every message is a
//! fresh request.
//!
//! It holds no provider key and knows no tool server. Everything goes through
//! the AI gateway:
//!   - the model: an OpenAI-style chat request to GATEWAY_URL, for MODEL
//!   - the tools: an MCP server the gateway serves at MCP_URL
//! The model may ask for a tool; the agent calls it over MCP, hands the result
//! back and lets the model finish. Without MCP_URL, or when the tools cannot
//! be reached, it answers without tools and says so.
//!
//! APP_VERSION is baked into the image (the image tag); STAGE comes from the
//! stage's kustomize overlay. The page shows both, so a promotion is visible.

use std::env;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, ProtocolVersion};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::signal::unix::{SignalKind, signal};

const NAME: &str = "helpdesk";
const SYSTEM_PROMPT: &str = "You are a first-line IT helpdesk agent. Give the most likely cause \
of the user's problem and up to three concrete steps to try, in plain language. If you cannot \
tell what the problem is, ask one clarifying question. Keep it under 120 words. \
If you have a tool that gives the current date or time, call it whenever the answer depends on \
today's date, the day of the week or the time; never guess those.";

/// The model gets this many chances to ask for tools before it must answer.
const MAX_TOOL_ROUNDS: usize = 4;

fn var(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.into())
}

fn version() -> String {
    var("APP_VERSION", "dev-build")
}

fn stage() -> String {
    var("STAGE", "local")
}

fn model() -> String {
    var("MODEL", "gpt-6-luna")
}

/// Where the gateway serves the MCP server, if this agent has tools at all.
fn mcp_url() -> Option<String> {
    env::var("MCP_URL").ok().filter(|url| !url.is_empty())
}

async fn index() -> Html<String> {
    let (version, stage, model) = (version(), stage(), model());
    let color = match stage.as_str() {
        "prod" => "#b3261e",
        "test" => "#9a6700",
        _ => "#1b6e3c",
    };
    Html(format!(
        r#"<!doctype html>
<meta charset="utf-8"><title>{NAME} {version} ({stage})</title>
<body style="font-family: system-ui; max-width: 40rem; margin: 4rem auto; padding: 0 1rem">
<h1>Helpdesk agent</h1>
<p>version <b>{version}</b> in stage <b style="color: {color}">{stage}</b>, model <b>{model}</b></p>
<form onsubmit="ask(event)"><input id="q" placeholder="Describe your problem" style="width: 70%"> <button id="b">Send</button></form>
<pre id="a" style="white-space: pre-wrap"></pre>
<pre id="t" style="white-space: pre-wrap; color: #666; font-size: .85em"></pre>
<script>
async function ask(e) {{
  e.preventDefault();
  const a = document.getElementById('a'), t = document.getElementById('t'), b = document.getElementById('b');
  a.textContent = 'Thinking...'; t.textContent = ''; b.disabled = true;
  try {{
    const r = await fetch('/chat', {{method: 'POST', headers: {{'content-type': 'application/json'}},
      body: JSON.stringify({{message: document.getElementById('q').value}})}});
    const d = await r.json();
    a.textContent = d.reply;
    const lines = (d.tools || []).map(u => 'tool ' + u.name + '(' + JSON.stringify(u.arguments) + ') -> ' + u.result);
    if (d.note) lines.push(d.note);
    t.textContent = lines.join('\n');
  }} catch (err) {{ a.textContent = 'Request failed: ' + err; }}
  b.disabled = false;
}}
</script>"#
    ))
}

#[derive(Deserialize)]
struct Chat {
    message: String,
}

/// What an answer consists of: the text, each tool call made on the way, and
/// a note when the tools could not be used.
struct Answer {
    reply: String,
    tools: Vec<Value>,
    note: Option<String>,
}

/// One chat completion through the gateway. Returns the model's message.
async fn complete(http: &reqwest::Client, messages: &[Value], tools: &[Value]) -> Result<Value, String> {
    let url = format!("{}/v1/chat/completions", var("GATEWAY_URL", "http://127.0.0.1:4000"));
    let mut request = json!({ "model": model(), "messages": messages });
    if !tools.is_empty() {
        request["tools"] = json!(tools);
        // This model refuses function tools on the chat completions API
        // unless its reasoning is switched off.
        request["reasoning_effort"] = json!("none");
    }
    let response = http.post(&url).json(&request).send().await.map_err(|e| format!("cannot reach the gateway at {url}: {e}"))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| format!("cannot read the gateway's answer: {e}"))?;
    if !status.is_success() {
        return Err(format!("the gateway answered {status}: {body}"));
    }
    let mut body: Value = serde_json::from_str(&body).map_err(|e| format!("the gateway's answer is not JSON: {e}"))?;
    match body["choices"][0]["message"].take() {
        Value::Null => Err("the gateway's answer has no message".into()),
        message => Ok(message),
    }
}

/// One question in, one answer out. The model may call MCP tools in between.
async fn answer(http: &reqwest::Client, question: &str) -> Result<Answer, String> {
    let mut note = None;
    // A fresh MCP connection per question: nothing to keep alive, and a
    // newly promoted MCP server is picked up at once.
    let mcp = match mcp_url() {
        None => None,
        Some(url) => {
            let config = ClientConfig::new(ClientCapabilities::default(), Implementation::new(NAME, version()))
                // The newest protocol version that still starts with `initialize`,
                // which is what agentgateway accepts.
                .with_protocol_version(ProtocolVersion::LATEST_WITH_INITIALIZE);
            let transport = StreamableHttpClientTransport::from_config(StreamableHttpClientTransportConfig::with_uri(url.clone()));
            match tokio::time::timeout(Duration::from_secs(5), config.serve(transport)).await {
                Ok(Ok(client)) => Some(client),
                Ok(Err(error)) => {
                    note = Some(format!("Answered without tools: cannot connect to {url}: {error}"));
                    None
                }
                Err(_) => {
                    note = Some(format!("Answered without tools: no answer from {url} within 5 seconds"));
                    None
                }
            }
        }
    };

    // The MCP server's tools, in the form the chat API wants them.
    let mut tools = Vec::new();
    if let Some(client) = &mcp {
        match client.list_all_tools().await {
            Ok(found) => {
                for tool in found {
                    tools.push(json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": Value::Object((*tool.input_schema).clone()),
                        },
                    }));
                }
            }
            Err(error) => note = Some(format!("Answered without tools: cannot list them: {error}")),
        }
    }

    let mut messages = vec![
        json!({ "role": "system", "content": SYSTEM_PROMPT }),
        json!({ "role": "user", "content": question }),
    ];
    let mut used = Vec::new();
    for _ in 0..MAX_TOOL_ROUNDS {
        let message = complete(http, &messages, &tools).await?;
        let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            let reply = message["content"].as_str().unwrap_or("(the model gave no text)").to_owned();
            return Ok(Answer { reply, tools: used, note });
        }
        messages.push(message);
        for call in calls {
            let name = call["function"]["name"].as_str().unwrap_or_default().to_owned();
            let arguments: Value = serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
            let result = match &mcp {
                Some(client) => {
                    let params = CallToolRequestParams::new(name.clone()).with_arguments(arguments.as_object().cloned().unwrap_or_default());
                    match client.call_tool(params).await {
                        // The result is a list of content blocks; the text ones are what the model reads.
                        Ok(result) => serde_json::to_value(&result.content)
                            .ok()
                            .and_then(|blocks| blocks.as_array().cloned())
                            .unwrap_or_default()
                            .iter()
                            .filter_map(|block| block["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n"),
                        Err(error) => format!("the tool call failed: {error}"),
                    }
                }
                None => "no tool server is connected".to_owned(),
            };
            messages.push(json!({ "role": "tool", "tool_call_id": call["id"], "content": result }));
            used.push(json!({ "name": name, "arguments": arguments, "result": result }));
        }
    }
    Err(format!("the model kept asking for tools after {MAX_TOOL_ROUNDS} rounds"))
}

async fn chat(State(http): State<reqwest::Client>, Json(chat): Json<Chat>) -> (StatusCode, Json<Value>) {
    match answer(&http, &chat.message).await {
        Ok(answer) => (StatusCode::OK, Json(json!({ "reply": answer.reply, "tools": answer.tools, "note": answer.note }))),
        Err(error) => {
            eprintln!("chat failed: {error}");
            (StatusCode::BAD_GATEWAY, Json(json!({ "reply": format!("Error: {error}") })))
        }
    }
}

async fn info() -> Json<Value> {
    Json(json!({ "agent": NAME, "version": version(), "stage": stage(), "model": model(), "mcp_url": mcp_url() }))
}

#[tokio::main]
async fn main() {
    let addr = var("AGENT_ADDR", "127.0.0.1:3000");
    let http = reqwest::Client::builder().timeout(Duration::from_secs(60)).build().expect("http client");
    let app = Router::new()
        .route("/", get(index))
        .route("/chat", post(chat))
        .route("/info", get(info))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(http);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("{NAME} {} ({}) listening on {addr}", version(), stage());
    let mut term = signal(SignalKind::terminate()).expect("signal handler");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            term.recv().await;
        })
        .await
        .expect("server");
}

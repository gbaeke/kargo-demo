//! The research agent: answers one question at a time with a model, in the
//! role of a research assistant. No chat history: every message is a
//! fresh request.
//!
//! It holds no provider key. It sends an OpenAI-style chat request to the AI
//! gateway (GATEWAY_URL), which adds the key and forwards it to the model
//! (MODEL).
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
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::signal::unix::{SignalKind, signal};

const NAME: &str = "research";
const SYSTEM_PROMPT: &str = "You are a research assistant. Answer the question with what is \
well established, as three to five short bullet points, then one line starting with 'Uncertain:' \
naming what is disputed or what you do not know. Do not invent sources. Keep it under 150 words.";

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
<h1>Research agent</h1>
<p>version <b>{version}</b> in stage <b style="color: {color}">{stage}</b>, model <b>{model}</b></p>
<form onsubmit="ask(event)"><input id="q" placeholder="What should I look into?" style="width: 70%"> <button id="b">Send</button></form>
<pre id="a" style="white-space: pre-wrap"></pre>
<script>
async function ask(e) {{
  e.preventDefault();
  const a = document.getElementById('a'), b = document.getElementById('b');
  a.textContent = 'Thinking...'; b.disabled = true;
  try {{
    const r = await fetch('/chat', {{method: 'POST', headers: {{'content-type': 'application/json'}},
      body: JSON.stringify({{message: document.getElementById('q').value}})}});
    a.textContent = (await r.json()).reply;
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

/// One question in, one answer out: a chat completion through the gateway.
async fn complete(http: &reqwest::Client, question: &str) -> Result<String, String> {
    let url = format!("{}/v1/chat/completions", var("GATEWAY_URL", "http://127.0.0.1:4000"));
    let request = json!({
        "model": model(),
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": question },
        ],
    });
    let response = http.post(&url).json(&request).send().await.map_err(|e| format!("cannot reach the gateway at {url}: {e}"))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| format!("cannot read the gateway's answer: {e}"))?;
    if !status.is_success() {
        return Err(format!("the gateway answered {status}: {body}"));
    }
    let body: Value = serde_json::from_str(&body).map_err(|e| format!("the gateway's answer is not JSON: {e}"))?;
    body["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "the gateway's answer has no message".into())
}

async fn chat(State(http): State<reqwest::Client>, Json(chat): Json<Chat>) -> (StatusCode, Json<Value>) {
    match complete(&http, &chat.message).await {
        Ok(reply) => (StatusCode::OK, Json(json!({ "reply": reply }))),
        Err(error) => {
            eprintln!("chat failed: {error}");
            (StatusCode::BAD_GATEWAY, Json(json!({ "reply": format!("Error: {error}") })))
        }
    }
}

async fn info() -> Json<Value> {
    Json(json!({ "agent": NAME, "version": version(), "stage": stage(), "model": model() }))
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

//! A stub "agent" to promote with Kargo: no model behind it, it only answers
//! with its version and the stage it runs in, so a promotion is visible.
//!
//! APP_VERSION is baked into the image (the image tag); STAGE comes from the
//! stage's kustomize overlay.

use std::env;

use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::signal::unix::{SignalKind, signal};

fn version() -> String {
    env::var("APP_VERSION").unwrap_or_else(|_| "dev-build".into())
}

fn stage() -> String {
    env::var("STAGE").unwrap_or_else(|_| "local".into())
}

async fn index() -> Html<String> {
    let (version, stage) = (version(), stage());
    let color = if stage == "prod" { "#b3261e" } else { "#1b6e3c" };
    Html(format!(
        r#"<!doctype html>
<meta charset="utf-8"><title>agent {version} ({stage})</title>
<body style="font-family: system-ui; max-width: 40rem; margin: 4rem auto; padding: 0 1rem">
<h1>Stub agent</h1>
<p>version <b>{version}</b> in stage <b style="color: {color}">{stage}</b></p>
<form onsubmit="ask(event)"><input id="q" placeholder="Ask something" style="width: 70%"> <button>Send</button></form>
<pre id="a"></pre>
<script>
async function ask(e) {{
  e.preventDefault();
  const r = await fetch('/chat', {{method: 'POST', headers: {{'content-type': 'application/json'}},
    body: JSON.stringify({{message: document.getElementById('q').value}})}});
  document.getElementById('a').textContent = (await r.json()).reply;
}}
</script>"#
    ))
}

#[derive(Deserialize)]
struct Chat {
    message: String,
}

async fn chat(Json(chat): Json<Chat>) -> Json<Value> {
    Json(json!({
        "reply": format!("[agent {} in {}] you said: {}", version(), stage(), chat.message),
    }))
}

async fn info() -> Json<Value> {
    Json(json!({ "version": version(), "stage": stage() }))
}

#[tokio::main]
async fn main() {
    let addr = env::var("AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let app = Router::new()
        .route("/", get(index))
        .route("/chat", post(chat))
        .route("/info", get(info))
        .route("/healthz", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("agent {} ({}) listening on {addr}", version(), stage());
    let mut term = signal(SignalKind::terminate()).expect("signal handler");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            term.recv().await;
        })
        .await
        .expect("server");
}

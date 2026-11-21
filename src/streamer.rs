use std::sync::mpsc::Sender;
use std::time::Duration;

use base64::Engine as _;
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::protocol::{NdjsonParser, VisualOp};

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Op(String, VisualOp),
    Status(String),
    Done,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Demo,
    OpenAI,
    Anthropic,
    Gemini,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Provider::Demo => "Demo (Offline)",
            Provider::OpenAI => "OpenAI (gpt-4o-mini)",
            Provider::Anthropic => "Anthropic (Claude Sonnet)",
            Provider::Gemini => "Gemini (2.5 Flash)",
        }
    }

    pub fn detect_default() -> Self {
        if std::env::var("OPENAI_API_KEY").is_ok() {
            Provider::OpenAI
        } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            Provider::Anthropic
        } else if std::env::var("GEMINI_API_KEY").is_ok() {
            Provider::Gemini
        } else {
            Provider::Demo
        }
    }
}

const SYSTEM_PROMPT: &str = r##"You are copain_t, a real-time 2D ink sketching and interactive 3D visual rendering engine.
Output ONLY newline-delimited JSON (NDJSON). One JSON object per line. No markdown fences, no commentary.
Emit coarse layout or pointer commands FIRST so the user sees immediate visual feedback, then details and 3D objects.

Coordinate system & layout rules:
- 2D: normalized [0.0, 1.0] for x, y, w, h, r. Use clean left-to-right columns for flowcharts; never route arrows through other boxes.
- When combining 2D and 3D, keep 2D annotations on the left (x: 0.04..0.58) and mount 3D on the right (x: 0.60..0.96).
- 3D: right-handed world coordinates centered at [0, 0, 0].
- Use plain ASCII arrows/dashes (->, <-, -) in text labels.

Allowed ops:
{"op":"clear","layer":"2d"|"3d"|"all"}
{"op":"backdrop","x":0.02,"y":0.04,"w":0.96,"h":0.84,"color":"#FAF9F6E8","radius":18}  (optional frosted panel behind dense diagrams for readability over desktop apps)
{"op":"pointer","x":0.42,"y":0.61,"label":"Click Export","color":"#FF5500"}
{"op":"rect","x":0.38,"y":0.58,"w":0.12,"h":0.06,"color":"#FF5500","style":"sketch"|"solid","label":"Optional"}
{"op":"circle","x":0.5,"y":0.5,"r":0.08,"color":"#3B82F6","style":"sketch"|"solid","label":"Optional"}
{"op":"arrow","from":[0.25,0.40],"to":[0.38,0.58],"color":"#FF5500","label":"Optional"}
{"op":"stroke","points":[[0.1,0.2],[0.15,0.22],[0.2,0.18]],"color":"#1A1A2E","width":3.0}
{"op":"label","x":0.15,"y":0.12,"text":"Heading","color":"#1A1A2E","size":18}
{"op":"3d_mount","bounds":{"x":0.55,"y":0.14,"w":0.38,"h":0.48},"title":"Scene Title"}
{"op":"3d_add","id":"cube1","type":"box"|"sphere"|"cylinder"|"torus"|"cone"|"plane"|"line"|"billboard","pos":[0,0,0],"rot":[0,0,0],"size":[1,1,1],"color":"#3B82F6","spin":[0,0.015,0],"bob":[0.15,2.0],"text":"Label","points":[[0,0,0],[1,1,1]],"wireframe":false}
{"op":"3d_update","id":"cube1","pos":[0,0.5,0],"color":"#10B981"}
{"op":"3d_remove","id":"cube1"}"##;

/// Capture the primary display on macOS as a base64-encoded JPEG (returns None on unsupported OS or failure).
pub fn capture_screen_jpeg_base64() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let tmp = std::env::temp_dir().join("copain_t_capture.jpg");
        let status = std::process::Command::new("screencapture")
            .args(["-x", "-t", "jpg", tmp.to_str()?])
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
        // Resize to max 1280px wide using macOS built-in `sips` to minimize upload latency
        let _ = std::process::Command::new("sips")
            .args(["-Z", "1280", tmp.to_str()?])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let bytes = std::fs::read(&tmp).ok()?;
        let _ = std::fs::remove_file(&tmp);
        Some(base64::engine::general_purpose::STANDARD.encode(bytes))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn spawn_stream(
    rt: &tokio::runtime::Runtime,
    provider: Provider,
    prompt: String,
    screenshot_b64: Option<String>,
    tx: Sender<StreamEvent>,
    repaint: impl Fn() + Send + Sync + 'static,
) {
    rt.spawn(async move {
        let res = match provider {
            Provider::Demo => run_demo_stream(&prompt, &tx, &repaint).await,
            Provider::OpenAI => {
                run_openai_stream(&prompt, screenshot_b64, &tx, &repaint).await
            }
            Provider::Anthropic => {
                run_anthropic_stream(&prompt, screenshot_b64, &tx, &repaint).await
            }
            Provider::Gemini => {
                run_gemini_stream(&prompt, screenshot_b64, &tx, &repaint).await
            }
        };

        if let Err(err) = res {
            let _ = tx.send(StreamEvent::Error(err));
        }
        let _ = tx.send(StreamEvent::Done);
        repaint();
    });
}

async fn run_demo_stream(
    prompt: &str,
    tx: &Sender<StreamEvent>,
    repaint: &(impl Fn() + Send + Sync),
) -> Result<(), String> {
    let lower = prompt.to_lowercase();
    let lines: Vec<&str> = if lower.contains("frustum") || lower.contains("camera") || lower.contains("3d") {
        demo_camera_frustum()
    } else if lower.contains("solar") || lower.contains("orbit") || lower.contains("planet") {
        demo_solar_orbit()
    } else if lower.contains("arch") || lower.contains("pipeline") || lower.contains("diagram") {
        demo_architecture()
    } else {
        demo_ui_guidance()
    };

    let _ = tx.send(StreamEvent::Status(format!(
        "Streaming {} NDJSON ops (Demo)…",
        lines.len()
    )));
    repaint();

    let mut parser = NdjsonParser::new();
    for line in lines {
        let mut chunk = String::from(line);
        chunk.push('\n');
        for (raw, op) in parser.push_chunk(&chunk) {
            let _ = tx.send(StreamEvent::Op(raw, op));
            repaint();
        }
        tokio::time::sleep(Duration::from_millis(210)).await;
    }
    Ok(())
}

async fn run_openai_stream(
    prompt: &str,
    screenshot_b64: Option<String>,
    tx: &Sender<StreamEvent>,
    repaint: &(impl Fn() + Send + Sync),
) -> Result<(), String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY not set in environment".to_string())?;
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

    let user_content = if let Some(b64) = screenshot_b64 {
        json!([
            { "type": "text", "text": prompt },
            { "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{}", b64) } }
        ])
    } else {
        json!(prompt)
    };

    let body = json!({
        "model": model,
        "stream": true,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_content }
        ]
    });

    let _ = tx.send(StreamEvent::Status(format!("Streaming from OpenAI ({})…", model)));
    repaint();

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI HTTP {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    let mut sse_buf = String::new();
    let mut parser = NdjsonParser::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Stream read error: {e}"))?;
        sse_buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(idx) = sse_buf.find('\n') {
            let line = sse_buf[..idx].trim().to_string();
            sse_buf.drain(..=idx);
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                if let Ok(val) = serde_json::from_str::<Value>(data) {
                    if let Some(delta) = val["choices"][0]["delta"]["content"].as_str() {
                        for (raw, op) in parser.push_chunk(delta) {
                            let _ = tx.send(StreamEvent::Op(raw, op));
                            repaint();
                        }
                    }
                }
            }
        }
    }
    for (raw, op) in parser.finish() {
        let _ = tx.send(StreamEvent::Op(raw, op));
        repaint();
    }
    Ok(())
}

async fn run_anthropic_stream(
    prompt: &str,
    screenshot_b64: Option<String>,
    tx: &Sender<StreamEvent>,
    repaint: &(impl Fn() + Send + Sync),
) -> Result<(), String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY not set in environment".to_string())?;
    let model = std::env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-3-5-sonnet-latest".to_string());

    let user_content = if let Some(b64) = screenshot_b64 {
        json!([
            { "type": "image", "source": { "type": "base64", "media_type": "image/jpeg", "data": b64 } },
            { "type": "text", "text": prompt }
        ])
    } else {
        json!(prompt)
    };

    let body = json!({
        "model": model,
        "max_tokens": 2048,
        "stream": true,
        "system": SYSTEM_PROMPT,
        "messages": [
            { "role": "user", "content": user_content }
        ]
    });

    let _ = tx.send(StreamEvent::Status(format!("Streaming from Anthropic ({})…", model)));
    repaint();

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Anthropic request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic HTTP {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    let mut sse_buf = String::new();
    let mut parser = NdjsonParser::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Stream read error: {e}"))?;
        sse_buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(idx) = sse_buf.find('\n') {
            let line = sse_buf[..idx].trim().to_string();
            sse_buf.drain(..=idx);
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(val) = serde_json::from_str::<Value>(data) {
                    if val["type"] == "content_block_delta" {
                        if let Some(delta) = val["delta"]["text"].as_str() {
                            for (raw, op) in parser.push_chunk(delta) {
                                let _ = tx.send(StreamEvent::Op(raw, op));
                                repaint();
                            }
                        }
                    }
                }
            }
        }
    }
    for (raw, op) in parser.finish() {
        let _ = tx.send(StreamEvent::Op(raw, op));
        repaint();
    }
    Ok(())
}

async fn run_gemini_stream(
    prompt: &str,
    screenshot_b64: Option<String>,
    tx: &Sender<StreamEvent>,
    repaint: &(impl Fn() + Send + Sync),
) -> Result<(), String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set in environment".to_string())?;
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_string());

    let mut parts = vec![json!({ "text": prompt })];
    if let Some(b64) = screenshot_b64 {
        parts.push(json!({
            "inline_data": { "mime_type": "image/jpeg", "data": b64 }
        }));
    }

    let body = json!({
        "system_instruction": { "parts": [{ "text": SYSTEM_PROMPT }] },
        "contents": [{ "role": "user", "parts": parts }]
    });

    let _ = tx.send(StreamEvent::Status(format!("Streaming from Gemini ({})…", model)));
    repaint();

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse&key={api_key}"
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gemini HTTP {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    let mut sse_buf = String::new();
    let mut parser = NdjsonParser::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Stream read error: {e}"))?;
        sse_buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(idx) = sse_buf.find('\n') {
            let line = sse_buf[..idx].trim().to_string();
            sse_buf.drain(..=idx);
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(val) = serde_json::from_str::<Value>(data) {
                    if let Some(delta) =
                        val["candidates"][0]["content"]["parts"][0]["text"].as_str()
                    {
                        for (raw, op) in parser.push_chunk(delta) {
                            let _ = tx.send(StreamEvent::Op(raw, op));
                            repaint();
                        }
                    }
                }
            }
        }
    }
    for (raw, op) in parser.finish() {
        let _ = tx.send(StreamEvent::Op(raw, op));
        repaint();
    }
    Ok(())
}

fn demo_ui_guidance() -> Vec<&'static str> {
    vec![
        r##"{"op":"clear","layer":"all"}"##,
        r##"{"op":"pointer","x":0.42,"y":0.56,"label":"Click Export","color":"#FF5500"}"##,
        r##"{"op":"rect","x":0.35,"y":0.51,"w":0.16,"h":0.09,"color":"#FF5500","style":"sketch","label":"Export Button"}"##,
        r##"{"op":"arrow","from":[0.20,0.36],"to":[0.35,0.52],"color":"#FF5500","label":"Step 1"}"##,
        r##"{"op":"label","x":0.10,"y":0.28,"text":"1. Start by selecting Export here","color":"#1A1A2E","size":17}"##,
        r##"{"op":"stroke","points":[[0.10,0.32],[0.16,0.325],[0.24,0.318],[0.31,0.322]],"color":"#FF5500","width":3.2}"##,
        r##"{"op":"3d_mount","bounds":{"x":0.58,"y":0.16,"w":0.36,"h":0.48},"title":"Exported 3D Mesh Preview"}"##,
        r##"{"op":"3d_add","id":"cube1","type":"box","pos":[0,0,0],"size":[1.2,1.2,1.2],"color":"#3B82F6","spin":[0.004,0.014,0]}"##,
        r##"{"op":"3d_add","id":"ring1","type":"torus","pos":[0,0,0],"rot":[1.2,0.3,0],"size":[1.8,1.8,1.8],"color":"#FF5500","spin":[0.01,0.005,0]}"##,
        r##"{"op":"3d_add","id":"label1","type":"billboard","pos":[0,1.25,0],"color":"#3B82F6","text":"GLTF / Mesh Ready"}"##,
    ]
}

fn demo_camera_frustum() -> Vec<&'static str> {
    vec![
        r##"{"op":"clear","layer":"all"}"##,
        r##"{"op":"label","x":0.06,"y":0.14,"text":"3D Perspective Camera Frustum","color":"#1A1A2E","size":20}"##,
        r##"{"op":"stroke","points":[[0.06,0.185],[0.16,0.182],[0.28,0.186],[0.36,0.183]],"color":"#3B82F6","width":3.0}"##,
        r##"{"op":"rect","x":0.06,"y":0.24,"w":0.36,"h":0.22,"color":"#3B82F6","style":"sketch","label":"Key Concepts"}"##,
        r##"{"op":"label","x":0.08,"y":0.28,"text":"• Near Plane: clips close geometry","color":"#1A1A2E","size":14}"##,
        r##"{"op":"label","x":0.08,"y":0.33,"text":"• Far Plane: max render depth","color":"#1A1A2E","size":14}"##,
        r##"{"op":"label","x":0.08,"y":0.38,"text":"• FOV: vertical field of view angle","color":"#1A1A2E","size":14}"##,
        r##"{"op":"arrow","from":[0.43,0.35],"to":[0.51,0.35],"color":"#FF5500","label":"Interactive 3D"}"##,
        r##"{"op":"3d_mount","bounds":{"x":0.52,"y":0.13,"w":0.43,"h":0.56},"title":"Camera Frustum"}"##,
        r##"{"op":"3d_add","id":"cam_body","type":"box","pos":[0,0,1.6],"size":[0.5,0.38,0.6],"color":"#334155"}"##,
        r##"{"op":"3d_add","id":"cam_lens","type":"cone","pos":[0,0,1.15],"rot":[-1.57,0,0],"size":[0.35,0.4,0.35],"color":"#FF5500"}"##,
        r##"{"op":"3d_add","id":"ray1","type":"line","points":[[0,0,1.2],[-1.3,0.9,-1.4]],"color":"#38BDF8"}"##,
        r##"{"op":"3d_add","id":"ray2","type":"line","points":[[0,0,1.2],[1.3,0.9,-1.4]],"color":"#38BDF8"}"##,
        r##"{"op":"3d_add","id":"ray3","type":"line","points":[[0,0,1.2],[1.3,-0.9,-1.4]],"color":"#38BDF8"}"##,
        r##"{"op":"3d_add","id":"ray4","type":"line","points":[[0,0,1.2],[-1.3,-0.9,-1.4]],"color":"#38BDF8"}"##,
        r##"{"op":"3d_add","id":"far_frame","type":"line","points":[[-1.3,0.9,-1.4],[1.3,0.9,-1.4],[1.3,-0.9,-1.4],[-1.3,-0.9,-1.4],[-1.3,0.9,-1.4]],"color":"#3B82F6"}"##,
        r##"{"op":"3d_add","id":"near_frame","type":"line","points":[[-0.45,0.31,0.3],[0.45,0.31,0.3],[0.45,-0.31,0.3],[-0.45,-0.31,0.3],[-0.45,0.31,0.3]],"color":"#10B981"}"##,
        r##"{"op":"3d_add","id":"target","type":"box","pos":[0,0,-0.5],"size":[0.55,0.55,0.55],"color":"#F59E0B","spin":[0.008,0.015,0]}"##,
        r##"{"op":"3d_add","id":"lbl_near","type":"billboard","pos":[0,0.55,0.3],"color":"#10B981","text":"Near Plane"}"##,
        r##"{"op":"3d_add","id":"lbl_far","type":"billboard","pos":[0,1.15,-1.4],"color":"#3B82F6","text":"Far Plane"}"##,
    ]
}

fn demo_solar_orbit() -> Vec<&'static str> {
    vec![
        r##"{"op":"clear","layer":"all"}"##,
        r##"{"op":"label","x":0.06,"y":0.14,"text":"Orbital Mechanics Sketch","color":"#1A1A2E","size":20}"##,
        r##"{"op":"circle","x":0.22,"y":0.42,"r":0.09,"color":"#F59E0B","style":"sketch","label":"Gravity Well"}"##,
        r##"{"op":"arrow","from":[0.32,0.42],"to":[0.49,0.42],"color":"#3B82F6","label":"3D Model"}"##,
        r##"{"op":"3d_mount","bounds":{"x":0.50,"y":0.13,"w":0.44,"h":0.58},"title":"Orbital System"}"##,
        r##"{"op":"3d_add","id":"sun","type":"sphere","pos":[0,0,0],"size":[1.0,1.0,1.0],"color":"#F59E0B","spin":[0,0.008,0]}"##,
        r##"{"op":"3d_add","id":"orbit1","type":"torus","pos":[0,0,0],"size":[2.6,0.2,2.6],"color":"#64748B","wireframe":true}"##,
        r##"{"op":"3d_add","id":"planet1","type":"sphere","pos":[1.3,0,0],"size":[0.42,0.42,0.42],"color":"#3B82F6","spin":[0,0.03,0],"bob":[0.15,2.2]}"##,
        r##"{"op":"3d_add","id":"planet2","type":"sphere","pos":[-1.5,0.2,0.9],"size":[0.32,0.32,0.32],"color":"#10B981","spin":[0,0.02,0],"bob":[0.12,1.6]}"##,
        r##"{"op":"3d_add","id":"lbl_sun","type":"billboard","pos":[0,0.85,0],"color":"#F59E0B","text":"Star (M)"}"##,
        r##"{"op":"3d_add","id":"lbl_p1","type":"billboard","pos":[1.3,0.55,0],"color":"#3B82F6","text":"Orbiting Body"}"##,
    ]
}

fn demo_architecture() -> Vec<&'static str> {
    vec![
        r##"{"op":"clear","layer":"all"}"##,
        r##"{"op":"label","x":0.08,"y":0.12,"text":"copain_t Streaming Pipeline","color":"#1A1A2E","size":20}"##,
        r##"{"op":"rect","x":0.08,"y":0.24,"w":0.18,"h":0.12,"color":"#3B82F6","style":"sketch","label":"1. Context"}"##,
        r##"{"op":"arrow","from":[0.26,0.30],"to":[0.34,0.30],"color":"#1A1A2E"}"##,
        r##"{"op":"rect","x":0.34,"y":0.24,"w":0.20,"h":0.12,"color":"#8B5CF6","style":"sketch","label":"2. Multimodal LLM"}"##,
        r##"{"op":"arrow","from":[0.54,0.30],"to":[0.62,0.30],"color":"#1A1A2E","label":"NDJSON"}"##,
        r##"{"op":"rect","x":0.62,"y":0.24,"w":0.24,"h":0.12,"color":"#FF5500","style":"sketch","label":"3. Incremental Parser"}"##,
        r##"{"op":"arrow","from":[0.74,0.36],"to":[0.45,0.52],"color":"#10B981","label":"2D Ops"}"##,
        r##"{"op":"arrow","from":[0.74,0.36],"to":[0.74,0.52],"color":"#3B82F6","label":"3D Ops"}"##,
        r##"{"op":"rect","x":0.32,"y":0.52,"w":0.24,"h":0.14,"color":"#10B981","style":"sketch","label":"4a. 2D Ink Engine"}"##,
        r##"{"op":"rect","x":0.62,"y":0.52,"w":0.24,"h":0.14,"color":"#3B82F6","style":"sketch","label":"4b. 3D Widget Engine"}"##,
        r##"{"op":"pointer","x":0.74,"y":0.30,"label":"<500ms first pixel","color":"#FF5500"}"##,
    ]
}

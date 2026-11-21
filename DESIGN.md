# `copain_t` — PoC Design Doc

> Real-time AI 2D ink sketching & interactive 3D visual rendering — inline on a canvas or floating over any desktop app.

---

## 1. Name & Identity
- **Name:** `copain_t`
- **Wordplay:**
  - **`copain`** — French for *buddy / companion*
  - **`copain_t`** — reads as *co-paint* (live sketching & visualizing together) while nodding to systems-programming type suffixes (`size_t`, `copain_t`)
  - **`copAIn_t`** — embeds **AI** directly in the name
- **Visual Direction:** `copain` in clean solid ink, a **tiny `_`** to keep a subtle, tight gap, and `t` styled as a hand-drawn stroke or accent highlight.

---

## 2. Objective
Build a lightweight Proof of Concept (PoC) for an AI system that renders **real-time 2D annotations/ink sketches** (Riddled-style) and **interactive 3D scenes** (HeyClicky-style) directly in context—without waiting for full responses or generating slow raster images.

### Core Goals
- **Sub-second time-to-first-pixel:** Start drawing within ~300–500ms of the model's first tokens.
- **Unified streaming protocol:** One lightweight DSL for both 2D vector/ink and 3D scene primitives.
- **Dual deployment modes:**
  1. **Canvas Mode (Web):** Inline rendering inside a notebook or infinite canvas.
  2. **Overlay Mode (Desktop):** Transparent, click-through window floating over any OS app.

---

## 3. High-Level Architecture

```text
[User Trigger: Hotkey / Inline Prompt]
                 │
                 ▼
┌─────────────────────────────────┐
│ 1. Context Capture              │  (DOM/Canvas state OR OS Screenshot + Window Bounds)
└────────────────┬────────────────┘
                 │ (Image + Prompt)
                 ▼
┌─────────────────────────────────┐
│ 2. Multimodal LLM (Streaming)   │  (Streams NDJSON visual commands token-by-token)
└────────────────┬────────────────┘
                 │ (Token Stream)
                 ▼
┌─────────────────────────────────┐
│ 3. Incremental Stream Parser    │  (Parses completed lines, pushes to Scene Store)
└────────────────┬────────────────┘
                 │
        ┌────────┴────────┐
        ▼                 ▼
┌───────────────┐ ┌───────────────┐
│ 4a. 2D Engine │ │ 4b. 3D Engine │  (60fps render loop + stroke/motion interpolation)
│ egui / Sketch │ │ glam / 3D Proj│
└───────┬───────┘ └───────┬───────┘
        └────────┬────────┘
                 ▼
┌─────────────────────────────────┐
│ 5. Host Surface                 │  (Native Canvas Window OR Click-Through Desktop Overlay)
└─────────────────────────────────┘
```

---

## 4. Streaming Visual Protocol (NDJSON)

Instead of a single giant JSON blob (which is fragile to parse mid-stream), instruct the model to emit **newline-delimited JSON (NDJSON)**. Every newline yields an immediately renderable primitive.

### Coordinate System
- **2D:** Normalized `[0.0, 1.0]` coordinates relative to the captured viewport/screen. The client multiplies by `(window.innerWidth, window.innerHeight)` to handle Retina/DPI scaling cleanly.
- **3D:** Standard right-handed world coordinates (`x, y, z`) centered at `(0, 0, 0)`.

### Example Stream Payload

```json
{"op":"clear","layer":"2d"}
{"op":"pointer","x":0.42,"y":0.61,"label":"Click Export"}
{"op":"rect","x":0.38,"y":0.58,"w":0.12,"h":0.06,"color":"#FF5500","style":"sketch"}
{"op":"arrow","from":[0.25,0.40],"to":[0.38,0.58],"color":"#FF5500"}
{"op":"stroke","points":[[0.1,0.2],[0.15,0.22],[0.2,0.18]],"color":"#111","width":3}
{"op":"3d_mount","bounds":{"x":0.55,"y":0.15,"w":0.35,"h":0.35}}
{"op":"3d_add","id":"cube1","type":"box","pos":[0,0,0],"size":[1,1,1],"color":"#3b82f6","spin":[0,0.01,0]}
{"op":"3d_add","id":"label1","type":"billboard","pos":[0,1.2,0],"text":"X-Axis Rotation"}
```

---

## 5. Component Design

### A. Context Capture
- **Canvas Mode:** Serialize surrounding prompt/canvas state.
- **Overlay Mode:** Capture the active display via native OS APIs (e.g., `screencapture` on macOS). Resize/encode as JPEG to minimize upload latency and vision token cost.

### B. LLM Streaming Layer
- **Model:** Fast multimodal model (e.g., GPT-4o, Gemini 2.5 Flash, or Claude Sonnet) streamed directly in Rust via `reqwest` + `tokio` SSE streams, plus a built-in local Demo Streamer for offline testing.
- **Prompting Strategy:**
  - Provide a strict schema of allowed `op` types.
  - Instruct the model to emit *coarse layout / pointer commands first*, followed by detail strokes or 3D meshes, so the user sees immediate visual feedback.
  - Optionally separate **voice/text explanation** and **visual commands** (via tool calls or interleaved tagged blocks).

### C. Incremental Parser & Animation Queue
- Reads incoming chunks from an async HTTP byte stream, splitting on `\n`.
- Valid JSON lines are parsed with `serde_json` and pushed into the `SceneStore` over a lock-free `mpsc` channel.
- **Progressive Ink Animation (Riddled-style):**
  - Do not snap-render strokes instantly. Store a `progress: 0.0 -> 1.0` value per primitive.
  - Advance `progress` over ~250–500ms in the 60fps `eframe` update loop so lines, arrows, and sketches appear to be drawn live.

### D. Dual Rendering Engine
1. **2D Layer (`egui::Painter` + Custom Sketch/Ink Engine):**
   - **Tapered Ink Strokes**: Catmull-Rom smoothing + variable-width pressure tapering for freehand strokes.
   - **Sketchy Primitives**: Deterministic multi-pass jittered strokes for rectangles, circles, and arrows with a hand-drawn aesthetic.
   - **Animated Cursor/Pointer**: Spring-interpolated `(x, y)` indicator with pulsing ring for HeyClicky-style guidance.
2. **3D Layer (`glam` + Software 3D Projection in `egui`):**
   - Mounted inside a bounded floating widget (`3d_mount`) rendered directly on the transparent surface.
   - Supports basic geometric primitives (`box`, `sphere`, `cylinder`, `torus`, `cone`, `plane`, `line`, `billboard`), directional + ambient lighting, z-sorted faces, drag-to-orbit & scroll-to-zoom camera controls, and per-frame animations (`spin`, `bob`).

### E. Host Surface (OS-Level Overlay)
For the HeyClicky-style desktop experience using **Rust + `eframe`/`egui`**:
- Configure a frameless, transparent, always-on-top native window:
  ```rust
  let viewport = egui::ViewportBuilder::default()
      .with_transparent(true)
      .with_decorations(false)
      .with_always_on_top();
  ```
- **Selective Interactivity (Crucial for 3D):**
  - Toggle `egui::ViewportCommand::MousePassthrough(true)` in Overlay Mode so clicks pass through to underlying apps.
  - When the user hovers over an interactive 3D widget, prompt bar, or dismiss button, toggle `MousePassthrough(false)`—allowing the user to orbit/zoom the 3D scene—then restore click-through on hover exit.

---

## 6. Recommended PoC Tech Stack

| Layer | Choice | Why |
| :--- | :--- | :--- |
| **App Shell** | Rust + `eframe` / `egui` | Single native binary supports both Canvas Mode & transparent Click-Through Overlay Mode |
| **State** | `SceneStore` + `std::sync::mpsc` | Zero-lock 60fps updates fed directly from async token streams |
| **2D Rendering** | `egui::Painter` + custom Sketch/Ink engine | Fast 60fps native vector drawing with hand-drawn jitter & tapered ink |
| **3D Rendering** | `glam` + z-sorted 3D widget renderer | Lightweight, zero-driver-friction interactive 3D primitives with orbit controls |
| **AI Streaming** | `tokio` + `reqwest` + `serde_json` | Direct SSE streaming across OpenAI / Anthropic / Google + built-in demo streamer |

---

## 7. Phased Implementation Plan

### Phase 1: Streaming 2D Ink & Diagram Engine (Web) — *1–2 Days*
- Build a split-screen or inline web canvas.
- Connect a prompt input to an LLM streaming NDJSON (`rect`, `arrow`, `stroke`, `label`).
- Implement `requestAnimationFrame` progressive drawing with `roughjs` and `perfect-freehand`.

### Phase 2: Streaming Interactive 3D Widgets (Web) — *1–2 Days*
- Add a transparent Three.js / R3F overlay layer.
- Support `3d_mount`, `3d_add`, `3d_update`, and `OrbitControls`.
- Test prompting the model to explain spatial concepts (e.g., *"Show me a 3D diagram of a camera frustum"*).

### Phase 3: Desktop Click-Through Overlay + Screen Grounding — *2–3 Days*
- Wrap the web app in a transparent, always-on-top Electron window.
- Add a global hotkey (`Cmd+Shift+Space`) that captures the screen, sends it to the multimodal model, and renders 2D pointers/highlights over the actual OS UI.
- Wire up hover-based click-through toggling so 3D widgets remain rotatable.

---

## 8. Key Risks & Mitigations

1. **Spatial Grounding Drift (Clicking/pointing slightly off-target):**
   - *Mitigation:* Overlay a subtle coordinate grid on the screenshot *before* sending it to the VLM (Set-of-Mark / grid prompting), or use models with strong native grounding (e.g., Gemini 2.5 / Claude Computer Use / GPT-4o).
2. **Malformed JSON Mid-Stream:**
   - *Mitigation:* NDJSON isolates errors to a single line—wrap each `JSON.parse()` in a `try/catch` and silently drop malformed lines without breaking the scene.
3. **Overlay Blocking User Input:**
   - *Mitigation:* Default to full click-through; include a global `Esc` hotkey to immediately clear all overlays.

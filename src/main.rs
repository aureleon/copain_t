mod protocol;
mod render_2d;
mod render_3d;
mod scene;
mod streamer;
mod tray;

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use eframe::egui::{
    self, Align, Color32, FontId, Frame, Key, Layout, Pos2, Rect, RichText, Rounding, Sense,
    Stroke, Vec2, ViewportCommand,
};

use crate::protocol::{ClearLayer, NdjsonParser, VisualOp};
use crate::render_2d::render_2d_layer;
use crate::render_3d::render_3d_widget;
use crate::scene::SceneStore;
use crate::streamer::{capture_screen_jpeg_base64, spawn_stream, Provider, StreamEvent};
use crate::tray::{TrayAction, TrayController};

fn main() -> eframe::Result<()> {
    let _ = dotenvy::dotenv();
    let args: Vec<String> = std::env::args().collect();
    let start_overlay = args.iter().any(|a| a == "--overlay");
    let read_stdin = args.iter().any(|a| a == "--stdin");
    let initial_prompt = args
        .windows(2)
        .find(|w| w[0] == "--prompt")
        .map(|w| w[1].clone());

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("copain_t — Real-Time AI 2D/3D Visual Companion")
        .with_inner_size([1240.0, 800.0])
        .with_transparent(true);

    if start_overlay {
        viewport = viewport
            .with_decorations(false)
            .with_always_on_top()
            .with_maximized(true);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "copain_t",
        options,
        Box::new(move |cc| Ok(Box::new(CopainApp::new(start_overlay, initial_prompt, read_stdin, cc.egui_ctx.clone())))),
    )
}

struct CopainApp {
    rt: tokio::runtime::Runtime,
    scene: SceneStore,
    tx: Sender<StreamEvent>,
    rx: Receiver<StreamEvent>,
    prompt: String,
    provider: Provider,
    overlay_mode: bool,
    show_chrome: bool,
    capture_screen: bool,
    show_stream_log: bool,
    streaming: bool,
    status_text: String,
    last_frame: Instant,
    /// Active freehand stroke being drawn by the user on the canvas
    active_user_stroke: Vec<[f32; 2]>,
    last_passthrough: bool,
    pending_initial_prompt: Option<String>,
    tray: Option<TrayController>,
}

impl CopainApp {
    fn new(
        overlay_mode: bool,
        initial_prompt: Option<String>,
        read_stdin: bool,
        ctx: egui::Context,
    ) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        let (tx, rx) = mpsc::channel();
        let provider = Provider::detect_default();

        if read_stdin {
            let tx_in = tx.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                let mut parser = NdjsonParser::new();
                for line in stdin.lock().lines().map_while(Result::ok) {
                    let mut chunk = line;
                    chunk.push('\n');
                    for (raw, op) in parser.push_chunk(&chunk) {
                        let _ = tx_in.send(StreamEvent::Op(raw, op));
                        ctx.request_repaint();
                        std::thread::sleep(std::time::Duration::from_millis(140));
                    }
                }
                let _ = tx_in.send(StreamEvent::Done);
                ctx.request_repaint();
            });
        }

        Self {
            rt,
            scene: SceneStore::default(),
            tx,
            rx,
            prompt: initial_prompt.clone().unwrap_or_default(),
            provider,
            overlay_mode,
            show_chrome: !overlay_mode,
            capture_screen: overlay_mode,
            show_stream_log: false,
            streaming: read_stdin,
            status_text: if read_stdin {
                "Streaming from stdin…".to_string()
            } else {
                format!("Ready ({})", provider.label())
            },
            last_frame: Instant::now(),
            active_user_stroke: Vec::new(),
            last_passthrough: false,
            pending_initial_prompt: initial_prompt,
            tray: TrayController::new(),
        }
    }

    fn trigger_prompt(&mut self, ctx: &egui::Context, prompt_text: String) {
        if prompt_text.trim().is_empty() {
            return;
        }
        self.streaming = true;
        self.status_text = "Connecting…".to_string();
        let screenshot = if self.capture_screen && self.provider != Provider::Demo {
            capture_screen_jpeg_base64()
        } else {
            None
        };
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();
        spawn_stream(
            &self.rt,
            self.provider,
            prompt_text,
            screenshot,
            tx,
            move || ctx_clone.request_repaint(),
        );
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                StreamEvent::Op(raw, op) => self.scene.apply_op(op, Some(raw)),
                StreamEvent::Status(msg) => self.status_text = msg,
                StreamEvent::Done => {
                    self.streaming = false;
                    if !self.status_text.starts_with("Error:") {
                        self.status_text = format!(
                            "Done · {} 2D ops · {} 3D objects",
                            self.scene.primitives_2d.len(),
                            self.scene.objects_3d.len()
                        );
                    }
                }
                StreamEvent::Error(err) => {
                    self.streaming = false;
                    self.status_text = format!("Error: {err}");
                }
            }
        }
    }
}

impl eframe::App for CopainApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.overlay_mode {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            // Warm paper canvas background (#FAF9F6)
            [250.0 / 255.0, 249.0 / 255.0, 246.0 / 255.0, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        self.drain_events();

        // Handle macOS menu bar / tray actions
        if let Some(action) = self.tray.as_ref().and_then(|t| t.poll()) {
            match action {
                TrayAction::TogglePrompt => {
                    self.show_chrome = !self.show_chrome;
                    if self.show_chrome {
                        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(false));
                        self.last_passthrough = false;
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                }
                TrayAction::ToggleMode => {
                    self.overlay_mode = !self.overlay_mode;
                    self.show_chrome = !self.overlay_mode;
                    ctx.send_viewport_cmd(ViewportCommand::Decorations(!self.overlay_mode));
                    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.overlay_mode {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }));
                    if !self.overlay_mode {
                        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(false));
                        self.last_passthrough = false;
                    }
                }
                TrayAction::Clear => {
                    self.scene.clear(ClearLayer::All);
                    self.status_text = "Cleared all layers".to_string();
                }
                TrayAction::Quit => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        // Keep polling menu bar events even when idle
        if self.tray.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        if let Some(p) = self.pending_initial_prompt.take() {
            self.trigger_prompt(ctx, p);
        }

        // Global Esc shortcut clears all overlays immediately (per DESIGN.md risk mitigation)
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.scene.clear(ClearLayer::All);
            self.status_text = "Cleared all layers (Esc)".to_string();
        }
        // Tab toggles the toolbar/prompt chrome in Overlay Mode
        if self.overlay_mode && ctx.input(|i| i.key_pressed(Key::Tab)) {
            self.show_chrome = !self.show_chrome;
        }

        let animating = self.scene.tick(dt);
        if animating || self.streaming {
            ctx.request_repaint();
        }

        let mut interactive_rects: Vec<Rect> = Vec::new();

        // Top floating bar (hidden in bare overlay mode; press Tab to toggle)
        if self.show_chrome {
        let top_panel = egui::TopBottomPanel::top("top_bar")
            .frame(
                Frame::none()
                    .fill(if self.overlay_mode {
                        Color32::from_rgba_unmultiplied(20, 22, 30, 215)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 240)
                    })
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .stroke(Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(120, 125, 140, 45),
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Brand identity: copain_t
                    let ink = if self.overlay_mode {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(26, 26, 46)
                    };
                    ui.label(RichText::new("copain").strong().size(18.0).color(ink));
                    ui.add_space(-6.0);
                    ui.label(
                        RichText::new("_")
                            .size(15.0)
                            .color(Color32::from_rgb(130, 136, 150)),
                    );
                    ui.add_space(-6.0);
                    ui.label(
                        RichText::new("t")
                            .strong()
                            .italics()
                            .size(19.0)
                            .color(Color32::from_rgb(255, 85, 0)),
                    );

                    ui.separator();

                    // Quick Demo Presets
                    if ui.button("UI Guidance").clicked() {
                        self.trigger_prompt(ctx, "Show UI guidance for Export button".into());
                    }
                    if ui.button("3D Camera Frustum").clicked() {
                        self.trigger_prompt(ctx, "Show me a 3D diagram of a camera frustum".into());
                    }
                    if ui.button("Orbital 3D").clicked() {
                        self.trigger_prompt(ctx, "Visualize solar orbital mechanics in 3D".into());
                    }
                    if ui.button("Architecture").clicked() {
                        self.trigger_prompt(ctx, "Draw the copain_t architecture pipeline diagram".into());
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Overlay / Canvas Mode Toggle
                        let mode_label = if self.overlay_mode {
                            "Switch to Canvas"
                        } else {
                            "Overlay Mode"
                        };
                        if ui.button(mode_label).clicked() {
                            self.overlay_mode = !self.overlay_mode;
                            self.show_chrome = !self.overlay_mode;
                            ctx.send_viewport_cmd(ViewportCommand::Decorations(!self.overlay_mode));
                            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(
                                if self.overlay_mode {
                                    egui::WindowLevel::AlwaysOnTop
                                } else {
                                    egui::WindowLevel::Normal
                                },
                            ));
                            if !self.overlay_mode {
                                ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(false));
                                self.last_passthrough = false;
                            }
                        }

                        if ui.button("Clear (Esc)").clicked() {
                            self.scene.clear(ClearLayer::All);
                        }

                        ui.toggle_value(&mut self.show_stream_log, "NDJSON Log");

                        egui::ComboBox::from_id_salt("provider_select")
                            .selected_text(self.provider.label())
                            .show_ui(ui, |ui| {
                                for p in [
                                    Provider::Demo,
                                    Provider::OpenAI,
                                    Provider::Anthropic,
                                    Provider::Gemini,
                                ] {
                                    ui.selectable_value(&mut self.provider, p, p.label());
                                }
                            });
                    });
                });
            });
        interactive_rects.push(top_panel.response.rect);

        // Bottom prompt bar (safe width clamping + right-aligned controls so it never overflows)
        let bottom_panel = egui::TopBottomPanel::bottom("prompt_bar")
            .frame(
                Frame::none()
                    .fill(if self.overlay_mode {
                        Color32::from_rgba_unmultiplied(20, 22, 30, 235)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 245)
                    })
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                    .stroke(Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(120, 125, 140, 45),
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let input_w = (ui.available_width() - 320.0).clamp(220.0, 760.0);
                    let text_edit = ui.add(
                        egui::TextEdit::singleline(&mut self.prompt)
                            .desired_width(input_w)
                            .hint_text("Ask copain_t to sketch, annotate, or render a 3D diagram... (Enter to stream)"),
                    );
                    let enter_pressed = text_edit.lost_focus()
                        && ui.input(|i| i.key_pressed(Key::Enter));

                    if self.provider != Provider::Demo {
                        ui.checkbox(&mut self.capture_screen, "Screen");
                    }

                    let send_clicked = ui
                        .add_enabled(
                            !self.streaming,
                            egui::Button::new(
                                RichText::new(if self.streaming { "Streaming..." } else { "Stream >" })
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(255, 85, 0)),
                        )
                        .clicked();

                    if (send_clicked || enter_pressed) && !self.streaming {
                        let p = self.prompt.clone();
                        self.trigger_prompt(ctx, p);
                        if self.overlay_mode {
                            self.show_chrome = false;
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(&self.status_text)
                                .size(12.0)
                                .color(Color32::from_rgb(130, 138, 155)),
                        );
                    });
                });
            });
        interactive_rects.push(bottom_panel.response.rect);
        }

        // Optional NDJSON stream inspector drawer
        if self.show_stream_log {
            let side = egui::SidePanel::right("ndjson_log")
                .default_width(340.0)
                .show(ctx, |ui| {
                    ui.heading("NDJSON Stream");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.scene.stream_log {
                                ui.label(
                                    RichText::new(line)
                                        .monospace()
                                        .size(11.0)
                                        .color(Color32::from_rgb(80, 90, 110)),
                                );
                            }
                        });
                });
            interactive_rects.push(side.response.rect);
        }

        // Main 2D + 3D viewport
        egui::CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| {
                let viewport = ui.available_rect_before_wrap();

                // In Canvas Mode, draw a subtle notebook dot-grid and allow user freehand co-painting
                if !self.overlay_mode {
                    draw_dot_grid(ui.painter(), viewport);

                    let canvas_resp =
                        ui.interact(viewport, ui.id().with("user_ink_canvas"), Sense::drag());
                    if canvas_resp.dragged() {
                        if let Some(pos) = canvas_resp.interact_pointer_pos() {
                            let nx = ((pos.x - viewport.left()) / viewport.width()).clamp(0.0, 1.0);
                            let ny = ((pos.y - viewport.top()) / viewport.height()).clamp(0.0, 1.0);
                            self.active_user_stroke.push([nx, ny]);
                        }
                    }
                    if canvas_resp.drag_stopped() && self.active_user_stroke.len() >= 2 {
                        let pts = std::mem::take(&mut self.active_user_stroke);
                        self.scene.apply_op(
                            VisualOp::Stroke {
                                points: pts,
                                color: "#1A1A2E".to_string(),
                                width: 3.0,
                            },
                            None,
                        );
                    }
                }

                // Render 2D sketchy/ink layer
                render_2d_layer(ui.painter(), viewport, &self.scene);

                // Preview active user stroke while dragging
                if self.active_user_stroke.len() >= 2 {
                    let pts: Vec<Pos2> = self
                        .active_user_stroke
                        .iter()
                        .map(|p| {
                            Pos2::new(
                                viewport.left() + p[0] * viewport.width(),
                                viewport.top() + p[1] * viewport.height(),
                            )
                        })
                        .collect();
                    ui.painter().add(egui::Shape::line(
                        pts,
                        Stroke::new(2.8_f32, Color32::from_rgb(26, 26, 46)),
                    ));
                }

                // Render interactive 3D floating widget (if mounted)
                if let Some(widget_rect) = render_3d_widget(ui, viewport, &mut self.scene) {
                    interactive_rects.push(widget_rect);
                }

                // In bare Overlay Mode, show a tiny floating "Clear ×" pill in the top-right
                // so the user can always dismiss overlays even when keyboard focus is elsewhere.
                if self.overlay_mode
                    && !self.show_chrome
                    && (!self.scene.primitives_2d.is_empty() || self.scene.mount_3d.is_some())
                {
                    let pill_rect = Rect::from_min_size(
                        Pos2::new(viewport.right() - 92.0, viewport.top() + 14.0),
                        Vec2::new(78.0, 28.0),
                    );
                    let resp = ui.interact(pill_rect, ui.id().with("overlay_clear_pill"), Sense::click());
                    let bg = if resp.hovered() {
                        Color32::from_rgba_unmultiplied(239, 68, 68, 235)
                    } else {
                        Color32::from_rgba_unmultiplied(20, 24, 36, 200)
                    };
                    ui.painter().rect_filled(pill_rect, Rounding::same(14.0), bg);
                    ui.painter().rect_stroke(
                        pill_rect,
                        Rounding::same(14.0),
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 50)),
                    );
                    ui.painter().text(
                        pill_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Clear x",
                        FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                    if resp.clicked() {
                        self.scene.clear(ClearLayer::All);
                    }
                    interactive_rects.push(pill_rect);
                }

                // Empty-state onboarding hint in Canvas Mode
                if !self.overlay_mode
                    && self.scene.primitives_2d.is_empty()
                    && self.scene.mount_3d.is_none()
                {
                    let center = viewport.center();
                    let box_rect = Rect::from_center_size(center, Vec2::new(420.0, 96.0));
                    ui.painter().rect_filled(
                        box_rect,
                        Rounding::same(12.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 210),
                    );
                    ui.painter().rect_stroke(
                        box_rect,
                        Rounding::same(12.0),
                        Stroke::new(1.0_f32, Color32::from_rgb(225, 224, 218)),
                    );
                    ui.painter().text(
                        center - Vec2::new(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        "Click a preset above, type a prompt below, or sketch directly with your mouse.",
                        FontId::proportional(13.5),
                        Color32::from_rgb(60, 66, 82),
                    );
                    ui.painter().text(
                        center + Vec2::new(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        "Tip: Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or GEMINI_API_KEY for live multimodal streaming.",
                        FontId::proportional(11.5),
                        Color32::from_rgb(130, 138, 155),
                    );
                }
            });

        // Selective click-through in Overlay Mode:
        // Keep input active while the prompt bar/chrome is open; otherwise only capture
        // mouse events when hovering interactive UI (3D widget or Clear pill).
        if self.overlay_mode {
            let should_passthrough = if self.show_chrome {
                false
            } else {
                let hover_pos = ctx.input(|i| i.pointer.hover_pos());
                let over_interactive = hover_pos
                    .map(|p| interactive_rects.iter().any(|r| r.contains(p)))
                    .unwrap_or(false);
                !over_interactive
            };
            if should_passthrough != self.last_passthrough {
                ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(should_passthrough));
                self.last_passthrough = should_passthrough;
            }
        }
    }
}

fn draw_dot_grid(painter: &egui::Painter, rect: Rect) {
    let spacing = 24.0;
    let dot_color = Color32::from_rgb(218, 216, 208);
    let mut y = rect.top() + spacing;
    while y < rect.bottom() {
        let mut x = rect.left() + spacing;
        while x < rect.right() {
            painter.circle_filled(Pos2::new(x, y), 1.2, dot_color);
            x += spacing;
        }
        y += spacing;
    }
}

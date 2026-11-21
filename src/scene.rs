use eframe::egui::Color32;
use glam::Vec3;

use crate::protocol::{
    parse_hex_color, ClearLayer, DrawStyle, NormalizedBounds, Primitive3DType, VisualOp,
};

#[derive(Debug, Clone)]
pub enum Shape2D {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color32,
        style: DrawStyle,
        label: Option<String>,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: Color32,
        style: DrawStyle,
        label: Option<String>,
    },
    Arrow {
        from: [f32; 2],
        to: [f32; 2],
        color: Color32,
        label: Option<String>,
    },
    Stroke {
        points: Vec<[f32; 2]>,
        color: Color32,
        width: f32,
    },
    Label {
        x: f32,
        y: f32,
        text: String,
        color: Color32,
        size: f32,
    },
}

#[derive(Debug, Clone)]
pub struct Backdrop2D {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: Color32,
    pub radius: f32,
    pub progress: f32,
}

#[derive(Debug, Clone)]
pub struct Primitive2D {
    pub seed: u64,
    pub progress: f32,
    pub duration: f32,
    pub shape: Shape2D,
}

#[derive(Debug, Clone)]
pub struct PointerState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub label: Option<String>,
    pub color: Color32,
    pub pulse: f32,
}

#[derive(Debug, Clone)]
pub struct Mount3DState {
    pub bounds: NormalizedBounds,
    #[allow(dead_code)]
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Object3D {
    pub id: String,
    pub kind: Primitive3DType,
    pub pos: Vec3,
    pub base_pos: Vec3,
    pub rot: Vec3,
    pub size: Vec3,
    pub color: Color32,
    pub spin: Option<Vec3>,
    pub bob: Option<[f32; 2]>,
    pub text: Option<String>,
    pub points: Option<Vec<Vec3>>,
    pub wireframe: bool,
    pub age: f32,
    pub spawn_progress: f32,
}

#[derive(Debug, Clone)]
pub struct Camera3D {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            yaw: 0.65,
            pitch: 0.25,
            distance: 6.8,
        }
    }
}

pub struct SceneStore {
    pub backdrops: Vec<Backdrop2D>,
    pub primitives_2d: Vec<Primitive2D>,
    pub pointer: Option<PointerState>,
    pub mount_3d: Option<Mount3DState>,
    pub objects_3d: Vec<Object3D>,
    pub camera_3d: Camera3D,
    pub stream_log: Vec<String>,
    next_seed: u64,
}

impl Default for SceneStore {
    fn default() -> Self {
        Self {
            backdrops: Vec::new(),
            primitives_2d: Vec::new(),
            pointer: None,
            mount_3d: None,
            objects_3d: Vec::new(),
            camera_3d: Camera3D::default(),
            stream_log: Vec::new(),
            next_seed: 1,
        }
    }
}

impl SceneStore {
    pub fn clear(&mut self, layer: ClearLayer) {
        match layer {
            ClearLayer::TwoD => {
                self.backdrops.clear();
                self.primitives_2d.clear();
                self.pointer = None;
            }
            ClearLayer::ThreeD => {
                self.objects_3d.clear();
                self.mount_3d = None;
            }
            ClearLayer::All => {
                self.backdrops.clear();
                self.primitives_2d.clear();
                self.pointer = None;
                self.objects_3d.clear();
                self.mount_3d = None;
            }
        }
    }

    pub fn apply_op(&mut self, op: VisualOp, raw_line: Option<String>) {
        if let Some(line) = raw_line {
            self.stream_log.push(line);
            if self.stream_log.len() > 200 {
                self.stream_log.remove(0);
            }
        }

        let seed = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(1);

        match op {
            VisualOp::Clear { layer } => self.clear(layer),
            VisualOp::Backdrop {
                x,
                y,
                w,
                h,
                color,
                radius,
            } => {
                self.backdrops.push(Backdrop2D {
                    x,
                    y,
                    w,
                    h,
                    color: parse_hex_color(&color),
                    radius,
                    progress: 0.0,
                });
            }
            VisualOp::Pointer { x, y, label, color } => {
                let c = color
                    .as_deref()
                    .map(parse_hex_color)
                    .unwrap_or_else(|| Color32::from_rgb(255, 85, 0));
                if let Some(ptr) = &mut self.pointer {
                    ptr.target_x = x;
                    ptr.target_y = y;
                    ptr.label = label;
                    ptr.color = c;
                } else {
                    self.pointer = Some(PointerState {
                        x: (x - 0.08).clamp(0.0, 1.0),
                        y: (y - 0.08).clamp(0.0, 1.0),
                        vx: 0.0,
                        vy: 0.0,
                        target_x: x,
                        target_y: y,
                        label,
                        color: c,
                        pulse: 0.0,
                    });
                }
            }
            VisualOp::Rect {
                x,
                y,
                w,
                h,
                color,
                style,
                label,
            } => {
                self.primitives_2d.push(Primitive2D {
                    seed,
                    progress: 0.0,
                    duration: 0.38,
                    shape: Shape2D::Rect {
                        x,
                        y,
                        w,
                        h,
                        color: parse_hex_color(&color),
                        style,
                        label,
                    },
                });
            }
            VisualOp::Circle {
                x,
                y,
                r,
                color,
                style,
                label,
            } => {
                self.primitives_2d.push(Primitive2D {
                    seed,
                    progress: 0.0,
                    duration: 0.40,
                    shape: Shape2D::Circle {
                        x,
                        y,
                        r,
                        color: parse_hex_color(&color),
                        style,
                        label,
                    },
                });
            }
            VisualOp::Arrow {
                from,
                to,
                color,
                label,
            } => {
                self.primitives_2d.push(Primitive2D {
                    seed,
                    progress: 0.0,
                    duration: 0.35,
                    shape: Shape2D::Arrow {
                        from,
                        to,
                        color: parse_hex_color(&color),
                        label,
                    },
                });
            }
            VisualOp::Stroke {
                points,
                color,
                width,
            } => {
                let dur = (0.25 + (points.len() as f32) * 0.015).clamp(0.25, 0.75);
                self.primitives_2d.push(Primitive2D {
                    seed,
                    progress: 0.0,
                    duration: dur,
                    shape: Shape2D::Stroke {
                        points,
                        color: parse_hex_color(&color),
                        width,
                    },
                });
            }
            VisualOp::Label {
                x,
                y,
                text,
                color,
                size,
            } => {
                self.primitives_2d.push(Primitive2D {
                    seed,
                    progress: 0.0,
                    duration: 0.25,
                    shape: Shape2D::Label {
                        x,
                        y,
                        text,
                        color: parse_hex_color(&color),
                        size,
                    },
                });
            }
            VisualOp::Mount3D { bounds, title } => {
                self.mount_3d = Some(Mount3DState {
                    bounds,
                    title: title.unwrap_or_else(|| "3D Scene".to_string()),
                });
            }
            VisualOp::Unmount3D => {
                self.mount_3d = None;
            }
            VisualOp::Add3D {
                id,
                kind,
                pos,
                rot,
                size,
                color,
                spin,
                bob,
                text,
                points,
                wireframe,
            } => {
                // Auto-mount a default 3D widget if the model emitted 3d_add without 3d_mount
                if self.mount_3d.is_none() {
                    self.mount_3d = Some(Mount3DState {
                        bounds: NormalizedBounds {
                            x: 0.56,
                            y: 0.14,
                            w: 0.38,
                            h: 0.46,
                        },
                        title: "3D Scene".to_string(),
                    });
                }
                self.objects_3d.retain(|o| o.id != id);
                let vpos = Vec3::from_array(pos);
                self.objects_3d.push(Object3D {
                    id,
                    kind,
                    pos: vpos,
                    base_pos: vpos,
                    rot: Vec3::from_array(rot),
                    size: Vec3::from_array(size),
                    color: parse_hex_color(&color),
                    spin: spin.map(Vec3::from_array),
                    bob,
                    text,
                    points: points.map(|pts| pts.into_iter().map(Vec3::from_array).collect()),
                    wireframe,
                    age: 0.0,
                    spawn_progress: 0.0,
                });
            }
            VisualOp::Update3D {
                id,
                pos,
                rot,
                size,
                color,
                spin,
                text,
            } => {
                if let Some(obj) = self.objects_3d.iter_mut().find(|o| o.id == id) {
                    if let Some(p) = pos {
                        obj.pos = Vec3::from_array(p);
                        obj.base_pos = obj.pos;
                    }
                    if let Some(r) = rot {
                        obj.rot = Vec3::from_array(r);
                    }
                    if let Some(s) = size {
                        obj.size = Vec3::from_array(s);
                    }
                    if let Some(c) = color {
                        obj.color = parse_hex_color(&c);
                    }
                    if let Some(sp) = spin {
                        obj.spin = Some(Vec3::from_array(sp));
                    }
                    if let Some(t) = text {
                        obj.text = Some(t);
                    }
                }
            }
            VisualOp::Remove3D { id } => {
                self.objects_3d.retain(|o| o.id != id);
            }
        }
    }

    /// Advance all 2D/3D animations by `dt` seconds. Returns `true` if another frame should be requested.
    pub fn tick(&mut self, dt: f32) -> bool {
        let dt = dt.clamp(0.0, 0.05);
        let mut active = false;

        for bd in &mut self.backdrops {
            if bd.progress < 1.0 {
                bd.progress = (bd.progress + dt * 3.5).min(1.0);
                active = true;
            }
        }

        for prim in &mut self.primitives_2d {
            if prim.progress < 1.0 {
                prim.progress = (prim.progress + dt / prim.duration.max(0.05)).min(1.0);
                active = true;
            }
        }

        if let Some(ptr) = &mut self.pointer {
            // Critically damped-ish spring for smooth HeyClicky cursor motion
            let stiffness = 140.0;
            let damping = 18.0;
            let ax = (ptr.target_x - ptr.x) * stiffness - ptr.vx * damping;
            let ay = (ptr.target_y - ptr.y) * stiffness - ptr.vy * damping;
            ptr.vx += ax * dt;
            ptr.vy += ay * dt;
            ptr.x += ptr.vx * dt;
            ptr.y += ptr.vy * dt;
            ptr.pulse = (ptr.pulse + dt * 2.5) % std::f32::consts::TAU;
            active = true;
        }

        for obj in &mut self.objects_3d {
            obj.age += dt;
            if obj.spawn_progress < 1.0 {
                obj.spawn_progress = (obj.spawn_progress + dt * 3.0).min(1.0);
                active = true;
            }
            if let Some(spin) = obj.spin {
                // Spin is specified in radians per 60fps frame (e.g. 0.01) -> scale by dt * 60
                obj.rot += spin * (dt * 60.0);
                active = true;
            }
            if let Some([amp, speed]) = obj.bob {
                obj.pos.y = obj.base_pos.y + (obj.age * speed).sin() * amp;
                active = true;
            }
        }

        active
    }
}

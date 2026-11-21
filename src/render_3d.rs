use eframe::egui::{
    self,
    epaint::{Mesh, Vertex, WHITE_UV},
    Color32, FontId, Painter, Pos2, Rect, Rounding, Sense, Shape, Stroke, Ui, Vec2,
};
use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::protocol::Primitive3DType;
use crate::scene::{Camera3D, Object3D, SceneStore};

/// A triangle with per-vertex positions and smooth normals (for silky Gouraud shading).
struct SmoothTri {
    pos: [Vec3; 3],
    norm: [Vec3; 3],
}

struct ProjectedMeshTri {
    pts: [Pos2; 3],
    colors: [Color32; 3],
    depth: f32,
}

struct ProjectedLine {
    a: Pos2,
    b: Pos2,
    depth: f32,
    stroke: Stroke,
}

struct ProjectedBillboard {
    pos: Pos2,
    depth: f32,
    text: String,
    color: Color32,
}

/// Renders the interactive 3D scene directly in the shared canvas space (no separate box).
/// Returns the interactive 3D region's screen `Rect` (used for selective click-through in Overlay Mode).
pub fn render_3d_widget(ui: &mut Ui, viewport: Rect, scene: &mut SceneStore) -> Option<Rect> {
    let mount = scene.mount_3d.clone()?;

    let canvas_rect = Rect::from_min_size(
        Pos2::new(
            viewport.left() + mount.bounds.x * viewport.width(),
            viewport.top() + mount.bounds.y * viewport.height(),
        ),
        Vec2::new(
            (mount.bounds.w * viewport.width()).max(260.0),
            (mount.bounds.h * viewport.height()).max(240.0),
        ),
    )
    .intersect(viewport.shrink(6.0));

    let resp = ui.interact(canvas_rect, ui.id().with("3d_orbit"), Sense::drag());
    if resp.dragged() {
        let delta = resp.drag_delta();
        scene.camera_3d.yaw -= delta.x * 0.01;
        scene.camera_3d.pitch = (scene.camera_3d.pitch + delta.y * 0.01).clamp(-1.45, 1.45);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll.abs() > 0.01 {
            scene.camera_3d.distance = (scene.camera_3d.distance - scroll * 0.01).clamp(2.2, 18.0);
        }
    }

    let canvas_painter = ui.painter_at(canvas_rect);
    render_3d_scene(&canvas_painter, canvas_rect, &scene.camera_3d, &scene.objects_3d);

    // Subtle unobtrusive orbit hint only when hovered
    if resp.hovered() {
        canvas_painter.text(
            Pos2::new(canvas_rect.center().x, canvas_rect.bottom() - 10.0),
            egui::Align2::CENTER_BOTTOM,
            "Drag to orbit · Scroll to zoom",
            FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(110, 118, 134, 180),
        );
    }

    Some(canvas_rect)
}

fn shade_vertex(
    base: Color32,
    world_pos: Vec3,
    world_normal: Vec3,
    eye: Vec3,
    key_dir: Vec3,
    fill_dir: Vec3,
) -> Color32 {
    let view_dir = (eye - world_pos).normalize_or_zero();
    let n = if world_normal.dot(view_dir) >= 0.0 {
        world_normal
    } else {
        -world_normal
    };
    let key_diff = n.dot(key_dir).max(0.0);
    let fill_diff = n.dot(fill_dir).max(0.0) * 0.25;
    let half_vec = (key_dir + view_dir).normalize_or_zero();
    let spec = n.dot(half_vec).max(0.0).powi(28) * 0.32;
    let rim = (1.0 - n.dot(view_dir).clamp(0.0, 1.0)).powi(3) * 0.16;
    let hemi = 0.28 + 0.10 * (n.y * 0.5 + 0.5);
    let shade = (hemi + key_diff * 0.68 + fill_diff + rim).clamp(0.18, 1.12);

    let r = ((base.r() as f32) * shade + spec * 255.0).min(255.0) as u8;
    let g = ((base.g() as f32) * shade + spec * 255.0).min(255.0) as u8;
    let b = ((base.b() as f32) * shade + spec * 255.0).min(255.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, base.a())
}

fn render_3d_scene(painter: &Painter, rect: Rect, cam: &Camera3D, objects: &[Object3D]) {
    let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);
    let eye = Vec3::new(
        cam.distance * cam.pitch.cos() * cam.yaw.sin(),
        cam.distance * cam.pitch.sin(),
        cam.distance * cam.pitch.cos() * cam.yaw.cos(),
    );
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
    let proj = Mat4::perspective_rh(42.0_f32.to_radians(), aspect, 0.1, 100.0);
    let vp = proj * view;

    let project = |p: Vec3| -> Option<(Pos2, f32)> {
        let clip = vp * p.extend(1.0);
        if clip.w <= 0.05 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let sx = rect.left() + (ndc.x * 0.5 + 0.5) * rect.width();
        let sy = rect.top() + (0.5 - ndc.y * 0.5) * rect.height();
        Some((Pos2::new(sx, sy), clip.w))
    };

    // Subtle ground reference grid in warm light-canvas ink
    let grid_y = -2.0;
    let grid_color = Color32::from_rgba_unmultiplied(150, 158, 175, 42);
    for i in -3..=3 {
        let f = i as f32 * 0.8;
        if let (Some((a, _)), Some((b, _))) = (
            project(Vec3::new(f, grid_y, -2.4)),
            project(Vec3::new(f, grid_y, 2.4)),
        ) {
            painter.line_segment([a, b], Stroke::new(1.0_f32, grid_color));
        }
        if let (Some((a, _)), Some((b, _))) = (
            project(Vec3::new(-2.4, grid_y, f)),
            project(Vec3::new(2.4, grid_y, f)),
        ) {
            painter.line_segment([a, b], Stroke::new(1.0_f32, grid_color));
        }
    }

    let key_dir = Vec3::new(0.6, 0.8, 0.55).normalize();
    let fill_dir = Vec3::new(-0.7, 0.25, -0.4).normalize();

    let mut mesh_tris: Vec<ProjectedMeshTri> = Vec::new();
    let mut lines: Vec<ProjectedLine> = Vec::new();
    let mut billboards: Vec<ProjectedBillboard> = Vec::new();

    for obj in objects {
        let scale_ease = 1.0 - (1.0 - obj.spawn_progress.clamp(0.0, 1.0)).powi(3);
        let rot_q = Quat::from_euler(EulerRot::XYZ, obj.rot.x, obj.rot.y, obj.rot.z);
        let model = Mat4::from_scale_rotation_translation(
            obj.size * scale_ease.max(0.01),
            rot_q,
            obj.pos,
        );

        match obj.kind {
            Primitive3DType::Billboard => {
                if let Some((pos, depth)) = project(obj.pos) {
                    billboards.push(ProjectedBillboard {
                        pos,
                        depth,
                        text: obj.text.clone().unwrap_or_else(|| obj.id.clone()),
                        color: obj.color,
                    });
                }
            }
            Primitive3DType::Line => {
                if let Some(pts) = &obj.points {
                    for w in pts.windows(2) {
                        let a_world = (model * w[0].extend(1.0)).truncate();
                        let b_world = (model * w[1].extend(1.0)).truncate();
                        if let (Some((pa, da)), Some((pb, db))) =
                            (project(a_world), project(b_world))
                        {
                            lines.push(ProjectedLine {
                                a: pa,
                                b: pb,
                                depth: (da + db) * 0.5,
                                stroke: Stroke::new(2.0_f32, obj.color),
                            });
                        }
                    }
                }
            }
            _ => {
                let tris = build_primitive_mesh(obj.kind);
                let cull_backfaces = !obj.wireframe && obj.kind != Primitive3DType::Plane;

                for tri in tris {
                    let w0 = (model * tri.pos[0].extend(1.0)).truncate();
                    let w1 = (model * tri.pos[1].extend(1.0)).truncate();
                    let w2 = (model * tri.pos[2].extend(1.0)).truncate();

                    let face_normal = (w1 - w0).cross(w2 - w0).normalize_or_zero();
                    let center = (w0 + w1 + w2) / 3.0;
                    let view_dir = (eye - center).normalize_or_zero();

                    if cull_backfaces && face_normal.dot(view_dir) < -0.02 {
                        continue;
                    }

                    if let (Some((p0, d0)), Some((p1, d1)), Some((p2, d2))) =
                        (project(w0), project(w1), project(w2))
                    {
                        if obj.wireframe {
                            let st = Stroke::new(1.1_f32, obj.color);
                            lines.push(ProjectedLine { a: p0, b: p1, depth: (d0 + d1) * 0.5, stroke: st });
                            lines.push(ProjectedLine { a: p1, b: p2, depth: (d1 + d2) * 0.5, stroke: st });
                            lines.push(ProjectedLine { a: p2, b: p0, depth: (d2 + d0) * 0.5, stroke: st });
                        } else {
                            // Transform smooth vertex normals for seamless Gouraud shading
                            let n0 = (rot_q * tri.norm[0]).normalize_or_zero();
                            let n1 = (rot_q * tri.norm[1]).normalize_or_zero();
                            let n2 = (rot_q * tri.norm[2]).normalize_or_zero();
                            let c0 = shade_vertex(obj.color, w0, n0, eye, key_dir, fill_dir);
                            let c1 = shade_vertex(obj.color, w1, n1, eye, key_dir, fill_dir);
                            let c2 = shade_vertex(obj.color, w2, n2, eye, key_dir, fill_dir);
                            mesh_tris.push(ProjectedMeshTri {
                                pts: [p0, p1, p2],
                                colors: [c0, c1, c2],
                                depth: (d0 + d1 + d2) / 3.0,
                            });
                        }
                    }
                }
            }
        }
    }

    // Painter's algorithm: sort back-to-front and emit ONE seamless GPU Mesh (zero AA spikes/seams!)
    mesh_tris.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    if !mesh_tris.is_empty() {
        let mut gpu_mesh = Mesh::default();
        gpu_mesh.vertices.reserve(mesh_tris.len() * 3);
        gpu_mesh.indices.reserve(mesh_tris.len() * 3);
        for t in mesh_tris {
            let base = gpu_mesh.vertices.len() as u32;
            for i in 0..3 {
                gpu_mesh.vertices.push(Vertex {
                    pos: t.pts[i],
                    uv: WHITE_UV,
                    color: t.colors[i],
                });
            }
            gpu_mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
        painter.add(Shape::mesh(gpu_mesh));
    }

    lines.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    for l in lines {
        painter.line_segment([l.a, l.b], l.stroke);
    }

    billboards.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    let mut placed_rects: Vec<Rect> = Vec::with_capacity(billboards.len());
    for bb in billboards {
        let galley = painter.layout_no_wrap(
            bb.text,
            FontId::proportional(11.5),
            Color32::from_rgb(26, 30, 44),
        );
        let size = galley.size() + Vec2::new(14.0, 7.0);
        let mut center = bb.pos;
        // Nudge apart if this billboard overlaps any already-placed billboard
        for _ in 0..8 {
            let candidate = Rect::from_center_size(center, size);
            if let Some(hit) = placed_rects.iter().find(|r| r.expand(2.0).intersects(candidate)) {
                let dy = if center.y <= hit.center().y {
                    -(hit.height() * 0.5 + size.y * 0.5 + 4.0) + (hit.center().y - center.y)
                } else {
                    (hit.height() * 0.5 + size.y * 0.5 + 4.0) - (center.y - hit.center().y)
                };
                center.y += dy.clamp(-24.0, 24.0);
            } else {
                break;
            }
        }
        let bg = Rect::from_center_size(center, size);
        placed_rects.push(bg);
        painter.rect_filled(
            bg.translate(Vec2::new(0.0, 2.0)),
            Rounding::same(7.0),
            Color32::from_rgba_unmultiplied(15, 20, 35, 28),
        );
        painter.rect_filled(
            bg,
            Rounding::same(7.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, 238),
        );
        painter.rect_stroke(bg, Rounding::same(7.0), Stroke::new(1.2_f32, bb.color));
        painter.galley(bg.min + Vec2::new(7.0, 3.5), galley, Color32::from_rgb(26, 30, 44));
    }
}

fn flat_tri(a: Vec3, b: Vec3, c: Vec3) -> SmoothTri {
    let n = (b - a).cross(c - a).normalize_or_zero();
    SmoothTri {
        pos: [a, b, c],
        norm: [n, n, n],
    }
}

fn build_primitive_mesh(kind: Primitive3DType) -> Vec<SmoothTri> {
    match kind {
        Primitive3DType::Box => mesh_box(),
        Primitive3DType::Sphere => mesh_sphere(16, 28),
        Primitive3DType::Cylinder => mesh_cylinder(40),
        Primitive3DType::Cone => mesh_cone(40),
        Primitive3DType::Torus => mesh_torus(32, 16, 0.5, 0.16),
        Primitive3DType::Plane => mesh_plane(),
        Primitive3DType::Line | Primitive3DType::Billboard => Vec::new(),
    }
}

fn mesh_box() -> Vec<SmoothTri> {
    let h = 0.5;
    let v = [
        Vec3::new(-h, -h, h),
        Vec3::new(h, -h, h),
        Vec3::new(h, h, h),
        Vec3::new(-h, h, h),
        Vec3::new(-h, -h, -h),
        Vec3::new(h, -h, -h),
        Vec3::new(h, h, -h),
        Vec3::new(-h, h, -h),
    ];
    let idx = [
        [0, 1, 2], [0, 2, 3], // front
        [5, 4, 7], [5, 7, 6], // back
        [4, 0, 3], [4, 3, 7], // left
        [1, 5, 6], [1, 6, 2], // right
        [3, 2, 6], [3, 6, 7], // top
        [4, 5, 1], [4, 1, 0], // bottom
    ];
    idx.iter().map(|[a, b, c]| flat_tri(v[*a], v[*b], v[*c])).collect()
}

fn mesh_plane() -> Vec<SmoothTri> {
    let h = 0.5;
    let a = Vec3::new(-h, 0.0, -h);
    let b = Vec3::new(h, 0.0, -h);
    let c = Vec3::new(h, 0.0, h);
    let d = Vec3::new(-h, 0.0, h);
    vec![flat_tri(a, b, c), flat_tri(a, c, d)]
}

fn mesh_sphere(stacks: usize, slices: usize) -> Vec<SmoothTri> {
    let mut tris = Vec::new();
    let r = 0.5;
    for i in 0..stacks {
        let phi0 = std::f32::consts::PI * (i as f32 / stacks as f32);
        let phi1 = std::f32::consts::PI * ((i + 1) as f32 / stacks as f32);
        for j in 0..slices {
            let th0 = std::f32::consts::TAU * (j as f32 / slices as f32);
            let th1 = std::f32::consts::TAU * ((j + 1) as f32 / slices as f32);

            let p = |phi: f32, th: f32| {
                let n = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
                (n * r, n)
            };
            let (p00, n00) = p(phi0, th0);
            let (p10, n10) = p(phi1, th0);
            let (p11, n11) = p(phi1, th1);
            let (p01, n01) = p(phi0, th1);
            if i > 0 {
                tris.push(SmoothTri { pos: [p00, p01, p10], norm: [n00, n01, n10] });
            }
            if i + 1 < stacks {
                tris.push(SmoothTri { pos: [p10, p01, p11], norm: [n10, n01, n11] });
            }
        }
    }
    tris
}

fn mesh_cylinder(segs: usize) -> Vec<SmoothTri> {
    let mut tris = Vec::new();
    let r = 0.5;
    let h = 0.5;
    let top_c = Vec3::new(0.0, h, 0.0);
    let bot_c = Vec3::new(0.0, -h, 0.0);
    for i in 0..segs {
        let a0 = std::f32::consts::TAU * (i as f32 / segs as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / segs as f32);
        let n0 = Vec3::new(a0.cos(), 0.0, a0.sin());
        let n1 = Vec3::new(a1.cos(), 0.0, a1.sin());
        let t0 = Vec3::new(r * a0.cos(), h, r * a0.sin());
        let t1 = Vec3::new(r * a1.cos(), h, r * a1.sin());
        let b0 = Vec3::new(r * a0.cos(), -h, r * a0.sin());
        let b1 = Vec3::new(r * a1.cos(), -h, r * a1.sin());
        // Smooth radial normals on side walls
        tris.push(SmoothTri { pos: [t0, t1, b1], norm: [n0, n1, n1] });
        tris.push(SmoothTri { pos: [t0, b1, b0], norm: [n0, n1, n0] });
        // Flat caps
        tris.push(SmoothTri { pos: [top_c, t1, t0], norm: [Vec3::Y, Vec3::Y, Vec3::Y] });
        tris.push(SmoothTri { pos: [bot_c, b0, b1], norm: [-Vec3::Y, -Vec3::Y, -Vec3::Y] });
    }
    tris
}

fn mesh_cone(segs: usize) -> Vec<SmoothTri> {
    let mut tris = Vec::new();
    let r = 0.5;
    let h = 0.5;
    let apex = Vec3::new(0.0, h, 0.0);
    let bot_c = Vec3::new(0.0, -h, 0.0);
    let slope_y = r / (r * r + (2.0 * h) * (2.0 * h)).sqrt();
    for i in 0..segs {
        let a0 = std::f32::consts::TAU * (i as f32 / segs as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / segs as f32);
        let am = (a0 + a1) * 0.5;
        let n0 = Vec3::new(a0.cos(), slope_y, a0.sin()).normalize();
        let n1 = Vec3::new(a1.cos(), slope_y, a1.sin()).normalize();
        let nm = Vec3::new(am.cos(), slope_y, am.sin()).normalize();
        let b0 = Vec3::new(r * a0.cos(), -h, r * a0.sin());
        let b1 = Vec3::new(r * a1.cos(), -h, r * a1.sin());
        tris.push(SmoothTri { pos: [apex, b1, b0], norm: [nm, n1, n0] });
        tris.push(SmoothTri { pos: [bot_c, b0, b1], norm: [-Vec3::Y, -Vec3::Y, -Vec3::Y] });
    }
    tris
}

fn mesh_torus(major_segs: usize, minor_segs: usize, major_r: f32, minor_r: f32) -> Vec<SmoothTri> {
    let mut tris = Vec::new();
    let pt = |u: f32, v: f32| {
        let center = Vec3::new(major_r * u.cos(), 0.0, major_r * u.sin());
        let r = major_r + minor_r * v.cos();
        let pos = Vec3::new(r * u.cos(), minor_r * v.sin(), r * u.sin());
        let norm = (pos - center).normalize_or_zero();
        (pos, norm)
    };
    for i in 0..major_segs {
        let u0 = std::f32::consts::TAU * (i as f32 / major_segs as f32);
        let u1 = std::f32::consts::TAU * ((i + 1) as f32 / major_segs as f32);
        for j in 0..minor_segs {
            let v0 = std::f32::consts::TAU * (j as f32 / minor_segs as f32);
            let v1 = std::f32::consts::TAU * ((j + 1) as f32 / minor_segs as f32);
            let (p00, n00) = pt(u0, v0);
            let (p10, n10) = pt(u1, v0);
            let (p11, n11) = pt(u1, v1);
            let (p01, n01) = pt(u0, v1);
            tris.push(SmoothTri { pos: [p00, p10, p11], norm: [n00, n10, n11] });
            tris.push(SmoothTri { pos: [p00, p11, p01], norm: [n00, n11, n01] });
        }
    }
    tris
}

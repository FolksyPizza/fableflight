//! Render an imported aircraft's real mesh for the hangar without touching
//! the flight renderer or blocking the UI while the model loads.
use crate::{airframe::{Model, Vis}, fgmodel::{Ext, M_TEX}, render::PartVertex};
use glam::{Quat, Vec2, Vec3};
use std::{sync::{Arc, mpsc}, time::{Duration, Instant}};

const WIDTH: usize = 520;
const HEIGHT: usize = 300;

struct Frame {
    model: Arc<Model>,
    angle: [f32; 2],
    pixels: Vec<u8>,
}

pub struct Preview {
    model_id: &'static Ext,
    angles: [f32; 2],
    shown: Option<[f32; 2]>,
    ready_at: Instant,
    model: Option<Arc<Model>>,
    pending: Option<mpsc::Receiver<Option<Frame>>>,
    texture: Option<egui::TextureHandle>,
    failed: bool,
}

impl Preview {
    pub fn new(model_id: &'static Ext) -> Self {
        Self {
            model_id, angles: [-0.65, -0.22], shown: None,
            ready_at: Instant::now() + Duration::from_millis(140),
            model: None, pending: None, texture: None, failed: false,
        }
    }

    pub fn matches(&self, ext: &Ext) -> bool { std::ptr::eq(self.model_id, ext) }

    pub fn draw(&mut self, ctx: &egui::Context, drag: egui::Vec2) -> Option<&egui::TextureHandle> {
        self.angles[0] = (self.angles[0] + drag.x * 0.012).rem_euclid(std::f32::consts::TAU);
        self.angles[1] = (self.angles[1] + drag.y * 0.010).clamp(-1.1, 0.65);
        if let Some(rx) = self.pending.as_ref() {
            match rx.try_recv() {
                Ok(Some(frame)) => {
                    let image = egui::ColorImage::from_rgba_unmultiplied([WIDTH, HEIGHT], &frame.pixels);
                    if let Some(tex) = &mut self.texture {
                        tex.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        self.texture = Some(ctx.load_texture("aircraft orbit", image, egui::TextureOptions::LINEAR));
                    }
                    self.shown = Some(frame.angle);
                    self.model = Some(frame.model);
                    self.pending = None;
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.failed = true;
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.failed && self.pending.is_none() && self.shown != Some(self.angles) {
            if let Some(wait) = self.ready_at.checked_duration_since(Instant::now()) {
                ctx.request_repaint_after(wait);
            } else {
                let (tx, rx) = mpsc::sync_channel(1);
                let (ext, angle, model, ctx) = (self.model_id, self.angles, self.model.clone(), ctx.clone());
                let worker = std::thread::Builder::new().name("aircraft preview".into()).spawn(move || {
                    let model = model.or_else(|| {
                        let mut model = ext.build().ok()?;
                        model.groups.clear();
                        Some(Arc::new(model))
                    });
                    let result = model.and_then(|model| Some(Frame { pixels: raster(&model, angle)?, model, angle }));
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
                if worker.is_ok() { self.pending = Some(rx); } else { self.failed = true; }
            }
        }
        self.texture.as_ref()
    }
}

fn raster(model: &Model, angle: [f32; 2]) -> Option<Vec<u8>> {
    let meshes = || model.pieces.iter().filter(|p| !matches!(p.vis, Vis::FastProp | Vis::Chute)).map(|p| &p.mesh);
    if meshes().map(|mesh| mesh.i.len()).sum::<usize>() > 6_000_000 { return None; }
    let rot = Quat::from_rotation_x(angle[1]) * Quat::from_rotation_y(angle[0]);
    let bounds = meshes().flat_map(|mesh| &mesh.v).filter_map(|vertex| {
        let p = Vec3::from(vertex.pos);
        p.is_finite().then_some(p)
    }).fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(lo, hi), p| (lo.min(p), hi.max(p)));
    let center = (bounds.0 + bounds.1) * 0.5;
    let size = (bounds.1 - bounds.0).length();
    if !size.is_finite() || size < 0.01 { return None; }
    // Fit a stable sphere so zoom stays constant as the plane turns.
    let scale = HEIGHT as f32 * 0.88 / size;
    let light = Vec3::new(-0.34, 0.76, 0.54).normalize();
    let srgb = |v: f32| (v.max(0.0).min(1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
    let mut pixels = vec![0; WIDTH * HEIGHT * 4];
    let mut depth = vec![f32::NEG_INFINITY; WIDTH * HEIGHT];
    for mesh in meshes() {
        let projected: Vec<_> = mesh.v.iter().map(|v| {
            let v = rot * (Vec3::from(v.pos) - center);
            Vec3::new(WIDTH as f32 * 0.5 + v.x * scale, HEIGHT as f32 * 0.5 - v.y * scale, v.z)
        }).collect();
        for tri in mesh.i.chunks_exact(3) {
            let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let [p, q, r] = ids.map(|i| projected[i]);
            let a = p.truncate();
            let e = q.truncate() - a;
            let f = r.truncate() - a;
            let determinant = e.perp_dot(f);
            if !determinant.is_finite() || determinant.abs() < 0.005 { continue; }
            let lo = a.min(q.truncate()).min(r.truncate()).floor().max(Vec2::ZERO);
            let hi = a.max(q.truncate()).max(r.truncate()).ceil().min(Vec2::new(WIDTH as f32, HEIGHT as f32));
            let [u, v, w] = ids.map(|i| mesh.v[i]);
            let layer = texture(model, u);
            let normal = rot * (Vec3::from(u.nrm) + Vec3::from(v.nrm) + Vec3::from(w.nrm)).normalize_or_zero();
            let shade = 0.37 + 0.63 * normal.dot(light).max(0.0);
            let rgb = Vec3::new(u.col[0], u.col[1], u.col[2]);
            let flat = [srgb(rgb.x * shade), srgb(rgb.y * shade), srgb(rgb.z * shade), 255];
            let lit = shade.powf(1.0 / 2.2);
            let tint = rgb.to_array().map(|v| f32::from(srgb(v)) * (lit / 255.0));
            let uv = [u.uv, v.uv, w.uv].map(Vec2::from);
            for y in lo.y as usize..hi.y as usize {
                for x in lo.x as usize..hi.x as usize {
                    let t = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - a;
                    let b = t.perp_dot(f) / determinant;
                    let c = e.perp_dot(t) / determinant;
                    let a = 1.0 - b - c;
                    if a < 0.0 || b < 0.0 || c < 0.0 { continue; }
                    let z = p.z * a + q.z * b + r.z * c;
                    let i = y * WIDTH + x;
                    if z <= depth[i] { continue; }
                    let color = if let Some((size, data)) = layer {
                        let xy = (uv[0] * a + uv[1] * b + uv[2] * c).rem_euclid(Vec2::ONE) * size as f32;
                        let j = ((xy.y as usize).min(size - 1) * size + (xy.x as usize).min(size - 1)) * 4;
                        if data[j + 3] < 128 { continue; }
                        [(f32::from(data[j]) * tint[0]) as u8,
                            (f32::from(data[j + 1]) * tint[1]) as u8,
                            (f32::from(data[j + 2]) * tint[2]) as u8, 255]
                    } else { flat };
                    pixels[i * 4..i * 4 + 4].copy_from_slice(&color);
                    depth[i] = z;
                }
            }
        }
    }
    Some(pixels)
}

fn texture(model: &Model, v: PartVertex) -> Option<(usize, &[u8])> {
    if v.mat[0] != M_TEX || v.mat[3] < 0.0 { return None; }
    let set = model.tex.as_ref()?;
    let size = set.size as usize;
    set.layers.get(v.mat[3] as usize)
        .filter(|data| size > 0 && data.len() == size * size * 4)
        .map(|data| (size, data.as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airframe::{Builder, Mesh, G_FUSE};

    #[test]
    fn orbit_renders_geometry_from_distinct_angles() {
        let mesh = Mesh {
            v: [[-1.0, -0.3, 0.0], [1.0, -0.3, 0.0], [0.0, 0.7, 0.0]].into_iter()
                .map(|pos| PartVertex { pos, nrm: [0.0, 0.0, 1.0], col: [0.83, 0.66, 0.45, 1.0], ..Default::default() })
                .collect(),
            i: vec![0, 1, 2],
        };
        let mut builder = Builder::default();
        builder.fixed("test", G_FUSE, mesh);
        let model = Model { pieces: builder.pieces, groups: Vec::new(), tex: None };
        let count = |angle| raster(&model, angle).unwrap().chunks_exact(4).filter(|pixel| pixel[3] != 0).count();
        let front = count([0.0, 0.0]);
        let side = count([1.1, 0.0]);
        assert!(front > 1000 && side < front && side > 100);
    }
}

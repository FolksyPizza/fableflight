//! Interactive handoff from the departure camera to the aircraft.
use crate::{Game, Loading, Plane, cars, flightui, menu, padui::Hint, render::Inst, t};
use glam::{DQuat, DVec2, DVec3, Mat4, Vec3};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Airborne,
    Walkaround(u8),
}

const CHECKS: u8 = 3;

impl Stage {
    fn next(self) -> Option<Self> {
        match self {
            Self::Walkaround(step) if step < CHECKS => Some(Self::Walkaround(step + 1)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Walkaround {
    point: DVec2,
    yaw: f64,
    pitch: f64,
    stride: f64,
}

impl Walkaround {
    pub fn new(plane: &Plane) -> Self {
        let side = f64::from(plane.spec.span).max(5.0);
        Self { point: DVec2::new(-0.53 * side, 1.0), yaw: 1.57, pitch: 0.0, stride: 0.0 }
    }

    fn axes(plane: &Plane) -> (DVec3, DVec3, DVec3) {
        let up = plane.pos.normalize();
        let forward = (plane.forward() - up * plane.forward().dot(up))
            .try_normalize().unwrap_or_else(|| crate::geo::enu(plane.pos).1);
        (forward.cross(up), forward, up)
    }

    fn station(plane: &Plane, step: u8) -> DVec2 {
        let span = f64::from(plane.spec.span);
        let door = plane.spec.eye.as_dvec3();
        let clearance = (span * 0.085 + 0.9).max(1.6);
        match step {
            0 => DVec2::new(-clearance, -door.z + 0.74),
            1 => plane.spec.gear.iter().find(|leg| leg.brakes && leg.at.x >= 0.0)
                .map_or(DVec2::new(clearance, 0.0), |leg| DVec2::new(leg.at.x + 1.3, -leg.at.z)),
            2 => DVec2::new(span * 0.45 + 0.5, -0.8),
            _ => DVec2::new(-clearance, -door.z),
        }
    }

    fn at_station(self, plane: &Plane, step: u8) -> bool {
        self.point.distance(Self::station(plane, step)) <= 2.1
    }

    fn move_by(&mut self, plane: &Plane, delta: DVec2) {
        let span = f64::from(plane.spec.span).max(6.0);
        let next = self.point + delta;
        let fuselage = next / DVec2::new((span * 0.055 + 0.50).max(1.0), span * 0.37 + 0.6);
        if next.length() <= span * 1.4 + 12.0 && fuselage.length_squared() > 1.0 {
            self.point = next;
        }
    }

    pub fn camera(self, plane: &Plane, world: &crate::world::World) -> (DVec3, DVec3, DVec3) {
        let (right, forward, up) = Self::axes(plane);
        let floor = plane.pos + right * self.point.x + forward * self.point.y - up * plane.stand_height();
        let (lat, lon, fallback) = crate::geo::pos_to_lla(floor);
        let h = world.terrain_ground(lat, lon).map_or(fallback, |g| g.h);
        let eye = crate::geo::lla_to_pos(lat, lon, h + 1.69);
        let dir = (forward * self.yaw.cos() + right * self.yaw.sin()) * self.pitch.cos() + up * self.pitch.sin();
        (eye, dir, up)
    }
}

pub fn airborne_camera(plane: &Plane, elapsed: f64) -> (DVec3, DVec3, DVec3) {
    let up = plane.pos.normalize();
    let back = (-plane.forward() + up * plane.forward().dot(up)).normalize_or_zero();
    let back = if back == DVec3::ZERO { crate::geo::enu(plane.pos).1 } else { back };
    let scale = plane.spec.cam_dist.max(f64::from(plane.spec.span) * 0.94);
    // Start behind the wing, drift to the other side, then circle back.
    let angle = (28.0 + 112.0 * Loading::ease((elapsed - 2.2) / 5.5)
        - 166.0 * Loading::ease((elapsed - 9.0) / 7.5)
        + 54.0 * Loading::ease((elapsed - 19.0) / 6.0)).to_radians();
    let rise = 0.15 + 0.09 * Loading::ease((elapsed - 2.0) / 7.0);
    let offset = DQuat::from_axis_angle(up, angle) * back * scale + up * (scale * rise);
    (plane.centre() + offset, -offset.normalize(), up)
}

pub fn equipment(plane: &Plane, cam: DVec3, step: u8, out: &mut Vec<(u32, Inst)>) {
    let mut block = |position: DVec3, half: Vec3, rgb: [f32; 3]| {
        let m = Mat4::from_scale_rotation_translation(half, plane.rot.as_quat(), (position - cam).as_vec3());
        let [x, y, z, p] = m.to_cols_array_2d();
        out.push((cars::M_CUBE, Inst {
            m: [[x[0], y[0], z[0], p[0]], [x[1], y[1], z[1], p[1]], [x[2], y[2], z[2], p[2]]],
            tint: [rgb[0], rgb[1], rgb[2], 1.0], ext: [0.0; 4],
        }));
    };
    if step == 0 {
        let at = plane.model_pos() + plane.rot * (plane.spec.eye.as_dvec3() + DVec3::new(0.34, -0.28, -0.74));
        block(at, Vec3::new(0.07, 0.24, 0.045), [0.68, 0.04, 0.03]);
    }
    if step < 2 {
        for leg in plane.spec.gear.iter().filter(|leg| leg.brakes) {
            let r = leg.wheel_r;
            let contact = plane.pos + plane.rot * leg.at;
            for side in [-1.0, 1.0] {
                let at = contact + plane.up() * (r * 0.15) + plane.forward() * (r * 1.07 * side);
                block(at, Vec3::new((r * 0.75) as f32, (r * 0.15) as f32, (r * 0.22) as f32), [0.86, 0.48, 0.04]);
            }
        }
    }
}

impl Game {
    pub(crate) fn toggle_briefing_view(&mut self) {
        self.preflight_cockpit = !self.preflight_cockpit;
        self.cam_mode = u8::from(self.preflight_cockpit);
        self.recenter_view();
    }

    pub(crate) fn move_walkaround(&mut self, dt: f64) {
        let Some(walk) = &mut self.walkaround else { return };
        let axis = |positive, negative| f64::from(self.keys.contains(&positive) as u8)
            - f64::from(self.keys.contains(&negative) as u8);
        use gilrs::Axis;
        let pad = self.pad_id.and_then(|id| self.gilrs.as_ref().map(|g| g.gamepad(id)));
        let stick = |a| pad.as_ref().map_or(0.0, |p| {
            let x = f64::from(p.value(a));
            if x.abs() > 0.15 { x } else { 0.0 }
        });
        walk.yaw += (axis(KeyCode::ArrowRight, KeyCode::ArrowLeft) + stick(Axis::RightStickX)) * dt * 1.65;
        walk.pitch = (walk.pitch + (axis(KeyCode::ArrowUp, KeyCode::ArrowDown) + stick(Axis::RightStickY)) * dt * 1.5).clamp(-1.38, 1.38);
        let side = axis(KeyCode::KeyD, KeyCode::KeyA) + stick(Axis::LeftStickX);
        let ahead = axis(KeyCode::KeyW, KeyCode::KeyS) + stick(Axis::LeftStickY);
        let (s, c) = walk.yaw.sin_cos();
        let delta = DVec2::new(c * side + s * ahead, c * ahead - s * side).clamp_length_max(1.0);
        let before = walk.point;
        walk.move_by(&self.plane, delta * 2.2 * dt);
        walk.stride += walk.point.distance(before);
        if walk.stride >= 0.76 {
            walk.stride -= 0.76;
            if let Some(audio) = &self.audio { audio.event(crate::audio::Event::Footstep); }
        }
    }

    pub(crate) fn advance_preflight(&mut self) {
        let Some(stage) = self.preflight else { return };
        if stage == Stage::Airborne && !self.preflight_cockpit && self.cam_mode == 0 {
            let (eye, _, _) = airborne_camera(&self.plane, self.real - self.preflight_step_since);
            self.briefing_exit = Some((eye - self.plane.centre(), self.real));
        }
        if let Stage::Walkaround(step) = stage
            && !self.walkaround.is_some_and(|walk| walk.at_station(&self.plane, step))
        {
            return;
        }
        self.preflight = stage.next();
        self.preflight_step_since = self.real;
        self.keys.clear();
        if self.preflight.is_none() {
            self.walkaround = None;
            self.flight_ui_since = self.real;
            if matches!(stage, Stage::Walkaround(_)) {
                self.cam_mode = 1;
                self.recenter_view();
                self.status = t!("Parking brake set · Start the engine with F4 or use the cockpit controls.",
                    "Frein de parc serré · Démarrez avec F4 ou les commandes du cockpit.").into();
                self.status_until = self.real + 15.0;
            }
            log::info!("Pilot took control");
        }
    }

    pub(crate) fn preflight_ui(&mut self, stage: Stage) -> (Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32) {
        use egui::{Align2, RichText};
        let raw = self.egui_input();
        let mut advance = false;
        let mut back = false;
        let ctx = self.egui_ctx.clone();
        let out = ctx.run_ui(raw, |root| {
            let ctx = root.ctx();
            let width = (ctx.content_rect().width() - 64.0).clamp(150.0, 326.0);
            if let Stage::Airborne = stage {
                egui::Area::new(egui::Id::unique("start_airborne"))
                    .anchor(Align2::RIGHT_BOTTOM, [-screen_margin(width), -24.0])
                    .show(ctx, |ui| {
                        ui.set_width(198.0);
                        advance = menu::wide_button(ui, t!("Start flight", "Commencer le vol"), Hint::None, true, true).clicked();
                    });
                if self.google_shown() { self.credits(ctx); }
                return;
            }
            let Stage::Walkaround(step) = stage else { return };
            let Some(walk) = self.walkaround else { return };
            if ctx.input(|i| i.pointer.secondary_down()) {
                let delta = ctx.input(|i| i.pointer.delta());
                if let Some(walk) = &mut self.walkaround {
                    walk.yaw += f64::from(delta.x) * 0.003;
                    walk.pitch = (walk.pitch - f64::from(delta.y) * 0.003).clamp(-1.38, 1.38);
                }
            }
            let range = walk.point.distance(Walkaround::station(&self.plane, step));
            let nearby = walk.at_station(&self.plane, step);
            let to = Walkaround::station(&self.plane, step) - walk.point;
            if range > 0.15 {
                let delta = to.x.atan2(to.y) - walk.yaw;
                let rect = ctx.content_rect();
                let center = rect.center();
                let offset = egui::vec2(delta.sin() as f32, -(delta.cos() as f32)) * 24.0;
                let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("walk_cue")));
                p.arrow(center + offset * 0.5, offset * 0.8, egui::Stroke::new(2.0, menu::ACCENT));
            }
            egui::Area::new(egui::Id::unique("preflight_back"))
                .anchor(Align2::LEFT_TOP, [screen_margin(width), 18.0])
                .show(ctx, |ui| {
                    back = ui.button(t!("Return to planner", "Retour au planificateur")).clicked();
                });
            egui::Area::new(egui::Id::unique("departure_briefing"))
                .anchor(Align2::RIGHT_BOTTOM, [-screen_margin(width), -20.0])
                .show(ctx, |ui| {
                    flightui::tools_frame().inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                        ui.set_width(width);
                        let (title, action) = match stage {
                            Stage::Airborne => unreachable!("airborne briefing uses a single button"),
                            Stage::Walkaround(0) => (t!("Exterior check", "Inspection extérieure"), t!("Remove protective cover", "Retirer la protection")),
                            Stage::Walkaround(1) => (t!("Exterior check", "Inspection extérieure"), t!("Remove wheel chocks", "Retirer les cales")),
                            Stage::Walkaround(2) => (t!("Exterior check", "Inspection extérieure"), t!("Check flight surfaces", "Vérifier les gouvernes")),
                            Stage::Walkaround(_) => (t!("Ready to board", "Prêt à embarquer"), t!("Enter cockpit", "Entrer dans le cockpit")),
                        };
                        ui.label(RichText::new(title).font(menu::medium(22.0)).color(menu::TEXT));
                        ui.label(RichText::new(t!("{:.1} m to checkpoint", "Point de contrôle à {:.1} m", range)).font(menu::label()).color(menu::TEXT));
                        if !nearby {
                            ui.label(RichText::new(t!("WASD to walk · Right-drag or arrows to look", "WASD pour marcher · Clic droit glissé ou flèches pour regarder")).font(menu::caption()).color(menu::DIM));
                        }
                        ui.add_space(8.0);
                        advance = menu::wide_button(ui, action, Hint::of("Enter", "A"), true, nearby).clicked();
                    });
                });
            if self.google_shown() { self.credits(ctx); }
        });
        self.egui_state.handle_platform_output(&self.window, out.platform_output);
        if back {
            self.to_globe();
        } else if advance {
            self.advance_preflight();
        }
        let opacity = Loading::ease((self.real - self.flight_ui_since) / 0.28) as f32;
        let mut jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        flightui::fade_meshes(&mut jobs, opacity);
        (jobs, out.textures_delta, out.pixels_per_point)
    }
}

fn screen_margin(width: f32) -> f32 { if width < 240.0 { 12.0 } else { 22.0 } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parked_pilot_has_to_reach_each_station() {
        let p = Plane::spawn_ground(&crate::aircraft::c172::SPEC, 0.6, -1.3, 12.0, 0.0, 0.0);
        let mut walk = Walkaround::new(&p);
        assert!(!walk.at_station(&p, 1));
        for step in 0..=CHECKS {
            walk.point = Walkaround::station(&p, step);
            assert!(walk.at_station(&p, step));
            let stood = walk.point;
            walk.move_by(&p, -stood);
            assert_eq!(walk.point, stood);
        }
    }

    #[test]
    fn airborne_showcase_changes_angle_without_losing_the_aircraft() {
        let plane = Plane::spawn(&crate::aircraft::c172::SPEC, 0.6, -1.3, 720.0, 0.0, 42.0);
        let first = airborne_camera(&plane, 0.0).0;
        assert!(first.distance(airborne_camera(&plane, 8.0).0) > plane.spec.cam_dist);
        for n in 0..200 {
            let (eye, dir, up) = airborne_camera(&plane, n as f64 * 0.1);
            assert!(eye.distance(plane.centre()) >= plane.spec.cam_dist);
            assert!(dir.dot((plane.centre() - eye).normalize()) > 0.9999);
            assert!(up.is_finite());
        }
    }

    #[test]
    fn walkaround_requires_all_checks_before_boarding() {
        let mut stage = Some(Stage::Walkaround(0));
        for step in 1..=CHECKS {
            stage = stage.and_then(Stage::next);
            assert_eq!(stage, Some(Stage::Walkaround(step)));
        }
        assert_eq!(stage.and_then(Stage::next), None);
        assert_eq!(Stage::Airborne.next(), None);
    }
}

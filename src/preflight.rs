//! Interactive handoff from the departure camera to the aircraft.
use crate::{Game, Loading, Plane, cars, flightui, menu, padui::Hint, render::Inst, t};
use glam::{DQuat, DVec3, Mat4, Vec3};

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

pub fn walkaround_camera(plane: &Plane, step: u8, blend: f64) -> (DVec3, DVec3, DVec3) {
    let up = plane.pos.normalize();
    let scale = f64::from(plane.spec.span).max(8.0);
    let shot = |step| match step {
        0 => (0.78, 0.48, 0.22, 0.22),
        1 => (0.92, -0.18, 0.06, 0.04),
        2 => (0.85, -0.54, 0.34, -0.18),
        _ => (0.94, 0.12, 0.17, 0.08),
    };
    let (a, b, c, d) = shot(step.saturating_sub(1));
    let (x, y, z, aim) = shot(step);
    let offset = plane.right() * (scale * (a + (x - a) * blend))
        + plane.forward() * (scale * (b + (y - b) * blend))
        + up * (scale * (c + (z - c) * blend) + 1.3);
    let target = plane.centre() + plane.forward() * ((d + (aim - d) * blend) * scale);
    let eye = plane.centre() + offset;
    (eye, (target - eye).normalize(), up)
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

    pub(crate) fn advance_preflight(&mut self) {
        let Some(stage) = self.preflight else { return };
        if stage == Stage::Airborne && !self.preflight_cockpit && self.cam_mode == 0 {
            let (eye, _, _) = airborne_camera(&self.plane, self.real - self.preflight_step_since);
            self.briefing_exit = Some((eye - self.plane.centre(), self.real));
        }
        self.preflight = stage.next();
        self.preflight_step_since = self.real;
        self.keys.clear();
        if self.preflight.is_none() {
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
                        ui.label(RichText::new(self.spec.name).font(menu::label()).color(menu::TEXT));
                        if !self.status.is_empty() {
                            ui.label(RichText::new(&self.status).font(menu::caption()).color(menu::DIM));
                        }
                        if let Stage::Walkaround(step) = stage {
                            ui.label(RichText::new(t!("Exterior checks {}/{}", "Vérifications extérieures {}/{}", step.min(CHECKS), CHECKS)).font(menu::data()).color(menu::DIM));
                        }
                        ui.add_space(8.0);
                        advance = menu::wide_button(ui, action, Hint::of("Enter", "A"), true, true).clicked();
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

//! Optional visual aid for the nearby airfield's active runway.
use crate::{airports, geo, traffic};
use egui::{Color32, Stroke, Vec2};
use glam::DVec3;

pub struct Guide {
    pub name: String,
    threshold: DVec3,
    direction: DVec3,
    up: DVec3,
    aim: f64,
    width: f64,
}

pub struct LandingPractice {
    pub target: Guide,
    length: f64,
    half_width: f64,
}

impl LandingPractice {
    pub fn new(field: &airports::Geo, runway: usize, end: usize) -> Self {
        let r = &field.runways[runway];
        Self {
            target: Guide::new(field, runway, end),
            length: f64::from(r.len - r.thr[end]),
            half_width: f64::from(r.width) * 0.5,
        }
    }

    pub fn report(&self, position: DVec3, velocity: DVec3, vertical_speed: f64) -> String {
        let offset = position - self.target.threshold;
        let along = offset.dot(self.target.direction);
        let across = offset.dot(self.target.direction.cross(self.target.up)).abs();
        let on_runway = (0.0..=self.length).contains(&along) && across <= self.half_width;
        let ground_speed = (velocity - self.target.up * velocity.dot(self.target.up)).length();
        let placement = if on_runway {
            crate::t!("{:.0} m beyond threshold · {:.0} m off centerline",
                "{:.0} m après le seuil · {:.0} m de l’axe", along, across)
        } else {
            crate::t!("Outside the selected runway", "Hors de la piste sélectionnée").to_owned()
        };
        crate::t!("{} · {:.0} kt · {:.0} ft/min descent · {}",
            "{} · {:.0} kt · {:.0} pi/min en descente · {}",
            self.target.name, ground_speed * 1.94384, (-vertical_speed * 196.85).max(0.0), placement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guides_leave_space_around_the_plane_and_hide_when_pointed_away() {
        let guide = Guide {
            name: "TEST 09".into(), threshold: DVec3::X * (geo::R + 100.0),
            direction: DVec3::Y, up: DVec3::X, aim: 300.0, width: 35.0,
        };
        let plane = guide.center(850.0);
        let camera = plane - DVec3::Y * 20.0;
        let lines = guide.world_lines(camera, plane, DVec3::Y);
        assert_eq!(lines.len(), 16);
        assert!(lines.iter().all(|p| p[1] > 95.0));
        assert!(guide.world_lines(camera, plane, -DVec3::Y).is_empty());
    }

    #[test]
    fn arrival_gates_extend_before_threshold_and_follow_three_degree_slope() {
        let guide = Guide {
            name: "TEST 09".into(), threshold: DVec3::X * (geo::R + 100.0),
            direction: DVec3::Y, up: DVec3::X, aim: 300.0, width: 35.0,
        };
        for distance in [250.0, 5250.0, 10250.0] {
            let gate = guide.center(distance);
            assert!((gate - guide.threshold).dot(guide.direction) < 0.0);
            let height = gate.length() - guide.threshold.length();
            assert!((height - (distance + guide.aim) * 3.0_f64.to_radians().tan()).abs() < 1e-6);
        }
    }
}

impl Guide {
    pub fn new(g: &airports::Geo, runway: usize, end: usize) -> Self {
        let r = &g.runways[runway];
        let (threshold, direction) = r.end(end);
        Self {
            name: format!("{} · {}", g.apt().ident, r.ident[end]),
            threshold: g.world(threshold, g.h(threshold) as f64),
            direction: (g.e * direction.x as f64 + g.n * direction.y as f64).normalize(),
            up: g.u,
            aim: traffic::aim_dist(r.len) as f64,
            width: (r.width as f64 * 0.75).max(35.0),
        }
    }

    fn center(&self, distance: f64) -> DVec3 {
        let ground = (self.threshold - self.direction * distance).normalize() * self.threshold.length();
        ground + ground.normalize() * ((distance + self.aim) * 3.0_f64.to_radians().tan())
    }

    pub const MAX_VERTICES: usize = 21 * 8;

    pub fn world_lines(&self, camera: DVec3, aircraft: DVec3, forward: DVec3) -> Vec<[f32; 3]> {
        let to_threshold = self.threshold - aircraft;
        let along = to_threshold.dot(self.direction);
        let right = self.direction.cross(self.up).normalize();
        if !(0.0..=18_000.0).contains(&along) || forward.dot(self.direction) < 0.65
            || to_threshold.dot(right).abs() > along * 0.25 + 220.0
        {
            return Vec::new();
        }
        let mut lines = Vec::with_capacity(Self::MAX_VERTICES);
        for i in 0..21 {
            let distance = 250.0 + i as f64 * 500.0;
            // Leave room around the airplane, including in an external camera.
            if distance + 95.0 > along {
                break;
            }
            let center = self.center(distance);
            let half_width = self.width + distance * 0.012;
            let half_height = 12.0 + distance * 0.004;
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, y)| (center - camera + right * x * half_width + self.up * y * half_height).as_vec3().to_array());
            for edge in 0..4 {
                lines.extend([corners[edge], corners[(edge + 1) % 4]]);
            }
        }
        lines
    }

    pub fn draw_map(&self, painter: &egui::Painter, rect: egui::Rect, cx: f64, cy: f64, n: f64, angle: f64) {
        let project = |point: DVec3| {
            let (lat, lon, _) = geo::pos_to_lla(point);
            let x = geo::lon_to_merc_x(lon) * n;
            let y = geo::lat_to_merc_y(lat) * n;
            let delta = Vec2::new(((x - cx + n * 0.5).rem_euclid(n) - n * 0.5) as f32, (y - cy) as f32) * 256.0;
            let (s, c) = (angle as f32).sin_cos();
            rect.center() + Vec2::new(c * delta.x - s * delta.y, s * delta.x + c * delta.y)
        };
        let end = project(self.threshold);
        let start = project(self.center(10_250.0));
        let delta = end - start;
        if delta.length() < 12.0 { return; }
        let direction = delta.normalized();
        let side = Vec2::new(-direction.y, direction.x);
        let color = Color32::from_rgb(255, 145, 50);
        painter.add(egui::Shape::convex_polygon(vec![end - side * 3.0, end + side * 3.0, start + side * 12.0, start - side * 12.0], color.gamma_multiply(0.15), Stroke::new(1.0, color)));
        for fraction in [0.25, 0.5, 0.75] {
            let tip = start + delta * fraction;
            painter.line_segment([tip - direction * 7.0 + side * 4.0, tip], Stroke::new(2.0, color));
            painter.line_segment([tip - direction * 7.0 - side * 4.0, tip], Stroke::new(2.0, color));
        }
        let label = start + Vec2::new(0.0, -16.0);
        if rect.shrink(35.0).contains(label) {
            painter.text(label, egui::Align2::CENTER_BOTTOM, &self.name, egui::FontId::proportional(12.0), color);
        }
    }
}

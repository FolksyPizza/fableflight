//! Runway automation for the simulator's Cessna 172RG.
use crate::{aircraft, airports, autopilot::State, geo, plane::{Controls, Plane}};
use glam::DVec3;

const GLIDE: f64 = 0.052_408;
const KT: f64 = 1.943_84;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Takeoff,
    Climb,
    Final,
    Flare,
    Rollout,
}

#[derive(Clone, Copy, Debug)]
pub struct Runway {
    threshold: DVec3,
    forward: DVec3,
    right: DVec3,
    course: f64,
    elevation: f64,
    length: f64,
    half_width: f64,
}

impl Runway {
    pub fn from_airfield(field: &airports::Geo, index: usize, end: usize) -> Option<Self> {
        let r = &field.runways[index];
        let (threshold, direction) = r.end(end);
        let displaced = f64::from(r.thr[end]);
        let length = f64::from(r.len) - displaced;
        if !r.paved || length < 1_100.0 || r.width < 20.0 {
            return None;
        }
        let elevation = f64::from(field.h(threshold));
        Some(Self::new(field.world(threshold, elevation), f64::from(airports::bearing(direction)), elevation, length, f64::from(r.width)))
    }

    fn new(threshold: DVec3, course: f64, elevation: f64, length: f64, width: f64) -> Self {
        let (e, n, u) = geo::enu(threshold);
        let (s, c) = course.to_radians().sin_cos();
        let forward = e * s + n * c;
        Self { threshold, forward, right: forward.cross(u), course, elevation, length, half_width: width * 0.5 }
    }

    pub fn local(self, position: DVec3) -> (f64, f64) {
        let offset = position - self.threshold;
        (offset.dot(self.forward), offset.dot(self.right))
    }

    fn height(self, plane: &Plane) -> f64 {
        plane.pos.length() - geo::R - self.elevation - plane.stand_height()
    }

    fn acceptable_wind(self, wind: DVec3) -> bool {
        wind.dot(self.right).abs() * KT <= 8.0 && wind.dot(self.forward) * KT <= 4.0
    }
}

fn angle(a: f64) -> f64 { (a + 180.0).rem_euclid(360.0) - 180.0 }

#[derive(Debug)]
pub struct RunwayPilot {
    pub phase: Phase,
    runway: Runway,
    elapsed: f64,
    speed_trim: f64,
}

pub enum Output {
    Control(Controls),
    Climbing,
    Stopped,
    Cancelled,
}

impl RunwayPilot {
    pub fn supported(plane: &Plane) -> bool {
        std::ptr::eq(plane.spec, &aircraft::c172::SPEC) && plane.running && !plane.wrecked()
    }

    pub fn takeoff(runway: Runway, plane: &Plane) -> Option<Self> {
        let s = State::of(plane);
        let (along, across) = runway.local(plane.pos);
        (Self::supported(plane) && plane.on_ground && !plane.hold
            && plane.gear >= 0.99 && s.gs < 2.0
            && (0.0..=runway.length * 0.15).contains(&along)
            && across.abs() < runway.half_width * 0.30
            && angle(s.hdg - runway.course).abs() < 8.0 && runway.acceptable_wind(plane.wind))
            .then_some(Self { phase: Phase::Takeoff, runway, elapsed: 0.0, speed_trim: 1.0 })
    }

    pub fn land(runway: Runway, plane: &Plane) -> Option<Self> {
        let s = State::of(plane);
        let (along, across) = runway.local(plane.pos);
        let h = runway.height(plane);
        let glide_height = (180.0 - along) * GLIDE;
        let speed = plane.spec.approach.0;
        (Self::supported(plane) && !plane.on_ground && !plane.hold && !plane.stalled
            && (-6_500.0..=-750.0).contains(&along)
            && (h - glide_height).abs() < (glide_height * 0.35).clamp(22.0, 70.0)
            && across.abs() < (-along * 0.065).max(35.0)
            && angle(s.trk - runway.course).abs() < 18.0 && s.bank.abs() < 15f64.to_radians()
            && (speed * 0.91..=speed * 1.22).contains(&s.ias)
            && plane.gear >= 0.99 && plane.gear_down && plane.flap_set == plane.spec.approach.1
            && runway.acceptable_wind(plane.wind))
            .then_some(Self { phase: Phase::Final, runway, elapsed: 0.0, speed_trim: plane.throttle })
    }

    pub fn step(&mut self, plane: &Plane, pilot: &Controls, dt: f64) -> Output {
        if !Self::supported(plane) || plane.stalled || manual_input(pilot) {
            return Output::Cancelled;
        }
        let r = self.runway;
        let s = State::of(plane);
        let (along, across) = r.local(plane.pos);
        let h = r.height(plane);
        let ground_yaw = (angle(r.course - s.hdg).to_radians() * 3.4
            - across * 0.065 - plane.vel.dot(r.right) * 0.13 + plane.omega.y * 0.40)
            .clamp(-0.65, 0.65);
        let steering = |lookahead: f64, limit: f64| {
            let track = r.course - across.atan2(lookahead).to_degrees();
            let bank = (angle(track - s.trk) * 0.48).clamp(-limit, limit).to_radians();
            ((bank - s.bank) * 3.0 - s.roll_rate * 0.95).clamp(-0.5, 0.5)
        };
        let pitch = |want: f64| ((want - s.pitch) * 4.3 - s.pitch_rate * 1.30).clamp(-0.65, 0.8);
        self.elapsed += dt;
        match self.phase {
            Phase::Takeoff | Phase::Climb => {
                if plane.on_ground && (across.abs() >= r.half_width * 0.72 || along >= r.length * 0.78 || self.elapsed > 38.0) {
                    self.phase = Phase::Rollout;
                    return Output::Control(Controls { throttle_set: Some(0.0), brake: true, yaw: ground_yaw, ..Default::default() });
                }
                if !plane.on_ground && h > 4.0 { self.phase = Phase::Climb; }
                if self.phase == Phase::Climb && h >= 105.0 && s.vs > 0.0 { return Output::Climbing; }
                if self.elapsed > 80.0 { return Output::Cancelled; }
                let rotate = s.ias * KT >= 61.0 || self.phase == Phase::Climb;
                let attitude = if self.phase == Phase::Climb {
                    (9.0 + (s.ias * KT - 72.0) * 0.24).clamp(4.0, 12.0)
                } else { 9.5 };
                Output::Control(Controls {
                    throttle_set: Some(1.0),
                    pitch: if rotate { pitch(attitude.to_radians()) } else { 0.0 },
                    roll: if plane.on_ground { 0.0 } else { steering(280.0, 10.0) },
                    yaw: if plane.on_ground { ground_yaw } else { 0.0 },
                    ..Default::default()
                })
            }
            Phase::Final | Phase::Flare => {
                if plane.on_ground {
                    if along < 0.0 || along > r.length || across.abs() > r.half_width {
                        return Output::Cancelled;
                    }
                    self.phase = Phase::Rollout;
                    return Output::Control(Controls { throttle_set: Some(0.0), yaw: ground_yaw, ..Default::default() });
                }
                if along > r.length * 0.62 || self.elapsed > 260.0 || (h < 30.0 && across.abs() > r.half_width * 0.8) {
                    return Output::Cancelled;
                }
                if h < 7.0 { self.phase = Phase::Flare; }
                let flare = self.phase == Phase::Flare;
                let target = if flare {
                    -(0.42 + h.max(0.0) * 0.19)
                } else {
                    let ideal = (180.0 - along) * GLIDE;
                    -s.gs * GLIDE + ((ideal - h) * 0.24).clamp(-1.65, 1.45)
                };
                let want = (s.pitch + (target - s.vs) * if flare { 0.059 } else { 0.031 })
                    .clamp(-10f64.to_radians(), 11f64.to_radians());
                let speed_error = plane.spec.approach.0 - s.ias;
                let throttle = if flare { 0.0 } else {
                    self.speed_trim = (self.speed_trim + speed_error * 0.023 * dt).clamp(0.0, 0.85);
                    (self.speed_trim + speed_error * 0.07).clamp(0.0, 0.95)
                };
                Output::Control(Controls {
                    throttle_set: Some(throttle),
                    pitch: pitch(want),
                    roll: steering(((-along) * 0.15).clamp(75.0, 460.0), if flare { 6.0 } else { 13.0 }),
                    yaw: if flare { (ground_yaw * 0.36).clamp(-0.2, 0.2) } else { 0.0 },
                    ..Default::default()
                })
            }
            Phase::Rollout => {
                if s.gs < 1.2 && plane.on_ground { return Output::Stopped; }
                if across.abs() > r.half_width * 1.05 || along > r.length + 15.0 { return Output::Cancelled; }
                Output::Control(Controls {
                    throttle_set: Some(0.0),
                    pitch: if plane.on_ground { 0.0 } else { pitch(4f64.to_radians()) },
                    yaw: ground_yaw,
                    brake_left: 0.65,
                    brake_right: 0.65,
                    ..Default::default()
                })
            }
        }
    }
}

pub fn manual_input(c: &Controls) -> bool {
    c.pitch.abs().max(c.roll.abs()).max(c.yaw.abs()) > 0.43 || c.brake
        || c.brake_left.max(c.brake_right) > 0.12 || c.throttle != 0.0 || c.boost || c.reverse
}

/// Ramp digital keys before mixing in analog commands. Full deflection is
/// still available, while a brief keyboard tap gives a small correction.
pub fn smooth_keyboard(previous: [f64; 3], target: [f64; 3], dt: f64) -> [f64; 3] {
    std::array::from_fn(|i| {
        let delta = target[i] - previous[i];
        let rate = if target[i] == 0.0 || target[i] * previous[i] < 0.0 { 8.0 } else { 4.5 };
        let limit = dt.clamp(0.0, 0.05) * rate;
        previous[i] + delta.clamp(-limit, limit)
    })
}

pub fn stabilize(plane: &Plane, c: &mut Controls) {
    if plane.on_ground || plane.hold || plane.stalled || plane.spec.rocket().is_some() { return; }
    let s = State::of(plane);
    if s.bank.abs() > 55f64.to_radians() || s.pitch.abs() > 28f64.to_radians() { return; }
    let blend = (1.0 - c.roll.abs() / 0.3).clamp(0.0, 1.0);
    c.roll = (c.roll - blend * (s.bank * 0.42 + s.roll_rate * 0.20).clamp(-0.24, 0.24)).clamp(-1.0, 1.0);
    if c.pitch.abs() < 0.35 {
        let near_stall = (plane.alpha - plane.stall_alpha() + 4f64.to_radians()).max(0.0);
        c.pitch = (c.pitch - (near_stall * 2.5).min(0.22)).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn test_world() -> crate::world::World {
        let fetch = crate::fetch::Fetcher::new(std::env::temp_dir(), true);
        let sources = Arc::new(crate::sources::Sources::new(fetch, String::new()));
        let mut world = crate::world::World::new(crate::sources::Loader::new(sources, 0), 14);
        world.flat = Some(0.0);
        world
    }

    #[test]
    fn keyboard_taps_are_small_but_a_held_key_reaches_full_input() {
        let tap = smooth_keyboard([0.0; 3], [1.0, 0.0, -1.0], 1.0 / 60.0);
        assert!((0.02..0.10).contains(&tap[0]) && tap[2] == -tap[0]);
        assert_eq!(smooth_keyboard(tap, [0.0; 3], 1.0 / 60.0), [0.0; 3]);
        let mut input = tap;
        for _ in 0..20 { input = smooth_keyboard(input, [1.0, 0.0, -1.0], 1.0 / 60.0); }
        assert_eq!(input, [1.0, 0.0, -1.0]);
    }

    #[test]
    fn automatic_takeoff_climbs_and_automatic_landing_stops_on_runway() {
        let world = test_world();
        let r = Runway::new(geo::lla_to_pos(0.6, -0.5, 0.0), 0.0, 0.0, 2_100.0, 45.0);
        let (lat, lon, _) = geo::pos_to_lla(r.threshold + r.forward * 25.0);
        let mut plane = Plane::spawn_ground(&aircraft::c172::SPEC, lat, lon, 0.0, 0.0, 0.0);
        plane.hold = false;
        plane.t = 2.0;
        plane.flap_set = 1;
        let mut pilot = RunwayPilot::takeoff(r, &plane).expect("lined up to depart");
        let fly = |pilot: &mut RunwayPilot, plane: &mut Plane, seconds| {
            let dt = 1.0 / 180.0;
            for _ in 0..seconds * 180 {
                match pilot.step(plane, &Controls::default(), dt) {
                    Output::Control(c) => {
                        assert!(plane.step(dt, &c, &world).is_none(), "{:?}", plane.notes);
                    },
                    other => return other,
                }
            }
            panic!("did not finish {:?} at {:?}, height {:.1}", pilot.phase, r.local(plane.pos), r.height(plane));
        };
        assert!(matches!(fly(&mut pilot, &mut plane, 70), Output::Climbing));
        let (lat, lon, _) = geo::pos_to_lla(r.threshold - r.forward * 2_400.0);
        let mut plane = Plane::spawn(&aircraft::c172::SPEC, lat, lon, 2_580.0 * GLIDE + 1.0, 0.0, aircraft::c172::SPEC.approach.0);
        plane.configure(plane.spec.approach.1, true, plane.spec.approach.0 * GLIDE);
        let mut pilot = RunwayPilot::land(r, &plane).expect("stabilized final");
        let pilot_override = Controls { throttle: -1.0, ..Default::default() };
        assert!(matches!(pilot.step(&plane, &pilot_override, 1.0 / 180.0), Output::Cancelled));
        assert!(matches!(fly(&mut pilot, &mut plane, 135), Output::Stopped),
            "phase {:?}, position {:?}, height {:.1}, state {:?}", pilot.phase, r.local(plane.pos), r.height(&plane), State::of(&plane));
        let (along, across) = r.local(plane.pos);
        assert!((0.0..r.length).contains(&along) && across.abs() < r.half_width);
    }
}

impl crate::Game {
    fn runway_status(&mut self, message: &str) {
        self.status = message.into();
        self.status_until = self.real + 8.0;
    }

    fn begin_runway_pilot(&mut self, pilot: RunwayPilot) {
        self.ap.disengage();
        self.flight_plan.follow = false;
        self.flight_plan.auto_ils = None;
        self.flight_plan.message = None;
        self.menu_hold = None;
        self.menu_hold_pending = false;
        self.plane.parking_brake = false;
        self.runway_pilot = Some(pilot);
    }

    pub(crate) fn select_automatic_landing(&mut self, airport: u32, ident: &str) -> bool {
        let want = airports::norm_ident(ident);
        let controller = self.world.airfield(airport).and_then(|field| {
            let field = &field.geo;
            field.runways.iter().enumerate().find_map(|(i, r)| {
                let end = r.ident.iter().position(|id| airports::norm_ident(id) == want)?;
                let runway = Runway::from_airfield(field, i, end)?;
                let (lat, lon, _) = geo::pos_to_lla(runway.threshold + runway.forward * 180.0);
                if self.world.ground(lat, lon).is_none_or(|g| g.water || (g.h - runway.elevation).abs() > 7.0) {
                    return None;
                }
                RunwayPilot::land(runway, &self.plane)
            })
        });
        let Some(pilot) = controller else { return false };
        self.begin_runway_pilot(pilot);
        self.runway_status(crate::t!("Automatic landing engaged", "Atterrissage automatique engagé"));
        true
    }

    pub(crate) fn try_automatic_landing(&mut self, input: &Controls) -> bool {
        let Some((airport, runway)) = &self.flight_plan.auto_ils else { return false };
        let apt = &airports::Db::get().apts[*airport as usize];
        let matching_ils = self.radios.nav.as_ref().is_some_and(|station| {
            station.name == apt.ident && airports::norm_ident(&station.ident) == airports::norm_ident(runway)
        });
        let altitude = self.plane.pos.length() - geo::R - f64::from(apt.elev);
        if self.ap.lat != crate::autopilot::Lat::Apr || !matching_ils || manual_input(input)
            || !RunwayPilot::supported(&self.plane) || altitude < 25.0
        {
            self.flight_plan.auto_ils = None;
            self.flight_plan.message = Some(crate::t!("Automatic landing disarmed", "Atterrissage automatique désarmé").into());
            return false;
        }
        if self.ap.nav_cap && self.ap.gs_cap && altitude < 430.0 {
            if self.plane.gear_down && self.plane.flap_set == self.plane.spec.approach.1 {
                self.ap.spd = self.plane.spec.approach.0;
            }
            let (airport, runway) = (*airport, runway.clone());
            return self.select_automatic_landing(airport, &runway);
        }
        false
    }

    pub(crate) fn runway_controls(&mut self, input: &Controls, dt: f64) -> Controls {
        let Some(pilot) = &mut self.runway_pilot else { return *input };
        match pilot.step(&self.plane, input, dt) {
            Output::Control(output) => output,
            Output::Cancelled => {
                self.runway_pilot = None;
                self.runway_status(crate::t!("Runway assist disengaged · Pilot flying", "Assistance piste coupée · Pilotage manuel"));
                *input
            }
            Output::Stopped => {
                self.runway_pilot = None;
                self.plane.parking_brake = true;
                self.runway_status(crate::t!("Stopped on runway · Take over to taxi clear", "Arrêt sur piste · Reprenez les commandes pour dégager"));
                Controls { brake: true, throttle_set: Some(0.0), ..Default::default() }
            }
            Output::Climbing => {
                let course = pilot.runway.course;
                let elevation = pilot.runway.elevation;
                self.runway_pilot = None;
                let state = State::of(&self.plane);
                use crate::autopilot::{Lat, Vert};
                self.ap.set_lat(Lat::Hdg, &state);
                self.ap.hdg = course;
                self.ap.set_vert(Vert::Vs, &state);
                self.ap.alt = elevation + 457.2;
                self.ap.vs = 2.3;
                self.ap.at = true;
                self.ap.spd = 80.0 / KT;
                self.ap.engage(&state);
                self.runway_status(crate::t!("Climbing to 1,500 ft above the airport", "Montée à 1 500 ft au-dessus de l’aéroport"));
                Controls { throttle_set: Some(1.0), ..Default::default() }
            }
        }
    }

    pub(crate) fn pilot_assist_ui(&mut self, ui: &mut egui::Ui) {
        use crate::{menu, t};
        use egui::RichText;
        let title = t!("Pilot assistance", "Assistance au pilotage");
        ui.label(RichText::new(title).font(menu::medium(24.0)).color(menu::TEXT));
        ui.checkbox(&mut self.stability_assist, t!("Stability assistance", "Aide à la stabilité"))
            .on_hover_text(t!("Gently levels the wings and limits nose-up input near a stall.", "Remet doucement les ailes à plat et limite le cabré près du décrochage."));
        if let Some(pilot) = &self.runway_pilot {
            let label = match pilot.phase {
                Phase::Takeoff => t!("Takeoff roll", "Roulement de décollage"),
                Phase::Climb => t!("Initial climb", "Montée initiale"),
                Phase::Final => t!("Tracking final", "Suivi de la finale"),
                Phase::Flare => t!("Flare", "Arrondi"),
                Phase::Rollout => t!("Braking on runway", "Freinage sur la piste"),
            };
            ui.label(RichText::new(label).font(menu::data()).color(menu::ACCENT));
            if menu::wide_button(ui, t!("Take control", "Reprendre le contrôle"), crate::padui::Hint::of("F5", ""), true, true).clicked() {
                self.ap_disconnect(None);
            }
        } else if self.plane.on_ground {
            let pilot = self.world.airfield_near(self.plane.pos, 6_000.0).and_then(|field| {
                let field = &field.geo;
                let p = field.local(self.plane.pos);
                let i = field.on_runway(p, 0.0)?;
                (0..2).find_map(|end| Runway::from_airfield(field, i, end).and_then(|r| RunwayPilot::takeoff(r, &self.plane)))
            });
            if menu::wide_button(ui, t!("Automatic takeoff", "Décollage automatique"), crate::padui::Hint::None, true, pilot.is_some()).clicked()
                && let Some(pilot) = pilot
            {
                self.plane.flap_set = 1;
                self.begin_runway_pilot(pilot);
            }
            ui.label(RichText::new(t!("Cessna 172RG · Engine running and lined up on a paved runway", "Cessna 172RG · Moteur en marche et aligné sur une piste revêtue"))
                .font(menu::caption()).color(menu::DIM));
        }
    }
}

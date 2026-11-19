//! Local flight checkpoints, independent of the planner's last departure.
use crate::{Game, aircraft, airports, geo, t};
use glam::{DQuat, DVec3};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SLOTS: usize = 4;

#[derive(Clone, Deserialize, Serialize)]
pub struct FlightSave {
    version: u8,
    aircraft: String,
    position: [f64; 3],
    velocity: [f64; 3],
    attitude: [f64; 4],
    angular_rate: [f64; 3],
    fuel: f64,
    payload: f64,
    trim: f64,
    throttle: f64,
    mixture: f64,
    flaps: u8,
    gear_down: bool,
    running: bool,
    on_ground: bool,
    utc: f64,
    day: f64,
    route: Vec<String>,
    arrival_runway: String,
    cruise_ft: u32,
}

impl FlightSave {
    fn path(slot: usize) -> Option<PathBuf> {
        let dir = dirs::config_dir()?.join("fablegame");
        match slot {
            0 => Some(dir.join("flight-save.json")),
            1..SLOTS => Some(dir.join(format!("flight-save-{}.json", slot + 1))),
            _ => None,
        }
    }

    pub fn load_all() -> [Option<Self>; SLOTS] {
        std::array::from_fn(|slot| {
            let data = std::fs::read(Self::path(slot)?).ok()?;
            let save: Self = serde_json::from_slice(&data).ok()?;
            save.valid().then_some(save)
        })
    }

    fn valid(&self) -> bool {
        let position = DVec3::from_array(self.position);
        let rotation = DQuat::from_array(self.attitude);
        self.version == 1
            && position.is_finite()
            && (geo::R - 550.0..=geo::R + 110_000.0).contains(&position.length())
            && DVec3::from_array(self.velocity).length() < 1_200.0
            && DVec3::from_array(self.angular_rate).length() < 18.0
            && (0.98..=1.02).contains(&rotation.length_squared())
            && (0.0..=1.0).contains(&self.throttle)
            && (0.0..=1.0).contains(&self.mixture)
            && (-0.3..=0.5).contains(&self.trim)
            && self.fuel.is_finite() && self.fuel >= 0.0
            && self.payload.is_finite() && self.payload >= 0.0
            && (0.0..24.0).contains(&self.utc)
            && (0.0..365.0).contains(&self.day)
            && self.route.len() <= 12
            && self.cruise_ft <= 55_000
    }

    fn store(&self, slot: usize) -> anyhow::Result<()> {
        let path = Self::path(slot).ok_or_else(|| anyhow::anyhow!("no configuration directory"))?;
        let dir = path.parent().ok_or_else(|| anyhow::anyhow!("no checkpoint directory"))?;
        std::fs::create_dir_all(dir)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn label(&self) -> String {
        let (lat, lon, alt) = geo::pos_to_lla(DVec3::from_array(self.position));
        let area = self.route.last().map_or_else(|| crate::menu::latlon(lat.to_degrees(), lon.to_degrees()), |dest| dest.clone());
        if self.on_ground {
            format!("{} · {}", self.aircraft_name(), area)
        } else {
            format!("{} · {} · {:.0} ft", self.aircraft_name(), area, alt * 3.28084)
        }
    }

    fn aircraft_name(&self) -> &str {
        aircraft::by_key(&self.aircraft).map_or(self.aircraft.as_str(), |spec| spec.name)
    }
}

impl Game {
    pub(crate) fn can_save_flight(&self) -> bool {
        self.mode == crate::Mode::Fly && !self.departing() && self.preflight.is_none()
            && self.flight_ended.is_none() && !self.crashed && !self.plane.wrecked()
            && self.spec.rocket().is_none() && !self.plane.stalled && !self.plane.on_hull
            && (!self.plane.on_ground || self.plane.vel.length() < 1.2)
    }

    pub(crate) fn save_flight(&mut self, slot: usize) {
        if !self.can_save_flight() || slot >= SLOTS { return; }
        let p = &self.plane;
        let save = FlightSave {
            version: 1, aircraft: self.spec.keys[0].to_owned(), position: p.pos.to_array(),
            velocity: p.vel.to_array(), attitude: p.rot.to_array(), angular_rate: p.omega.to_array(),
            fuel: p.fuel, payload: p.payload, trim: p.trim, throttle: p.throttle, mixture: p.mixture,
            flaps: p.flap_set, gear_down: p.gear_down, running: p.running, on_ground: p.on_ground,
            utc: self.utc, day: self.doy, route: self.flight_plan.airports.iter()
                .map(|&id| airports::Db::get().apts[id as usize].ident.clone()).collect(),
            arrival_runway: self.flight_plan.runway.clone(), cruise_ft: self.flight_plan.cruise_ft,
        };
        if !save.valid() {
            self.save_message = Some(t!("This flight state cannot be saved", "Cet état de vol ne peut pas être enregistré").into());
            return;
        }
        match save.store(slot) {
            Ok(()) => {
                self.saved_flights[slot] = Some(save);
                self.save_message = Some(t!("Flight saved", "Vol enregistré").into());
            }
            Err(err) => {
                log::warn!("flight save: {err:#}");
                self.save_message = Some(t!("Could not save the flight", "Impossible d’enregistrer le vol").into());
            }
        }
    }

    pub(crate) fn resume_saved_flight(&mut self, slot: usize) {
        let Some(save) = self.saved_flights.get(slot).and_then(Clone::clone) else { return };
        let Some(spec) = aircraft::by_key(&save.aircraft).filter(|spec| spec.rocket().is_none()) else {
            self.status = t!("Install the saved flight’s aircraft to resume", "Installez l’avion du vol enregistré pour reprendre").into();
            self.save_message = Some(self.status.clone());
            return;
        };
        self.save_message = None;
        self.set_aircraft(spec);
        let position = DVec3::from_array(save.position);
        let (lat, lon, alt) = geo::pos_to_lla(position);
        let rotation = DQuat::from_array(save.attitude).normalize();
        let velocity = DVec3::from_array(save.velocity);
        let (east, north, _) = geo::enu(position);
        let f = rotation * DVec3::NEG_Z;
        let hdg = f.dot(east).atan2(f.dot(north));
        if save.on_ground {
            let cold = self.menu.cold;
            self.menu.cold = !save.running;
            let sag = -spec.gear.iter().map(|leg| leg.at.y).fold(0.0f64, f64::min);
            self.respawn_ground(lat, lon, alt - sag, hdg, 0.0);
            self.menu.cold = cold;
        } else {
            self.respawn_at(lat, lon, alt, hdg, velocity.length().max(spec.approach.0));
            self.plane.configure(save.flaps, save.gear_down, 0.0);
        }
        let p = &mut self.plane;
        if !save.running { p.cold_and_dark(); }
        p.pos = position;
        p.rot = rotation;
        p.vel = if save.on_ground { DVec3::ZERO } else { velocity };
        p.omega = if save.on_ground { DVec3::ZERO } else { DVec3::from_array(save.angular_rate) };
        p.fuel = save.fuel.clamp(0.0, spec.fuel_kg);
        p.payload = save.payload.clamp(0.0, spec.payload[1].max(0.0));
        p.trim = save.trim;
        p.throttle = if save.running && !save.on_ground { save.throttle } else { 0.0 };
        p.mixture = save.mixture;
        p.flap_set = save.flaps.min(spec.max_notch());
        p.parking_brake = save.on_ground;
        self.utc = save.utc;
        self.doy = save.day;
        self.manual_time_lon = None;
        self.wx.real_time = false;
        self.flight_plan.airports = save.route.iter().filter_map(|name| airports::Db::get().find(name)).collect();
        self.flight_plan.runway = save.arrival_runway;
        self.flight_plan.cruise_ft = save.cruise_ft;
        self.radios.nav = None;
        self.status = t!("Saved flight restored", "Vol enregistré restauré").into();
        self.status_until = self.real + 20.0;
        self.mode = crate::Mode::Fly;
        self.departure_terrain = 0.0;
        self.prepare_departure((!save.on_ground).then_some(crate::preflight::Stage::Airborne));
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_round_trip_and_validation() {
        let save = FlightSave {
            version: 1,
            aircraft: "c172".into(),
            position: geo::lla_to_pos(0.4, -0.3, 1600.0).to_array(),
            velocity: [0.0, 20.0, 60.0],
            attitude: DQuat::IDENTITY.to_array(),
            angular_rate: [0.0; 3],
            fuel: 42.0,
            payload: 130.0,
            trim: 0.02,
            throttle: 0.62,
            mixture: 0.9,
            flaps: 0,
            gear_down: false,
            running: true,
            on_ground: false,
            utc: 19.0,
            day: 120.0,
            route: vec!["KLGA".into()],
            arrival_runway: "22".into(),
            cruise_ft: 3500,
        };
        let mut restored: FlightSave = serde_json::from_slice(&serde_json::to_vec(&save).unwrap()).unwrap();
        assert!(restored.valid());
        assert_eq!(restored.position, save.position);
        assert_eq!(restored.route, ["KLGA"]);
        assert_eq!(restored.arrival_runway, "22");
        restored.attitude = [0.0; 4];
        assert!(!restored.valid());
    }
}

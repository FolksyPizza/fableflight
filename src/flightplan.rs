//! An in-flight airport route and a shortcut to couple a nearby ILS.
use crate::{Game, airports, autopilot::{Lat, State, Vert}, geo, menu, nav, t};
use glam::DVec3;
use std::collections::VecDeque;

#[derive(Default)]
pub struct FlightPlan {
    pub(crate) airports: VecDeque<u32>,
    entry: String,
    pub(crate) runway: String,
    pub(crate) cruise_ft: u32,
    pub follow: bool,
    pub(crate) auto_ils: Option<(u32, String)>,
    pub(crate) message: Option<String>,
}

fn airport_pos(index: u32) -> DVec3 {
    let apt = &airports::Db::get().apts[index as usize];
    geo::lla_to_pos(apt.lat.to_radians(), apt.lon.to_radians(), 0.0)
}

fn range(from: DVec3, to: DVec3) -> f64 {
    geo::R * from.angle_between(to)
}

fn heading_for_track(course: f64, state: &State) -> f64 {
    let correction = (state.hdg - state.trk + 180.0).rem_euclid(360.0) - 180.0;
    (course + correction.clamp(-25.0, 25.0)).rem_euclid(360.0)
}

impl Game {
    pub(crate) fn update_flight_plan(&mut self, state: &State) {
        if !self.flight_plan.follow || self.ap.lat != Lat::Hdg {
            return;
        }
        if let Some(&next) = self.flight_plan.airports.front()
            && self.flight_plan.airports.len() > 1
            && range(self.plane.pos, airport_pos(next)) < (state.gs * 20.0).clamp(1_600.0, 5_000.0)
        {
            self.flight_plan.airports.pop_front();
        }
        let Some(&next) = self.flight_plan.airports.front() else {
            self.flight_plan.follow = false;
            return;
        };
        let target = airport_pos(next);
        if range(self.plane.pos, target) < (state.gs * 18.0).clamp(1_200.0, 4_000.0) {
            self.flight_plan.follow = false;
            self.flight_plan.message = Some(t!("Destination reached. Heading and altitude hold.", "Destination atteinte. Maintien du cap et de l’altitude.").into());
        } else {
            self.ap.hdg = heading_for_track(nav::bearing(self.plane.pos, target), state);
        }
    }

    fn route_altitude(&self) -> f64 {
        let mut from = self.plane.pos.normalize();
        let mut highest: f64 = 0.0;
        for &i in &self.flight_plan.airports {
            let to = airport_pos(i).normalize();
            let turn = glam::DQuat::from_rotation_arc(from, to);
            let samples = ((geo::R * from.angle_between(to)) / 8_000.0).ceil().clamp(1.0, 180.0) as usize;
            for j in 0..=samples {
                let p = glam::DQuat::IDENTITY.slerp(turn, j as f64 / samples as f64) * from;
                let (lat, lon, _) = geo::pos_to_lla(p * geo::R);
                highest = highest.max(f64::from(crate::sources::fallback_height(lat, lon)));
                if let Some(ground) = self.world.ground(lat, lon) { highest = highest.max(ground.h); }
            }
            from = to;
        }
        let clearance = ((highest * 3.28084 + 2200.0) / 500.0).ceil() * 500.0;
        clearance.max(f64::from(self.flight_plan.cruise_ft)) / 3.28084
    }

    fn follow_flight_plan(&mut self) {
        let s = State::of(&self.plane);
        let floor = self.route_altitude();
        let target = if self.flight_plan.cruise_ft == 0 { floor.max(s.alt) } else { floor };
        self.runway_pilot = None;
        self.flight_plan.auto_ils = None;
        self.ap.set_lat(Lat::Hdg, &s);
        if (target - s.alt).abs() < 25.0 {
            self.ap.set_vert(Vert::Alt, &s);
        } else {
            self.ap.set_vert(Vert::Vs, &s);
            self.ap.vs = (target - s.alt).signum() * 2.55;
            self.ap.alt = target;
        }
        if !self.ap.on { self.ap_toggle(); }
        if self.ap.on && self.plane.running && self.throttle_axis.is_none() {
            self.ap.at = true;
            self.ap.spd = s.ias.max(self.plane.spec.approach.0 * 1.18);
        }
        self.flight_plan.follow = self.ap.on;
        self.flight_plan.message = Some(t!("Cruise target {:.0} ft MSL", "Altitude de croisière {:.0} ft AMSL", target * 3.28084));
    }

    fn arm_planned_ils(&mut self, destination: u32, auto_land: bool) {
        let apt = &airports::Db::get().apts[destination as usize];
        let runway = airports::norm_ident(self.flight_plan.runway.trim());
        let state = State::of(&self.plane);
        let (lat, lon, _) = geo::pos_to_lla(self.plane.pos);
        let station = nav::near(lat.to_degrees(), lon.to_degrees(), 38_000.0, nav::Kind::Ils)
            .into_iter().map(|(s, _)| s)
            .filter(|s| s.name == apt.ident && (runway.is_empty() || airports::norm_ident(&s.ident) == runway))
            .find(|s| {
                let angle = (s.course - state.trk + 180.0).rem_euclid(360.0) - 180.0;
                let reading = nav::nav_reading(s, s.course, self.plane.pos);
                angle.abs() < 70.0 && reading.live
            });
        let Some(station) = station else {
            self.flight_plan.message = Some(if auto_land {
                t!("Set gear and flaps and line up on a stabilized final, or approach a runway with a usable ILS.", "Sortez le train et les volets et stabilisez la finale, ou approchez une piste dotée d’un ILS utilisable.")
            } else {
                t!("No usable ILS ahead. Approach the runway from its arrival side.", "Aucun ILS utilisable devant. Rejoignez la piste du côté de l’approche.")
            }.into());
            return;
        };
        let runway = station.ident.clone();
        let course = station.course;
        self.runway_pilot = None;
        self.radios.tune_nav(station, self.plane.pos);
        self.flight_plan.follow = false;
        if !self.ap.on && self.ap.alt == 0.0 { self.ap.sync(&state); }
        self.ap.hdg = heading_for_track(course, &state);
        self.ap.set_vert(Vert::Alt, &state);
        self.ap.gs_cap = false;
        self.ap.set_lat(Lat::Apr, &state);
        if !self.ap.on { self.ap_toggle(); }
        self.flight_plan.auto_ils = (auto_land && self.ap.on).then(|| (destination, runway.clone()));
        if self.flight_plan.auto_ils.is_some() {
            self.ap.at = true;
            self.ap.spd = (self.plane.spec.approach.0 * 1.14).max(state.ias);
            self.flight_plan.message = Some(t!("ILS {} armed for automatic landing. Set landing gear and flaps before final.", "ILS {} armé pour l’atterrissage automatique. Sortez le train et les volets avant la finale.", runway));
        } else {
            self.flight_plan.message = Some(t!("ILS {} armed. Take control to land and taxi.", "ILS {} armé. Reprenez les commandes pour atterrir et rouler.", runway));
        }
    }

    pub(crate) fn flight_plan_ui(&mut self, ui: &mut egui::Ui) {
        use egui::RichText;
        ui.separator();
        menu::heading(ui, t!("Route and arrival", "Route et arrivée"));
        ui.horizontal(|ui| {
            let code = ui.add(egui::TextEdit::singleline(&mut self.flight_plan.entry)
                .desired_width(100.0).char_limit(8).hint_text(t!("Airport code", "Code aéroport")));
            let submit = code.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button(t!("Add airport", "Ajouter")).clicked() || submit {
                if let Some(i) = airports::Db::get().find(self.flight_plan.entry.trim()) {
                    if self.flight_plan.airports.len() < 12 && self.flight_plan.airports.back() != Some(&i) {
                        self.flight_plan.airports.push_back(i);
                        self.flight_plan.auto_ils = None;
                        self.flight_plan.runway.clear();
                        self.flight_plan.follow = false;
                    }
                    self.flight_plan.entry.clear();
                    self.flight_plan.message = None;
                } else {
                    self.flight_plan.message = Some(t!("Airport code not found", "Code aéroport inconnu").into());
                }
            }
        });
        let mut remove = None;
        for (index, &i) in self.flight_plan.airports.iter().enumerate() {
            let apt = &airports::Db::get().apts[i as usize];
            let nm = range(self.plane.pos, airport_pos(i)) / 1852.0;
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button(t!("Remove", "Retirer")).clicked() { remove = Some(index); }
                    ui.label(RichText::new(format!("{}  {:>5.1} NM", apt.code(), nm)).font(menu::data()));
                });
            });
        }
        if let Some(index) = remove {
            self.flight_plan.airports.remove(index);
            self.flight_plan.auto_ils = None;
            self.flight_plan.follow = false;
            self.flight_plan.message = None;
        }
        let can_fly = !self.plane.on_ground && !self.crashed && self.spec.rocket().is_none();
        if let Some(destination) = self.flight_plan.airports.back().copied().or_else(|| self.approach_airfield().map(|(field, _)| field.geo.idx)) {
            if !self.flight_plan.airports.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Cruise altitude", "Altitude de croisière"));
                    ui.add(egui::DragValue::new(&mut self.flight_plan.cruise_ft).range(0..=55_000).speed(100).suffix(" ft"));
                    ui.label(RichText::new(t!("0 for automatic", "0 pour la gestion automatique")).font(menu::caption()).color(menu::DIM));
                });
                if ui.add_enabled(can_fly, egui::Button::new(if self.flight_plan.follow { t!("Stop following", "Arrêter le suivi") } else { t!("Follow route", "Suivre la route") })).clicked() {
                    if self.flight_plan.follow {
                        self.flight_plan.follow = false;
                        self.flight_plan.message = None;
                    } else {
                        self.follow_flight_plan();
                    }
                }
            }
            let apt = &airports::Db::get().apts[destination as usize];
            ui.label(RichText::new(apt.code()).font(menu::data()).color(menu::TEXT));
            let selection = apt.rwys.iter().flat_map(|r| r.ident.iter())
                .find(|id| !id.is_empty() && airports::norm_ident(id) == airports::norm_ident(&self.flight_plan.runway))
                .cloned();
            egui::ComboBox::from_id_salt("arrival_runway")
                .selected_text(selection.as_deref().unwrap_or(t!("Select runway", "Choisir la piste")))
                .show_ui(ui, |ui| {
                    for r in &apt.rwys {
                        for id in r.ident.iter().filter(|e| !e.is_empty()) {
                            let label = format!("{id}  ·  {:.0} m  ·  {}", r.len, if r.paved { t!("Paved", "Revêtue") } else { t!("Unpaved", "Non revêtue") });
                            if ui.selectable_value(&mut self.flight_plan.runway, id.clone(), label).changed() {
                                self.flight_plan.auto_ils = None;
                                self.flight_plan.message = None;
                            }
                        }
                    }
                });
            let available = can_fly && selection.is_some();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(can_fly, egui::Button::new(t!("Arm ILS approach", "Armer l’approche ILS"))).clicked() {
                    self.arm_planned_ils(destination, false);
                }
                if ui.add_enabled(available && self.runway_pilot.is_none() && crate::pilot_assist::RunwayPilot::landing_supported(&self.plane),
                    egui::Button::new(if self.flight_plan.auto_ils.is_some() {
                        t!("Cancel automatic landing", "Annuler l’atterrissage automatique")
                    } else {
                        t!("Automatic landing", "Atterrissage automatique")
                    })).clicked()
                    && let Some(runway) = &selection
                {
                    if self.flight_plan.auto_ils.take().is_some() {
                        self.flight_plan.message = Some(t!("Automatic landing canceled. ILS remains armed.", "Atterrissage automatique annulé. L’ILS reste armé.").into());
                    } else if !self.select_automatic_landing(destination, runway) {
                        self.arm_planned_ils(destination, true);
                    }
                }
            });
            ui.label(RichText::new(t!("Cessna 172RG / Robin DR400 · Engage on final or arm from a usable ILS.", "Cessna 172RG / Robin DR400 · Engagez en finale ou armez depuis un ILS utilisable."))
                .font(menu::caption()).color(menu::DIM));
        }
        if let Some(message) = &self.flight_plan.message {
            ui.label(RichText::new(message).font(menu::caption()).color(menu::TEXT));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airport_route_track_compensates_crosswind_through_north() {
        let p = crate::Plane::spawn(crate::aircraft::BUILTIN[0], 0.0, 0.0, 1800.0, 0.0, 55.0);
        let mut state = State::of(&p);
        (state.hdg, state.trk) = (354.0, 5.0);
        assert!((heading_for_track(3.0, &state) - 352.0).abs() < 1e-9);
        let east = geo::lla_to_pos(0.0, 179.99f64.to_radians(), 0.0);
        let west = geo::lla_to_pos(0.0, -179.99f64.to_radians(), 0.0);
        assert!((2_000.0..2_400.0).contains(&range(east, west)));
    }
}

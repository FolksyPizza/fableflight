//! An in-flight airport route and a shortcut to couple a nearby ILS.
use crate::{Game, airports, autopilot::{Lat, State, Vert}, geo, menu, nav, t};
use glam::DVec3;
use std::collections::VecDeque;

#[derive(Default)]
pub struct FlightPlan {
    airports: VecDeque<u32>,
    entry: String,
    runway: String,
    pub follow: bool,
    message: Option<String>,
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

    fn arm_planned_ils(&mut self, destination: u32) {
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
            self.flight_plan.message = Some(t!("No usable ILS ahead. Approach the runway from its arrival side.", "Aucun ILS utilisable devant. Rejoignez la piste du côté de l’approche.").into());
            return;
        };
        let runway = station.ident.clone();
        let course = station.course;
        self.radios.tune_nav(station, self.plane.pos);
        self.flight_plan.follow = false;
        self.ap.hdg = heading_for_track(course, &state);
        self.ap.set_lat(Lat::Apr, &state);
        if !self.ap.on { self.ap_toggle(); }
        self.flight_plan.message = Some(t!("ILS {} armed. Take control to land and taxi.", "ILS {} armé. Reprenez les commandes pour atterrir et rouler.", runway));
    }

    pub(crate) fn flight_plan_ui(&mut self, ui: &mut egui::Ui) {
        use egui::RichText;
        ui.separator();
        menu::heading(ui, t!("Flight plan", "Plan de vol"));
        ui.horizontal(|ui| {
            let code = ui.add(egui::TextEdit::singleline(&mut self.flight_plan.entry)
                .desired_width(100.0).char_limit(8).hint_text(t!("Airport code", "Code aéroport")));
            let submit = code.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button(t!("Add airport", "Ajouter")).clicked() || submit {
                if let Some(i) = airports::Db::get().find(self.flight_plan.entry.trim()) {
                    if self.flight_plan.airports.len() < 12 && self.flight_plan.airports.back() != Some(&i) {
                        self.flight_plan.airports.push_back(i);
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
            if self.flight_plan.airports.is_empty() { self.flight_plan.follow = false; }
            self.flight_plan.message = None;
        }
        let can_fly = !self.plane.on_ground && !self.crashed && self.spec.rocket().is_none();
        if let Some(&destination) = self.flight_plan.airports.back() {
            if ui.add_enabled(can_fly, egui::Button::new(if self.flight_plan.follow { t!("Stop following", "Arrêter le suivi") } else { t!("Follow route", "Suivre la route") })).clicked() {
                if self.flight_plan.follow {
                    self.flight_plan.follow = false;
                } else {
                    let state = State::of(&self.plane);
                    self.ap.set_lat(Lat::Hdg, &state);
                    self.ap.set_vert(Vert::Alt, &state);
                    if !self.ap.on { self.ap_toggle(); }
                    self.flight_plan.follow = self.ap.on;
                }
                self.flight_plan.message = None;
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.flight_plan.runway)
                    .desired_width(88.0).char_limit(5).hint_text(t!("Runway (auto)", "Piste (auto)")));
                if ui.add_enabled(can_fly, egui::Button::new(t!("Arm ILS approach", "Armer l’approche ILS"))).clicked() {
                    self.arm_planned_ils(destination);
                }
            });
            ui.label(RichText::new(t!("Route holds current altitude. Check terrain clearance.", "La route maintient l’altitude. Vérifiez la marge terrain.")).font(menu::caption()).color(menu::DIM));
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

//! The interface over the flying itself, beside the instruments (`flight_hud`):
//!
//! - the flight toolbar, a row of icons that slides down when the pointer
//!   reaches the top edge (or while the flight panel is open) and opens the
//!   panel's pages, the minimap, the next camera and the pause menu;
//! - notices at the top centre: an airport's name, runway and wind for a few
//!   seconds when you arrive at it, and the game's status messages, fading;
//! - the guidance line above the instruments: on a runway, at a holding
//!   point, or lined up on an approach (live, so it stays while it applies);
//! - a systems card at the top left, only for the aircraft that have
//!   something to show there: a rocket's flight figures, a fighter's weapons.
use crate::menu::{self, Icon, Page, DIM, TEXT};
use crate::{t, Game};
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Sense, Stroke};

/// What the toolbar asked for this frame.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Bar {
    Page(Page),
    Map,
    Camera,
    Pause,
}

/// How long the toolbar stays after the pointer leaves the top edge (s), and
/// how near the edge counts (points).
const BAR_LINGER: f64 = 1.2;
const BAR_EDGE: f32 = 64.0;
/// How long an airport notice shows (s), the last of it fading.
const NOTE_SHOW: f64 = 7.0;
const NOTE_FADE: f64 = 1.0;

/// A glass panel for anything over the flying.
pub fn glass() -> egui::Frame {
    egui::Frame::new().fill(menu::GLASS).stroke(Stroke::new(1.0, menu::LINE)).corner_radius(menu::R_PANEL).inner_margin(egui::Margin::symmetric(14, 10))
}

/// Reserve the PFD and its right-hand readouts before placing the navigation card.
pub(crate) fn nav_hud_dock(available: f32, width: f32) -> Option<(f32, f32)> {
    let bottom = crate::HUD_BOTTOM + crate::HUD_H + 14.0;
    let right = available * 0.5 + crate::HUD_W * 0.5 + 18.0;
    if right + width + 14.0 <= available {
        Some((right, bottom))
    } else if width + 28.0 <= available {
        Some(((available - width) * 0.5, bottom))
    } else {
        None
    }
}

#[cfg(test)]
#[test]
fn nav_card_clears_primary_instruments_at_wide_and_narrow_sizes() {
    let width = 366.0;
    for available in [430.0, 820.0, 1_280.0] {
        let (left, bottom) = nav_hud_dock(available, width).unwrap();
        assert!(left >= 14.0 && left + width <= available - 14.0);
        assert!(bottom >= crate::HUD_BOTTOM + crate::HUD_H + 12.0);
    }
    assert!(nav_hud_dock(350.0, width).is_none());
}

pub(crate) fn fade_meshes(jobs: &mut [egui::ClippedPrimitive], opacity: f32) {
    for job in jobs {
        if let egui::epaint::Primitive::Mesh(mesh) = &mut job.primitive {
            for vertex in &mut mesh.vertices {
                vertex.color = vertex.color.gamma_multiply(opacity);
            }
        }
    }
}

pub(crate) fn draw_primary_warning(painter: &egui::Painter, center: egui::Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), menu::data(), Color32::WHITE);
    let bounds = Rect::from_center_size(center, galley.size() + vec2(22.0, 8.0));
    painter.rect_filled(bounds, 2.0, Color32::from_rgb(132, 27, 35));
    painter.galley(center - galley.size() * 0.5, galley, Color32::WHITE);
}

pub(crate) fn readable_status(message: &str) -> String {
    let text = if let Some(part) = message.strip_prefix("L ") {
        format!("Left {}", part.to_lowercase())
    } else if let Some(part) = message.strip_prefix("R ") {
        format!("Right {}", part.to_lowercase())
    } else if message.chars().any(char::is_lowercase) {
        return message.to_owned();
    } else {
        let mut letters = message.chars();
        letters.next().map_or_else(String::new, |first| format!("{first}{}", letters.as_str().to_lowercase()))
    };
    text.replace(" gone", " lost").replace(" dead", " failed")
}

pub(crate) fn tools_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(menu::PANEL)
        .stroke(Stroke::new(1.0, Color32::from_rgb(58, 69, 82)))
        .corner_radius(4)
}

impl Game {
    pub(crate) fn flight_toolbar_visible(&self) -> bool {
        self.flight_tools_open || self.real - self.toolbar_t < BAR_LINGER
    }

    /// Draws the toolbar when it is wanted; a press goes into `bar`.
    pub(crate) fn toolbar(&mut self, ctx: &egui::Context, bar: &mut Option<Bar>) {
        let screen = ctx.content_rect();
        let near_top = ctx.pointer_latest_pos().is_some_and(|p| p.y < BAR_EDGE && screen.contains(p));
        if !near_top {
            self.toolbar_hover_blocked = false;
        }
        if self.flight_tools_open || (near_top && !self.toolbar_hover_blocked) {
            self.toolbar_t = self.real;
        }
        let want = self.flight_toolbar_visible();
        let show = ctx.animate_bool_with_time_and_easing(egui::Id::unique("toolbar_show"), want, 0.2, egui::emath::easing::cubic_out);
        if show <= 0.0 {
            return;
        }
        if want && show < 1.0 {
            ctx.request_repaint();
        }
        let items: [(Icon, &str, Bar, bool); 8] = [
            (Icon::Flight, t!("Flight", "Vol"), Bar::Page(Page::Flight), self.info_open && self.info_page == Page::Flight),
            (Icon::GoTo, t!("Go to", "Aller à"), Bar::Page(Page::Places), self.info_open && self.info_page == Page::Places),
            (Icon::Radio, t!("Radio", "Radio"), Bar::Page(Page::Radio), self.info_open && self.info_page == Page::Radio),
            (Icon::Autopilot, t!("Autopilot", "Pilote auto"), Bar::Page(Page::Autopilot), self.info_open && self.info_page == Page::Autopilot),
            (Icon::Weather, t!("Weather", "Météo"), Bar::Page(Page::Weather), self.info_open && self.info_page == Page::Weather),
            (Icon::Map, t!("Map", "Carte"), Bar::Map, self.minimap.visible),
            (Icon::Camera, if self.cam_mode == 1 { t!("Cockpit", "Cockpit") } else { t!("Chase", "Extérieur") }, Bar::Camera, false),
            (Icon::Pause, t!("Pause", "Pause"), Bar::Pause, false),
        ];
        let compact = screen.width() < 710.0;
        let bw = if compact { ((screen.width() - 32.0) / items.len() as f32).clamp(30.0, 48.0) } else { 85.0 };
        let bh = 35.0;
        let w = bw * items.len() as f32 + 12.0;
        let y = 9.0 - (bh + 23.0) * (1.0 - show);
        egui::Area::new(egui::Id::unique("toolbar")).order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.center().x - w * 0.5, screen.top() + y)).constrain(false).show(ctx, |ui| {
            ui.multiply_opacity(show);
            tools_frame().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.horizontal(|ui| {
                    for (k, (icon, label, action, active)) in items.into_iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(vec2(bw, bh), Sense::click());
                        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, label));
                        if k == 0 && self.flight_tools_open && !self.info_open {
                            self.pad_home_focus(&response);
                        }
                        let lit = menu::ease(ctx, response.id, response.hovered() || response.has_focus());
                        let painter = ui.painter();
                        if active || lit > 0.0 {
                            let bg = if active { Color32::from_rgb(28, 48, 69) } else { Color32::from_white_alpha((15.0 * lit) as u8) };
                            painter.rect_filled(rect.shrink(1.0), 2.0, bg);
                        }
                        if active {
                            let y = rect.bottom() - 2.0;
                            painter.hline(rect.left() + 7.0..=rect.right() - 7.0, y, Stroke::new(2.0, Color32::from_rgb(133, 187, 241)));
                        }
                        if k == 5 {
                            painter.vline(rect.left(), rect.top() + 6.0..=rect.bottom() - 6.0, Stroke::new(1.0, menu::LINE_HI));
                        }
                        let col = menu::mix(TEXT, Color32::WHITE, lit);
                        let at = if compact { rect.center() } else { rect.left_center() + vec2(13.0, 0.0) };
                        menu::icon(painter, icon, at, 15.0, col);
                        if !compact {
                            painter.text(rect.left_center() + vec2(24.0, 0.0), Align2::LEFT_CENTER, label, menu::medium(12.0), col);
                        }
                        let response = response.on_hover_text(label);
                        if response.clicked() {
                            *bar = Some(action);
                        }
                    }
                });
                if !self.hud_on() && self.menu_hold.is_some() {
                    ui.label(RichText::new(t!("MENU HOLD", "MAINTIEN MENU")).font(menu::data()).color(menu::OK));
                }
            });
        });
    }

    /// Carries out what the toolbar asked for.
    pub(crate) fn toolbar_act(&mut self, b: Bar) {
        match b {
            Bar::Page(p) => {
                if !self.flight_tools_open {
                    self.set_flight_tools(true);
                }
                if self.info_open && self.info_page == p {
                    self.info_open = false;
                } else {
                    self.info_open = true;
                    self.info_page = p;
                }
                self.pad_home = true;
            }
            Bar::Map => self.minimap.visible = !self.minimap.visible,
            Bar::Camera => self.next_view(),
            Bar::Pause => self.set_pause_menu(true),
        }
    }

    /// The notices at the top centre, and the guidance line over the
    /// instruments. `airport`: `airport_hud`'s lines (name, runway and wind,
    /// then the live line, coloured 1 good, 2 caution, 3 warning).
    pub(crate) fn notices(&mut self, ctx: &egui::Context, airport: Option<&[(String, u8)]>) {
        // A new airport (or a new runway in use) brings its notice back.
        let view_width = self.flight_view_width(ctx);
        let shift = (view_width - ctx.content_rect().width()) * 0.5;
        let key = airport.map(|l| l.iter().take(2).map(|x| x.0.as_str()).collect::<Vec<_>>().join("|")).unwrap_or_default();
        if key != self.airport_note.0 {
            self.airport_note = (key.clone(), if key.is_empty() { -100.0 } else { self.real });
        }
        let fade = |since: f64, show: f64| -> f32 { (1.0 - ((self.real - since - (show - NOTE_FADE)) / NOTE_FADE).max(0.0)).clamp(0.0, 1.0) as f32 };
        let mut notes: Vec<(f32, Vec<(String, egui::FontId, Color32)>)> = Vec::new();
        let airport_notice = airport.filter(|_| !(self.info_open && self.info_page == Page::Flight));
        if let (Some(lines), a) = (airport_notice, fade(self.airport_note.1, NOTE_SHOW)) {
            if a > 0.0 && !lines.is_empty() {
                let mut v = vec![(lines[0].0.clone(), menu::item_title(), Color32::WHITE)];
                if let Some(l) = lines.get(1) {
                    v.push((l.0.clone(), menu::caption(), TEXT));
                }
                notes.push((a, v));
            }
        }
        // (a status that only repeats the airport the notice above already names is left out)
        let airport_code = airport.and_then(|l| l.first()).and_then(|l| l.0.split_whitespace().next()).unwrap_or("\u{0}");
        let repeats = !notes.is_empty() && self.status.starts_with(airport_code);
        if self.real < self.status_until && !self.status.is_empty() && !repeats {
            let a = ((self.status_until - self.real) / NOTE_FADE).clamp(0.0, 1.0) as f32;
            notes.push((a, vec![(self.status.chars().take(90).collect(), menu::label(), Color32::WHITE)]));
        }
        if !notes.is_empty() {
            // under the toolbar while it is down
            let top = if self.flight_toolbar_visible() { 65.0 } else { 14.0 };
            let y = ctx.animate_value_with_time(egui::Id::unique("notes_y"), top, 0.2);
            egui::Area::new(egui::Id::unique("notices")).anchor(Align2::CENTER_TOP, [shift, y]).interactable(false).show(ctx, |ui| {
                ui.set_max_width((view_width - 24.0).max(100.0));
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.vertical_centered(|ui| {
                    for (a, lines) in &notes {
                        // (a scope, so each notice takes its own room in the stack)
                        ui.scope(|ui| {
                            ui.multiply_opacity(*a);
                            glass().inner_margin(egui::Margin::symmetric(18, 10)).show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                for (text, font, col) in lines {
                                    ui.label(RichText::new(text).font(font.clone()).color(*col));
                                }
                            });
                        });
                    }
                });
            });
            ctx.request_repaint();
        }
        // The live line: on a runway, at a holding point, or on an approach.
        let guidance = airport.and_then(|l| l.get(2)).is_some();
        self.ap_strip(ctx, guidance, shift);
        if let Some((text, col)) = airport.and_then(|l| l.get(2)) {
            let col = match col {
                1 => menu::OK,
                2 => Color32::from_rgb(255, 196, 80),
                3 => menu::BAD,
                _ => TEXT,
            };
            egui::Area::new(egui::Id::unique("guidance")).anchor(Align2::CENTER_BOTTOM, [shift, -crate::HUD_BOTTOM - crate::HUD_H - 10.0]).interactable(false).show(ctx, |ui| {
                glass().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, col);
                        ui.label(RichText::new(text).font(menu::label()).color(TEXT));
                    });
                });
            });
        }
    }

    /// The autopilot's modes over the instruments, as an annunciator strip:
    /// green what it flies, grey what is armed; "AP OFF" flashing amber a
    /// few seconds after it lets go. Above the guidance line when that shows.
    fn ap_strip(&self, ctx: &egui::Context, guidance: bool, shift: f32) {
        let off_for = self.real - self.ap_off_t;
        let hold = self.menu_hold.as_ref();
        let flash = !self.ap.on && hold.is_none() && off_for < 4.0;
        if !self.ap.on && hold.is_none() && !flash {
            return;
        }
        let lift = if guidance { 44.0 } else { 0.0 };
        egui::Area::new(egui::Id::unique("ap_strip")).anchor(Align2::CENTER_BOTTOM, [shift, -crate::HUD_BOTTOM - crate::HUD_H - 10.0 - lift]).interactable(false).show(ctx, |ui| {
            glass().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.horizontal(|ui| {
                    let mono = egui::FontId::monospace(14.0);
                    if let Some(hold) = hold {
                        let label = if hold.at { t!("MENU HOLD  HDG ALT IAS", "MAINTIEN MENU  CAP ALT IAS") }
                            else { t!("MENU HOLD  HDG ALT", "MAINTIEN MENU  CAP ALT") };
                        ui.label(RichText::new(label).font(mono).color(menu::OK));
                    } else if self.ap.on {
                        let (on, armed) = self.ap.annunciation();
                        ui.label(RichText::new("AP").font(mono.clone()).strong().color(menu::OK));
                        for m in on {
                            ui.add_space(6.0);
                            ui.label(RichText::new(m).font(mono.clone()).color(menu::OK));
                        }
                        for m in armed {
                            ui.add_space(6.0);
                            ui.label(RichText::new(m).font(mono.clone()).color(DIM));
                        }
                    } else if (off_for * 2.5).fract() < 0.6 {
                        ui.label(RichText::new("AP OFF").font(mono).strong().color(Color32::from_rgb(255, 196, 80)));
                    } else {
                        ui.label(RichText::new("      ").font(mono));
                    }
                });
            });
        });
    }

    /// The systems card at the top left: a rocket's flight figures, or a
    /// fighter's weapons. Nothing (None) for every other aircraft.
    pub(crate) fn systems_card(&self, ctx: &egui::Context, vs: f64, base_up: f64) -> Option<Rect> {
        let p = &self.plane;
        let rocket = p.engines().is_some();
        if !rocket && !self.missiles_ok() {
            return None;
        }
        let amber = Color32::from_rgb(255, 196, 80);
        let row = |ui: &mut egui::Ui, k: &str, v: String, col: Color32| {
            ui.label(RichText::new(k).font(menu::caption()).color(DIM));
            ui.label(RichText::new(v).font(menu::data()).color(col));
            ui.end_row();
        };
        let r = egui::Area::new(egui::Id::unique("systems")).anchor(Align2::LEFT_TOP, [14.0, 14.0]).interactable(false).show(ctx, |ui| {
            glass().show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                if let Some((name, _, _)) = p.engines() {
                    ui.label(menu::section_text(&format!("{}  {}", p.stage_name().unwrap_or(""), name)).color(DIM));
                    egui::Grid::new("rocket").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
                        let apo = p.apogee().map_or(t!("escape", "libération").to_string(), |a| format!("{:.0} km", a.max(0.0) / 1000.0));
                        let high = if base_up < 3000.0 { format!("{:.0} m", base_up.max(0.0)) } else { format!("{:.1} km", base_up / 1000.0) };
                        row(ui, t!("Throttle", "Poussée"), format!("{:.0}%  ·  {:.0} MN", p.throttle * 100.0, p.thrust_now() / 1.0e6), TEXT);
                        row(ui, t!("Climb, speed", "Montée, vitesse"), format!("{:+.0} m/s  ·  {:.0} m/s", vs, p.vel.length()), TEXT);
                        row(ui, t!("Height", "Hauteur"), high, TEXT);
                        row(ui, t!("Apogee", "Apogée"), apo, TEXT);
                        row(ui, t!("Downrange", "Distance"), format!("{:.0} km", p.downrange() / 1000.0), TEXT);
                        if let Some(t) = p.burnout() {
                            row(ui, t!("Burnout", "Fin de combustion"), format!("{}:{:02}", t as u32 / 60, t as u32 % 60), TEXT);
                        }
                        row(ui, t!("Mass, load", "Masse, facteur"), format!("{:.0} t  ·  {:.1} g", p.mass() / 1000.0, p.g_load), TEXT);
                    });
                }
                if self.missiles_ok() {
                    let armed = self.weapons.armed;
                    let power = p.sys.powered();
                    ui.label(menu::section_text(t!("Weapons", "Armement")).color(DIM));
                    egui::Grid::new("weapons").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
                        let arm_col = if !armed { DIM } else if power { menu::OK } else { amber };
                        row(ui, t!("Master arm", "Armement général"), if armed { "ON".into() } else { "OFF".into() }, arm_col);
                        let (mut left, mut right) = (0u8, 0u8);
                        for (i, st) in self.weapons.stations.iter().enumerate() {
                            if self.weapons.loaded(i) {
                                if st.spec.side == 0 { left += 1 } else { right += 1 }
                            }
                        }
                        row(ui, "AIM-9", format!("L {left}  ·  R {right}"), TEXT);
                        let (line, col) = if !armed {
                            (t!("safe", "sécurisé").to_string(), DIM)
                        } else if !power {
                            (t!("no power", "pas d'alimentation").to_string(), amber)
                        } else if let Some(v) = &self.weapons.target_view {
                            if self.weapons.lock >= 1.0 {
                                (t!("lock  {:.1} km", "verrouillé  {:.1} km", v.range / 1000.0), menu::OK)
                            } else {
                                (t!("track  {:.1} km", "poursuite  {:.1} km", v.range / 1000.0), amber)
                            }
                        } else {
                            (t!("search", "recherche").to_string(), DIM)
                        };
                        row(ui, t!("Seeker", "Autodirecteur"), line, col);
                    });
                }
            });
        });
        Some(r.response.rect)
    }
}

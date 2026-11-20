//! Flight planning, aircraft selection and settings on the main screen.
use crate::menu::{self, Screen, ACCENT, DIM, TEXT};
use crate::padui;
use crate::{aircraft, hangar_ui, t, tr, Game};
use egui::{pos2, vec2, Align2, Color32, Painter, Rect, Sense, Stroke};

/// The picture behind the FLY tile, 4:3 at a little over twice the tile's
/// size: the game's own globe (sky.wgsl `globe_*`, live clouds) rendered
/// from 7600 km over the Caribbean (`--menu-view 24,-66,7600000 --time 8.35
/// --day 80`, exposure 0.3, a 3840x2160 frame with the menu hidden), cropped
/// so the planet rises off the right of the tile with the morning terminator
/// and the US's city lights along its left edge, the dark space to its left
/// left for the title. Swap the file to change it; nothing else depends on
/// its size beyond the aspect (it is cropped to fill the tile).
pub const GLOBE_PICTURE: &[u8] = include_bytes!("../assets/ui/globe.jpg");

/// What a tile does when pressed.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Tile {
    Aircraft,
    Fly,
    Settings,
    /// straight back into the air where the last flight started
    Again,
    SavedFlight,
}

impl Game {
    /// The main menu (see the module docs).
    pub(crate) fn main_menu(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let globe = self.picture(ctx, GLOBE_PICTURE);
        let spec = aircraft::nth(self.menu.aircraft);
        let mut press = None;
        let mut quit = false;
        let again = self.last_start_label();
        let saved: Vec<_> = self.saved_flights.iter().enumerate()
            .filter_map(|(slot, save)| Some((slot, save.as_ref()?.label()))).collect();
        egui::Area::new(egui::Id::unique("main_menu")).order(egui::Order::Background)
            .fixed_pos(screen.min).show(ctx, |ui| {
                let p = ui.painter().clone();
                let ink = Color32::from_rgb(11, 15, 18);
                p.rect_filled(screen, 0.0, ink);
                let scale = (screen.height() / 580.0).clamp(0.58, 1.16);
                let margin = (screen.width() * 0.062).clamp(20.0, 88.0);
                let rail = (screen.width() * 0.34).clamp(222.0, 386.0).min(screen.width() - margin * 2.0);
                let art = Rect::from_min_max(pos2(screen.left() + rail * 0.56, screen.top()), screen.max);
                paint_earth(&p, art, &globe);
                hgradient(&p, Rect::from_min_max(art.min, pos2(art.left() + rail * 0.82, art.bottom())), ink, Color32::TRANSPARENT);
                let x = screen.left() + margin;
                let first = screen.top() + 44.0 * scale;
                p.text(pos2(x, first), Align2::LEFT_TOP, "Fable Flight", menu::bold(48.0 * scale), TEXT);
                p.line_segment([pos2(x, first + 66.0 * scale), pos2(x + 38.0 * scale, first + 66.0 * scale)], Stroke::new(2.0, ACCENT));

                let mut y = first + 110.0 * scale;
                let mut row = |ui: &mut egui::Ui, id, title: &str, detail: Option<&str>, primary| {
                    let rect = Rect::from_min_size(pos2(x, y), vec2(rail, 52.0 * scale));
                    y += 56.0 * scale;
                    let response = ui.interact(rect, ui.scope_id().with(id), Sense::click());
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
                    dispatch_row(&p, rect, &response, title, detail, primary, scale);
                    response
                };
                let plan = row(ui, "plan", t!("Plan a flight", "Préparer un vol"), None, true);
                self.pad_home_focus(&plan);
                if plan.clicked() { press = Some(Tile::Fly); }
                if row(ui, "aircraft", t!("Aircraft", "Avions"), Some(spec.name), false).clicked() {
                    press = Some(Tile::Aircraft);
                }
                if let Some((_, label)) = saved.first()
                    && row(ui, "resume", t!("Resume saved flight", "Reprendre le vol"), Some(label), false).clicked()
                {
                    press = Some(Tile::SavedFlight);
                }
                if let Some((place, _)) = &again
                    && row(ui, "again", t!("Fly again", "Revoler"), Some(place), false).clicked()
                {
                    press = Some(Tile::Again);
                }
                if row(ui, "settings", t!("Settings", "Réglages"), None, false).clicked() { press = Some(Tile::Settings); }
                if let Some(message) = &self.save_message {
                    p.text(pos2(x, y + 5.0), Align2::LEFT_TOP, message, menu::caption(), DIM);
                }
                let foot = Rect::from_min_size(pos2(x, screen.bottom() - 42.0), vec2(90.0, 30.0));
                let button = ui.interact(foot, ui.scope_id().with("quit"), Sense::click());
                button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("Quit", "Quitter")));
                let lit = menu::ease(ctx, button.id, button.hovered() || button.has_focus());
                p.text(foot.left_center(), Align2::LEFT_CENTER, t!("Quit", "Quitter"), menu::label(), menu::mix(DIM, TEXT, lit));
                quit = button.clicked();
                if padui::active() && screen.width() > 580.0 {
                    let mut f = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(screen.center().x, foot.center().y), vec2(340.0, 24.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    padui::prompts(&mut f, &[("Dpad", tr("move")), ("A", tr("press"))], 18.0, 12.0, DIM);
                }
            });
        match press {
            Some(Tile::Fly) => self.menu.screen = Screen::Fly,
            Some(Tile::Aircraft) => self.open_aircraft(hangar_ui::Tab::Mine, Screen::Main),
            Some(Tile::Settings) => self.menu.screen = Screen::Settings,
            Some(Tile::SavedFlight) if saved.len() == 1 => self.resume_saved_flight(saved[0].0),
            Some(Tile::SavedFlight) => self.show_saved_flights = true,
            Some(Tile::Again) => {
                self.restore_last_start();
                self.menu.screen = Screen::Fly;
                self.begin();
            }
            None => {}
        }
        if self.show_saved_flights {
            let mut selected = None;
            egui::Window::new(t!("Saved flights", "Vols enregistrés"))
                .id(egui::Id::unique("flight_checkpoints"))
                .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut self.show_saved_flights)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    for (slot, label) in &saved {
                        if ui.button(t!("Slot {} · {}", "Emplacement {} · {}", slot + 1, label)).clicked() {
                            selected = Some(*slot);
                        }
                    }
                });
            if let Some(slot) = selected {
                self.show_saved_flights = false;
                self.resume_saved_flight(slot);
            }
        }
        if press.is_some() {
            self.pad_home = true;
        }
        if quit {
            self.quit = true;
        }
    }
}

fn dispatch_row(p: &Painter, rect: Rect, response: &egui::Response, title: &str, detail: Option<&str>, primary: bool, scale: f32) {
    let lit = menu::ease(p.ctx(), response.id, response.hovered() || response.has_focus());
    let fill = if primary { menu::mix(Color32::from_rgb(227, 235, 233), Color32::WHITE, lit) }
        else { Color32::from_white_alpha((14.0 * lit) as u8) };
    p.rect_filled(rect, 3.0, fill);
    if !primary {
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, Color32::from_white_alpha(26)));
    }
    let title_color = if primary { Color32::from_rgb(19, 29, 31) } else { TEXT };
    let x = rect.left() + 14.0 * scale;
    let y = rect.center().y - if detail.is_some() { 8.0 * scale } else { 0.0 };
    p.text(pos2(x, y), Align2::LEFT_CENTER, title, menu::medium(18.0 * scale), title_color);
    if let Some(detail) = detail {
        let mut job = egui::text::LayoutJob::simple_singleline(detail.into(), menu::medium(13.0 * scale), DIM);
        job.wrap.max_width = (rect.width() - 54.0 * scale).max(8.0);
        job.wrap.max_rows = 1;
        p.galley(pos2(x, y + 10.0 * scale), p.layout_job(job), DIM);
    }
    let a = rect.right_center() - vec2(18.0 * scale, 0.0);
    p.line(vec![a + vec2(-4.0, -4.0) * scale, a, a + vec2(-4.0, 4.0) * scale], Stroke::new(1.5, title_color));
}

fn paint_earth(p: &Painter, rect: Rect, earth: &egui::TextureHandle) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 { return; }
    let [w, h] = earth.size().map(|v| v as f32);
    let k = (rect.width() / w).max(rect.height() / h);
    let image_rect = Rect::from_center_size(rect.center(), vec2(w, h) * k);
    p.with_clip_rect(rect).image(earth.id(), image_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

fn hgradient(p: &Painter, r: Rect, left: Color32, right: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), left);
    mesh.colored_vertex(r.right_top(), right);
    mesh.colored_vertex(r.right_bottom(), right);
    mesh.colored_vertex(r.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}

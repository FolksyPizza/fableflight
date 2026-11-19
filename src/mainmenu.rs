//! Flight planning, aircraft selection and settings on the main screen.
use crate::menu::{self, Screen, ACCENT, DIM, FAINT, TEXT};
use crate::padui::{self, Hint};
use crate::{aircraft, hangar_ui, t, tr, Game};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke};

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
        let still = self.still(ctx, spec);
        let mut press: Option<Tile> = None;
        let mut quit = false;
        let again = self.last_start_label();
        let saved = self.saved_flight.as_ref().map(|flight| flight.aircraft_name().to_string());
        let resume = saved.as_deref().map(|plane| (t!("Saved checkpoint", "Point de reprise"), plane));
        egui::Area::new(egui::Id::unique("main_menu")).order(egui::Order::Background).fixed_pos(screen.min).show(ctx, |ui| {
            let p = ui.painter().clone();
            // Darken space towards the foot of the screen, for depth.
            gradient(&p, Rect::from_min_max(pos2(screen.left(), screen.center().y), screen.max), Color32::TRANSPARENT, Color32::from_rgba_premultiplied(4, 6, 10, 200));

            let (fw, fh, sw, gap, settings_h) = (620.0, 390.0, 310.0, 16.0, 58.0);
            let top = 95.0;
            let below = (u8::from(again.is_some()) + u8::from(resume.is_some())) as f32 * 58.0;
            let k = ((screen.width() - 40.0) / (fw + sw + gap)).min((screen.height() - top - 62.0) / (fh + below)).clamp(0.32, 1.15);
            let x = screen.center().x - (fw + sw + gap) * k * 0.5;
            let y = screen.top() + top + ((screen.height() - top - 62.0) - (fh + below) * k).max(0.0) * 0.5;
            let fly = Rect::from_min_size(pos2(x, y), vec2(fw, fh) * k);
            let air = Rect::from_min_size(pos2(fly.right() + gap * k, y), vec2(sw, fh - settings_h - gap) * k);
            let set = Rect::from_min_size(pos2(air.left(), air.bottom() + gap * k), vec2(sw, settings_h) * k);

            let r = ui.interact(fly, ui.scope_id().with("tile_fly"), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("Plan a flight", "Préparer un vol")));
            self.pad_home_focus(&r);
            fly_tile(&p, fly, &r, k, &globe);
            if r.clicked() {
                press = Some(Tile::Fly);
            }
            let r = ui.interact(air, ui.scope_id().with("tile_air"), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("Change aircraft ({})", "Changer d’avion ({})", spec.name)));
            aircraft_tile(&p, air, &r, k, spec.name, &still);
            if r.clicked() {
                press = Some(Tile::Aircraft);
            }
            let r = ui.interact(set, ui.scope_id().with("tile_set"), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("Settings", "Réglages")));
            settings_tile(&p, set, &r, k);
            if r.clicked() {
                press = Some(Tile::Settings);
            }

            let last = again.as_ref().map(|(place, plane)| (place.as_str(), *plane));
            let mut row_y = fly.bottom() + 12.0 * k;
            for (tile, details, title, id) in [
                (Tile::SavedFlight, resume, t!("Resume saved flight", "Reprendre le vol enregistré"), "resume"),
                (Tile::Again, last, t!("Fly again", "Revoler"), "again"),
            ] {
                let Some((place, plane)) = details else { continue };
                let b = Rect::from_min_size(pos2(fly.left(), row_y), vec2(fly.width(), 46.0 * k));
                row_y += 58.0 * k;
                let r = ui.interact(b, ui.scope_id().with(id), Sense::click());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
                again_button(&p, b, &r, title, place, plane);
                if r.clicked() {
                    press = Some(tile);
                }
            }
            if resume.is_some() && let Some(message) = &self.save_message {
                p.text(pos2(fly.left(), row_y), Align2::LEFT_TOP, message, menu::caption(), DIM);
            }

            // The foot: Quit at the left, the gamepad's buttons (or a word on the keys) in the middle.
            let foot_y = screen.bottom() - 34.0;
            let q = Rect::from_min_size(pos2(screen.left() + 24.0, foot_y - 17.0), vec2(104.0, 34.0));
            let qr = ui.interact(q, ui.scope_id().with("quit"), Sense::click());
            qr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("Quit", "Quitter")));
            let lit = menu::ease(ctx, qr.id, qr.hovered() || qr.has_focus());
            if lit > 0.0 {
                p.rect_filled(q, menu::R_WIDGET, Color32::from_white_alpha((14.0 * lit) as u8));
            }
            let qc = menu::mix(DIM, Color32::WHITE, lit);
            power_icon(&p, pos2(q.left() + 18.0, q.center().y), 14.0, qc);
            p.text(pos2(q.left() + 34.0, q.center().y), Align2::LEFT_CENTER, t!("Quit", "Quitter"), menu::medium(15.0), qc);
            if qr.clicked() {
                quit = true;
            }
            if padui::active() {
                let mut f = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(screen.center().x, foot_y), vec2(360.0, 28.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                padui::prompts(&mut f, &[("Dpad", tr("move")), ("A", tr("press"))], 20.0, 12.5, DIM);
            }
            p.text(pos2(screen.right() - 20.0, screen.bottom() - 10.0), Align2::RIGHT_BOTTOM,
                t!("Globe preview © Esri, Maxar, Earthstar · NASA Black Marble · Live Cloud Maps", "Aperçu du globe © Esri, Maxar, Earthstar · NASA Black Marble · Live Cloud Maps"),
                FontId::proportional(10.0), FAINT);
        });
        match press {
            Some(Tile::Fly) => self.menu.screen = Screen::Fly,
            Some(Tile::Aircraft) => self.open_aircraft(hangar_ui::Tab::Mine, Screen::Main),
            Some(Tile::Settings) => self.menu.screen = Screen::Settings,
            Some(Tile::SavedFlight) => self.resume_saved_flight(),
            Some(Tile::Again) => {
                self.restore_last_start();
                self.menu.screen = Screen::Fly;
                self.begin();
            }
            None => {}
        }
        if press.is_some() {
            self.pad_home = true;
        }
        if quit {
            self.quit = true;
        }
    }
}

/// "Fly again": a slim bar under FLY with the last start and aircraft.
fn again_button(p: &Painter, r: Rect, resp: &egui::Response, title: &str, place: &str, plane: &str) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    p.rect_filled(r, menu::R_WIDGET, menu::mix(Color32::from_rgba_premultiplied(12, 16, 22, 226), Color32::from_rgba_premultiplied(26, 32, 42, 238), lit));
    p.rect_stroke(r, menu::R_WIDGET, Stroke::new(1.0, menu::mix(Color32::from_white_alpha(26), ACCENT, lit)), egui::StrokeKind::Inside);
    // a circular arrow: round again
    let c = pos2(r.left() + 24.0, r.center().y);
    let col = menu::mix(TEXT, ACCENT, lit);
    let pts: Vec<Pos2> = (0..=18).map(|i| {
        let a = -0.4 + i as f32 / 18.0 * 4.9;
        c + vec2(a.cos(), a.sin()) * 7.0
    }).collect();
    let end = *pts.last().unwrap();
    p.line(pts, Stroke::new(1.8, col));
    p.add(egui::Shape::convex_polygon(vec![end + vec2(3.5, -3.5), end + vec2(4.0, 3.0), end + vec2(-2.5, 1.5)], col, Stroke::NONE));
    p.text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, title, menu::label(), menu::mix(TEXT, Color32::WHITE, lit));
    let label_width = p.layout_no_wrap(title.to_string(), menu::label(), TEXT).size().x;
    let right = Rect::from_min_max(pos2(r.left() + 56.0 + label_width, r.top()), r.right_bottom() - vec2(16.0, 0.0));
    if right.width() > 40.0 {
        let font = menu::caption();
        let mut job = egui::text::LayoutJob::simple_singleline(format!("{place}  ·  {plane}"), font, DIM);
        job.wrap.max_width = right.width();
        job.wrap.max_rows = 1;
        let g = p.layout_job(job);
        p.galley(pos2(right.right() - g.size().x, r.center().y - g.size().y * 0.5), g, DIM);
    }
}

/// The frame every tile shares: a dark fill, a hairline border, and when
/// hovered or picked out by the gamepad an accent border easing in.
fn tile_frame(p: &Painter, r: Rect, lit: f32, fill: bool) {
    if fill {
        p.rect_filled(r, menu::R_PANEL, menu::mix(Color32::from_rgba_premultiplied(12, 16, 22, 226), Color32::from_rgba_premultiplied(22, 28, 37, 236), lit));
    }
    p.rect_stroke(r, menu::R_PANEL, Stroke::new(1.0 + lit, menu::mix(Color32::from_white_alpha(26), ACCENT, lit)), egui::StrokeKind::Inside);
}

/// The caption of a side tile: its name in the display face, a dim line under it.
fn caption(p: &Painter, r: Rect, k: f32, lit: f32, title: &str, sub: &str) {
    let y = r.bottom() - 47.0 * k;
    p.text(pos2(r.left() + 22.0 * k, y), Align2::LEFT_BOTTOM, title, menu::bold(34.0 * k), menu::mix(TEXT, Color32::WHITE, lit));
    p.text(pos2(r.left() + 22.0 * k, y + 7.0 * k), Align2::LEFT_TOP, sub, menu::medium(17.0 * k), menu::mix(DIM, TEXT, lit));
}

/// Aircraft: the aircraft you will fly, large, easing forward when hovered.
fn aircraft_tile(p: &Painter, r: Rect, resp: &egui::Response, k: f32, name: &str, still: &egui::TextureHandle) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    tile_frame(p, r, lit, true);
    // a pool of light under the aeroplane
    let c = pos2(r.center().x, r.top() + r.height() * 0.38);
    let clip = p.with_clip_rect(r.shrink(1.0));
    menu::glow(&clip, c + vec2(0.0, 30.0 * k), vec2(r.width() * 0.62, r.width() * 0.36), Color32::from_white_alpha((14.0 + 8.0 * lit) as u8));
    // The picture is 680 x 392 with the aeroplane in its middle, easing forward on hover.
    let w = r.width() * 0.98;
    let pic = Rect::from_center_size(c + vec2(5.0 * lit, -4.0 * lit) * k, vec2(w, w * 392.0 / 680.0));
    clip.image(still.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    caption(&clip, r, k, lit, name, t!("Change aircraft", "Changer d’avion"));
}

fn settings_tile(p: &Painter, r: Rect, resp: &egui::Response, k: f32) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    tile_frame(p, r, lit, true);
    let c = pos2(r.left() + 28.0 * k, r.center().y);
    gear_icon_turned(p, c, 21.0 * k, menu::mix(DIM, Color32::WHITE, lit), 0.0);
    p.text(c + vec2(21.0 * k, 0.0), Align2::LEFT_CENTER, t!("Settings", "Réglages"), menu::medium(20.0 * k), TEXT);
}

/// FLY: the globe picture filling it (easing in a little when hovered),
/// shades along the foot and the left for the words, and the word.
fn fly_tile(p: &Painter, r: Rect, resp: &egui::Response, k: f32, globe: &egui::TextureHandle) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    let zoom = p.ctx().animate_bool_with_time(resp.id.with("zoom"), lit > 0.5, 0.9);
    p.rect_filled(r, menu::R_PANEL, Color32::BLACK);
    // Fill the tile, cropping the picture to its aspect.
    let [iw, ih] = globe.size().map(|v| v as f32);
    let (ta, ia) = (r.width() / r.height(), iw / ih);
    let uv = if ia > ta {
        let f = ta / ia;
        Rect::from_min_max(pos2(0.5 - f * 0.5, 0.0), pos2(0.5 + f * 0.5, 1.0))
    } else {
        let f = ia / ta;
        Rect::from_min_max(pos2(0.0, 0.5 - f * 0.5), pos2(1.0, 0.5 + f * 0.5))
    };
    // A slow push in toward the planet's lit face on hover (eased).
    let z = 1.0 - 0.035 * (zoom * zoom * (3.0 - 2.0 * zoom));
    let at = uv.lerp_inside(vec2(0.72, 0.42));
    let uv = Rect::from_min_max(at + (uv.min - at) * z, at + (uv.max - at) * z);
    p.image(globe.id(), r.shrink(1.0), uv, menu::mix(Color32::from_gray(222), Color32::WHITE, lit));
    // Shades for the words: across from the left, and up from the foot.
    let inner = r.shrink(1.0);
    hgradient(p, Rect::from_min_max(inner.min, pos2(inner.left() + inner.width() * 0.5, inner.bottom())), Color32::from_black_alpha(130), Color32::TRANSPARENT);
    gradient(p, Rect::from_min_max(pos2(inner.left(), inner.top() + inner.height() * 0.5), inner.max), Color32::TRANSPARENT, Color32::from_black_alpha(180));
    tile_frame(p, r, lit, false);
    let x = r.left() + 32.0 * k;
    let base = r.bottom() - 31.0 * k;
    p.text(pos2(x, base - 20.0 * k), Align2::LEFT_BOTTOM, t!("Plan a flight", "Préparer un vol"), menu::bold(67.0 * k), menu::mix(Color32::WHITE, ACCENT, lit));
    p.text(pos2(x, base), Align2::LEFT_CENTER, t!("Choose a departure on the globe", "Choisissez un départ sur le globe"), menu::medium(18.0 * k), Color32::from_rgb(220, 226, 234));
    let hc = Color32::from_white_alpha(190);
    match Hint::of("Enter", "A") {
        Hint::Text(h) => {
            let g = p.layout_no_wrap(h.to_string(), FontId::monospace(11.5), hc);
            let cap = Rect::from_min_size(pos2(r.right() - 16.0 * k - g.size().x - 14.0, r.top() + 16.0 * k), vec2(g.size().x + 14.0, 22.0));
            p.rect_filled(cap, menu::R_WIDGET, Color32::from_white_alpha(18));
            p.rect_stroke(cap, menu::R_WIDGET, Stroke::new(1.0, Color32::from_white_alpha(46)), egui::StrokeKind::Inside);
            p.galley(cap.center() - g.size() * 0.5, g, hc);
        }
        Hint::Pad(spec) => {
            let h = 22.0;
            let w = padui::spec_width(p, spec, h);
            padui::paint_spec(p, r.right() - 16.0 * k - w, r.top() + 16.0 * k + h * 0.5, spec, h, hc);
        }
        Hint::None => {}
    }
}

/// A horizontal gradient over a rectangle, `left` to `right`.
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

/// A vertical gradient over a rectangle.
fn gradient(p: &Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), top);
    mesh.colored_vertex(r.right_top(), top);
    mesh.colored_vertex(r.right_bottom(), bottom);
    mesh.colored_vertex(r.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}

/// A cog, turned `turn` radians: eight teeth round a ring, a hole in the middle.
fn gear_icon_turned(p: &Painter, c: Pos2, s: f32, col: Color32, turn: f32) {
    let (r_out, r_in, teeth) = (s * 0.46, s * 0.36, 8);
    let mut pts = Vec::new();
    for i in 0..teeth {
        let a0 = turn + i as f32 / teeth as f32 * std::f32::consts::TAU;
        let step = std::f32::consts::TAU / teeth as f32;
        // root, rise, top across, fall
        for (f, r) in [(0.0, r_in), (0.18, r_in), (0.28, r_out), (0.62, r_out), (0.72, r_in)] {
            let a = a0 + f * step;
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    let st = Stroke::new((s * 0.03).clamp(1.6, 3.0), col);
    p.add(egui::Shape::closed_line(pts, st));
    p.circle_stroke(c, s * 0.15, st);
}

/// The power symbol: a ring open at the top, a bar through the gap.
fn power_icon(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let r = s * 0.45;
    let st = Stroke::new(1.6, col);
    let pts: Vec<Pos2> = (0..=20).map(|i| {
        let a = -std::f32::consts::FRAC_PI_2 + 0.65 + i as f32 / 20.0 * (std::f32::consts::TAU - 1.3);
        c + vec2(a.cos(), a.sin()) * r
    }).collect();
    p.line(pts, st);
    p.line_segment([c - vec2(0.0, r * 1.15), c - vec2(0.0, r * 0.2)], st);
}

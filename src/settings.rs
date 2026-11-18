//! Settings kept between runs, in `<config dir>/fablegame/settings.json`
//! (`~/.config/fablegame` on Linux): the language, sound, camera, HUD and
//! anti-aliasing. Command-line flags still win over them for one run.

use crate::i18n::Lang;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollisionMode {
    #[default]
    Recover,
    EndFlight,
    Detailed,
}

impl CollisionMode {
    pub const ALL: [Self; 3] = [Self::Recover, Self::EndFlight, Self::Detailed];

    pub fn key(self) -> &'static str {
        match self {
            Self::Recover => "recover",
            Self::EndFlight => "end-flight",
            Self::Detailed => "detailed",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }

    pub fn description(self) -> &'static str {
        use crate::i18n::t;
        match self {
            Self::Recover => t!("Recover nearby after a severe impact.", "Reprenez à proximité après un impact sévère."),
            Self::EndFlight => t!("Stop the flight at impact and show a restart screen.", "Interrompt le vol à l’impact et propose de reprendre."),
            Self::Detailed => t!("Simulate damage and the full crash sequence.", "Simule les dégâts et la séquence complète de l’accident."),
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            Self::Recover => t!("Recover", "Récupération"),
            Self::EndFlight => t!("End flight", "Fin du vol"),
            Self::Detailed => t!("Detailed damage", "Dégâts détaillés"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// None: follow the system's language
    pub lang: Option<Lang>,
    pub sound_on: bool,
    pub volume: f32,
    /// 0 chase, 1 cockpit
    pub cam_mode: u8,
    pub cockpit_fov: f32,
    pub look_sens: f64,
    pub show_hud: bool,
    /// the HUD in the cockpit view too (off: hidden there, the cockpit's own instruments in view)
    pub hud_cockpit: bool,
    pub taa: bool,
    pub minimap: bool,
    pub landing_guidance: bool,
    pub runway_assist: bool,
    pub keyboard_rudder: f64,
    pub keyboard_rudder_fine: f64,
    pub remember_start: bool,
    /// arcade mode: the game's fun features (missiles, boost, respawn at will, weather keys)
    pub arcade: bool,
    pub collision_mode: CollisionMode,
    pub minimap_plan: bool,
    pub minimap_heading_up: bool,
    pub minimap_zoom: u8,
    pub minimap_size: f32,
    pub minimap_height: f32,
    /// real air traffic (see live.rs)
    pub live_traffic: bool,
    /// real weather (see livewx.rs)
    pub live_weather: bool,
    /// the clock follows the real time and date
    pub real_time: bool,
    /// where and how the last flight started (the globe opens on it, and
    /// the main menu offers to fly it again)
    pub last: Option<LastStart>,
}

/// The start of the last flight.
#[derive(Clone, Debug, PartialEq)]
pub struct LastStart {
    pub lat: f64,
    pub lon: f64,
    /// what the start point was called on the globe
    pub name: String,
    /// the airport's ident, when it was one
    pub airport: Option<String>,
    /// "runway", "approach" or "overhead"
    pub start: String,
    /// the runway end asked for (None: the one into the wind)
    pub runway: Option<String>,
    /// the aircraft's key (`aircraft::Spec::keys[0]`)
    pub aircraft: String,
}

impl LastStart {
    fn from_json(j: &serde_json::Value) -> Option<Self> {
        let lat = j["lat"].as_f64().filter(|x| (-90.0..=90.0).contains(x))?;
        let lon = j["lon"].as_f64().filter(|x| (-180.0..=180.0).contains(x))?;
        let s = |k: &str| j[k].as_str().map(str::to_string);
        Some(Self { lat, lon, name: s("name").unwrap_or_default(), airport: s("airport"), start: s("start").unwrap_or_else(|| "runway".into()), runway: s("runway"), aircraft: s("aircraft").unwrap_or_default() })
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "lat": self.lat, "lon": self.lon, "name": self.name, "airport": self.airport, "start": self.start, "runway": self.runway, "aircraft": self.aircraft })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self { lang: None, sound_on: true, volume: 1.0, cam_mode: 0, cockpit_fov: crate::cockpit::FOV, look_sens: 1.0, show_hud: true, hud_cockpit: false, minimap: true, landing_guidance: true, runway_assist: true, keyboard_rudder: 1.0, keyboard_rudder_fine: 0.35, remember_start: false, arcade: false, collision_mode: CollisionMode::default(), minimap_plan: false, minimap_heading_up: false, minimap_zoom: 12, minimap_size: 280.0, minimap_height: 280.0, taa: true, live_traffic: true, live_weather: true, real_time: true, last: None }
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("fablegame").join("settings.json"))
}

impl Settings {
    /// The saved settings; the defaults for anything missing or unreadable.
    pub fn load() -> Self {
        let Some(j) = path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok()) else {
            return Self::default();
        };
        Self::from_json(&j)
    }

    fn from_json(j: &serde_json::Value) -> Self {
        let d = Self::default();
        let b = |k: &str, v: bool| j[k].as_bool().unwrap_or(v);
        let f = |k: &str, v: f64| j[k].as_f64().filter(|x| x.is_finite()).unwrap_or(v);
        Self {
            lang: j["language"].as_str().and_then(Lang::parse),
            sound_on: b("sound", d.sound_on),
            volume: f("volume", d.volume as f64).clamp(0.0, 1.0) as f32,
            cam_mode: if j["camera"].as_str() == Some("cockpit") { 1 } else { 0 },
            cockpit_fov: f("cockpit_fov", d.cockpit_fov as f64).clamp(35.0, 85.0) as f32,
            look_sens: f("mouse_look_speed", d.look_sens).clamp(0.25, 3.0),
            show_hud: b("hud", d.show_hud),
            hud_cockpit: b("hud_cockpit", d.hud_cockpit),
            taa: b("taa", d.taa),
            minimap: b("minimap", d.minimap),
            landing_guidance: b("landing_guidance", d.landing_guidance),
            runway_assist: b("runway_assist", d.runway_assist),
            keyboard_rudder: f("keyboard_rudder", d.keyboard_rudder).clamp(0.25, 1.0),
            keyboard_rudder_fine: f("keyboard_rudder_fine", d.keyboard_rudder_fine).clamp(0.1, 0.75),
            remember_start: b("remember_start", d.remember_start),
            arcade: b("arcade", d.arcade),
            collision_mode: j["collision_mode"].as_str().and_then(CollisionMode::parse).unwrap_or_default(),
            minimap_plan: b("minimap_plan", d.minimap_plan),
            minimap_heading_up: b("minimap_heading_up", d.minimap_heading_up),
            minimap_size: f("minimap_size", d.minimap_size as f64).clamp(240.0, 640.0) as f32,
            minimap_height: f("minimap_height", f("minimap_size", d.minimap_size as f64)).clamp(240.0, 640.0) as f32,
            minimap_zoom: f("minimap_zoom", d.minimap_zoom as f64).clamp(3.0, 17.0) as u8,
            live_traffic: b("live_traffic", d.live_traffic),
            live_weather: b("live_weather", d.live_weather),
            real_time: b("real_time", d.real_time),
            last: b("remember_start", d.remember_start).then(|| LastStart::from_json(&j["last_start"])).flatten(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "language": self.lang.map(|l| l.code()),
            "sound": self.sound_on,
            "volume": self.volume,
            "camera": if self.cam_mode == 1 { "cockpit" } else { "chase" },
            "cockpit_fov": self.cockpit_fov,
            "mouse_look_speed": self.look_sens,
            "hud": self.show_hud,
            "hud_cockpit": self.hud_cockpit,
            "taa": self.taa,
            "minimap": self.minimap,
            "landing_guidance": self.landing_guidance,
            "runway_assist": self.runway_assist,
            "keyboard_rudder": self.keyboard_rudder,
            "keyboard_rudder_fine": self.keyboard_rudder_fine,
            "remember_start": self.remember_start,
            "arcade": self.arcade,
            "collision_mode": self.collision_mode.key(),
            "minimap_plan": self.minimap_plan,
            "minimap_heading_up": self.minimap_heading_up,
            "minimap_zoom": self.minimap_zoom,
            "minimap_size": self.minimap_size,
            "minimap_height": self.minimap_height,
            "live_traffic": self.live_traffic,
            "live_weather": self.live_weather,
            "real_time": self.real_time,
            "last_start": self.last.as_ref().map(LastStart::to_json),
        })
    }

    /// Writes them (through a temporary file, so a crash cannot leave half a file).
    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.to_json()).unwrap_or_default();
        if let Err(e) = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &p)) {
            log::warn!("saving settings to {}: {e}", p.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings { lang: Some(Lang::Fr), sound_on: false, volume: 0.4, cam_mode: 1, cockpit_fov: 50.0, look_sens: 1.5, show_hud: false, hud_cockpit: true, minimap: false, landing_guidance: false, runway_assist: false, keyboard_rudder: 0.8, keyboard_rudder_fine: 0.4, remember_start: true, arcade: true, collision_mode: CollisionMode::EndFlight, minimap_plan: true, minimap_heading_up: true, minimap_zoom: 15, minimap_size: 420.0, minimap_height: 300.0, taa: false, live_traffic: false, live_weather: false, real_time: false, last: Some(LastStart { lat: 37.6, lon: -122.4, name: "KSFO".into(), airport: Some("KSFO".into()), start: "approach".into(), runway: Some("28L".into()), aircraft: "c172".into() }) };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_json(&serde_json::json!({"remember_start": true, "last_start": {"lat": 300.0, "lon": 0.0}})).last, None);
        let mut old = s.to_json();
        old.as_object_mut().unwrap().remove("remember_start");
        let fresh = Settings::from_json(&old);
        assert!(!fresh.remember_start && fresh.last.is_none());
        let legacy = Settings::from_json(&serde_json::json!({"minimap_size": 420}));
        assert_eq!((legacy.minimap_size, legacy.minimap_height), (420.0, 420.0));
        // Missing or odd values fall back to the defaults.
        let j = serde_json::json!({ "volume": 7.0, "language": "xx" });
        let t = Settings::from_json(&j);
        assert_eq!((t.volume, t.lang, t.taa), (1.0, None, true));
        assert_eq!((t.minimap, t.minimap_heading_up, t.minimap_zoom), (true, false, 12));
        assert_eq!(Settings::from_json(&serde_json::json!({"minimap_size": 9999})).minimap_size, 640.0);
        assert_eq!(Settings::from_json(&serde_json::json!({"minimap_size": -5})).minimap_size, 240.0);
        assert_eq!(Settings::from_json(&serde_json::json!({"minimap_zoom": 999})).minimap_zoom, 17);
        assert_eq!(Settings::from_json(&serde_json::json!({"minimap_zoom": -5})).minimap_zoom, 3);
    }
}

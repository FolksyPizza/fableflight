
<div align="center">

# FableFlight

### An Open-World Flight Simulator Built in Pure Rust

Explore the skies with real-world weather, live air traffic, and an immersive open-world flying experience.

*Created with Claude Fable 5.2 & Opus 5.5*

<br>

[![Discord](https://img.shields.io/badge/Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/9akYzMRBc6)
[![Rust](https://img.shields.io/badge/Rust-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
<br>
</div>
<div align="center">
  <video
    src="https://github.com/user-attachments/assets/4f8833da-13c0-4d20-a50b-c0acf2d88df1"
    width="800"
    controls>
  </video>
</div>

## Build and run

Install the [current stable Rust toolchain](https://rustup.rs/). If Rust is already installed through rustup, run `rustup update stable`. Clone the fork and open the checkout.

```sh
git clone https://github.com/FolksyPizza/fableflight.git
cd fableflight
```

**Linux**

Install the build dependencies for your distribution once. The runtime libraries alone do not provide the `libudev.pc` and `alsa.pc` files Cargo needs.

| Distribution | Command |
| --- | --- |
| Debian or Ubuntu | `sudo apt update && sudo apt install build-essential pkg-config libasound2-dev libudev-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel systemd-devel` |
| Arch Linux | `sudo pacman -S --needed base-devel pkgconf alsa-lib systemd-libs` |

```sh
cargo run --release --locked
```

If Cargo reports missing `libudev` on Ubuntu, install the packages above and rerun Cargo. The command `./scripts/build-linux.sh --check` checks the native prerequisites, and `pkg-config --modversion alsa libudev` shows the detected versions. If the compiler or linker is killed during the release build, try thin link-time optimization and fewer build jobs.

```sh
CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8 ./scripts/build-linux.sh --jobs 2
./target/release/fablegame
```

On Linux the loading window stays responsive while graphics shaders compile. Its title shows the current stage and elapsed time. The main menu opens before the full aircraft shaders compile. Those shaders prepare when you start a flight. Startup also prints the selected GPU and backend in the terminal. If it reports `llvmpipe` or `lavapipe`, the system is exposing a CPU renderer. Check for a hardware adapter with `vulkaninfo --summary` (from the `vulkan-tools` package on Ubuntu) and check the GPU driver. Software rendering remains available for scripted captures or with `FABLE_ALLOW_SOFTWARE=1`.

**macOS and Windows**

On macOS, install the Xcode command-line tools with `xcode-select --install` if needed. On Windows, use the stable Rust MSVC toolchain and the C++ build tools for Visual Studio.

```sh
cargo run --release --locked
```

To install the executable from this checkout, run `cargo install --path . --locked --bin fablegame`, then launch it with `fablegame`.

### Update a source checkout

```sh
git pull --ff-only
cargo run --release --locked
```

Cargo builds the pulled changes and starts Fable Flight. For the optional graphical update window, run `python3 scripts/launch.py` in place of the Cargo command after pulling.

The launcher opens before the simulator builds. On Linux it checks for the required development packages before compiling. It shows aircraft photography, build progress, and **Skip update** when a previous build is available. The new build opens automatically after a successful update. An unchanged Git revision opens its installed copy. The first build can take several minutes. For a terminal-only run use `python3 scripts/launch.py --no-ui`. The window uses Python's Tk interface. On macOS, the Python installer from python.org includes Tk. If a build fails, see `target/launcher/build.log` and use **Skip update** to open the previous build.

This workflow builds the pulled source in the background. Use a clean checkout when you want the launcher to pick up a new revision. For a Cargo-installed copy, rerun the install command after pulling.

## First flight

Open **Plan a flight**, choose an airport or pick a point on the globe, then select your aircraft. Choosing an airport shows its runway ends, lengths and surfaces. Choose a specific runway or let the simulator choose one into the wind. Choose **Parking** for a cold departure away from the runway, **Runway** for a prepared takeoff, **Final** for a configured approach, or **Overhead** for a start in the air. You can also start in the air from any point on the globe. The planner starts with no departure selected and returns to a wide globe view after a flight. Turn on **Settings → General → Flight planner → Remember last departure** to restore the last airport, runway and aircraft. If detailed airport data is missing, runways with bundled coordinates are still available.

Select **Start flight**. The planner fades as the camera moves to the view above the departure area. Two counter-rotating arcs in the lower-left corner indicate loading. The globe, satellite preview and flight share the selected clock. In **Conditions → Time**, turn off **Real time** to set local solar time at the chosen location. The planner raises the camera before long-distance search moves and keeps the bundled globe visible while the destination scenery is prepared. The prepared terrain fades in together, with a darker map palette at night so the coast remains readable. It stays visible while the simulator prepares terrain and scenery for the overhead view, the descent and the flight camera. The detailed scene fades in before the descent, then clouds and rain return as the camera approaches the aircraft. Once those map loads finish, the spinner disappears and the camera descends into the cockpit or chase view. A prepared local departure descends in roughly six seconds. On a cold start the camera holds in orbit while scenery loads, then takes roughly eight seconds to descend to the aircraft. Both paths ease into the selected flight camera. For an airborne start, the aircraft stays frozen after the descent while the exterior camera changes angles. Use **Start flight** in the lower-right corner or press **Enter** to take control. Press **C** to preview the cockpit or exterior view while frozen. A parking start puts you on the ramp in first person. Walk with **WASD** and look with the arrow keys or by right-dragging. Follow the direction cue to the cover, wheel chocks, flight surfaces, and boarding point. Press **Enter** near each point. Start the engine with its cockpit controls or **F4**, release the parking brake with **.**, and taxi to your selected runway. Where a taxi network and hold are mapped, the cyan line on the moving map guides you to the hold. An airport without suitable parking offers an overhead start. The instruments fade in when you take control. Long frames slow the move so the camera does not jump ahead. Press Enter during the descent to go straight to the aircraft. Missing data uses cached or generated scenery. Departure gives optional Google 3D imagery up to three minutes to prepare before falling back to the available scenery. Time varies with the location, cache, graphics settings, and provider.

The 8192 × 4096 orbital globe and all 21600 × 10800 pixels of NASA’s Blue Marble scan are bundled. Regional terrain streams the native-resolution scan in small patches when detailed imagery is unavailable. Orbital sunlight responds to elevation in the bundled relief atlas. FableFlight generates terrain and scenery from its default imagery, elevation and vector sources with Google Maps turned off. If imagery is unavailable, it can still draw terrain using the bundled globe and any available land-cover and elevation data. A fresh offline install uses bundled NASA imagery and six-arc-minute NOAA elevation samples for 3D terrain. The terrain keeps mapped runways level at the published airport elevation. Mapped ocean uses the sea-level datum. The terrain filter corrects isolated elevation spikes where nearby land samples agree and clips implausible ridges just beyond low coastal runways, including LaGuardia. Detailed satellite scenery and city geometry depend on available map tiles. [NASA Blue Marble Next Generation](https://science.nasa.gov/earth/earth-observatory/blue-marble-next-generation/base-topography/) · [NOAA ETOPO1 elevation source](https://www.ncei.noaa.gov/products/etopo-global-relief-model)

## Graphics presets

Choose **Ultra** in **Settings → Graphics** on a high-end GPU such as the Radeon RX 9070 XT. Ultra traces volumetric clouds at the scene’s full resolution with finer march steps. It uses more closely spaced reflection samples on water and building glass, a weighted filter for the 6K sun shadows, and longer scenery detail distances. Ultra starts at native resolution. Increase **Render resolution** separately to supersample, or lower **Clouds** or **Reflections** individually if frame rate drops.

To try Ultra and log frame timing for one run, use `cargo run --release --locked -- --graphics ultra --fps-log`. Frame rate depends on the resolution, scene, weather, drivers, and GPU.

## Graphics on Apple silicon

Native Apple silicon builds start on **Low** with a fresh graphics configuration. Low renders the scene at 60% of the window resolution, launches one-quarter as many cloud rays as High at the same scene resolution, and uses the base terrain mesh. The interface renders at window resolution. With a saved configuration, choose **Low** in **Settings → Graphics** or compare another preset for a single run using `cargo run --release -- --graphics low --fps-log`. The log includes the adapter and rendering backend, internal resolution, average FPS and slowest frame. Compare presets at the same location, weather, view and window size after scenery has settled.

## Storage and removal

FableFlight keeps downloaded imagery, elevation and map tiles for reuse. To free that space and keep the game, open **Settings → Map and data → Scenery cache → Clear cache**. The game may download scenery again as you fly. If enabled, Google’s 3D tile cache also follows the provider’s expiry and size limits.

To remove FableFlight and its data, quit the game first. Run `cargo uninstall fablegame` if you installed it with Cargo. If you built it from source, remove the checkout’s `target` directory to remove the build, or remove the entire checkout if you no longer need it. If you copied the executable elsewhere, delete that copy too.

Remove the following FableFlight folders to delete downloaded maps and materials, settings, bindings, the saved Google API key, downloaded aircraft, the local flight checkpoints and app-managed captures. Back up any custom aircraft or captures you want to keep.

| Platform | Cache | Settings and user data |
| --- | --- | --- |
| macOS | `~/Library/Caches/fablegame` | `~/Library/Application Support/fablegame` |
| Linux | `${XDG_CACHE_HOME:-$HOME/.cache}/fablegame` | `${XDG_CONFIG_HOME:-$HOME/.config}/fablegame` and `${XDG_DATA_HOME:-$HOME/.local/share}/fablegame` |
| Windows | `%LOCALAPPDATA%\fablegame` | `%APPDATA%\fablegame` |

F12 screenshots go to the working directory. Remove screenshots, recordings and exports saved to paths you chose separately. If you used the JSBSim comparison commands, the system temporary directory may also contain `fablegame-jsbsim-compare`.

## Landing practice

Choose **Final**, select a runway, and turn on **Landing practice**. The simulator starts you on a configured approach. After touchdown, **Tab → Flight** reports speed, descent rate and runway placement.

## Collisions and reset

Press **R** during any flight to reset the aircraft in the air near your current location. Under **Settings → General → Flight rules → Collisions**, **Recover** is the default and puts the aircraft back into stable flight after a severe impact. **End flight** shows an immediate flight-ended screen with options to restart nearby or return to the planner. **Detailed damage** includes airframe deformation and a progressive impact when the aircraft hits a building at speed. To try one setting for a single run, pass `--collision recover`, `--collision end-flight`, or `--collision detailed`.

# Arcade mode
FableFlight is a simulator first: by default, the aircraft fly as they really do. **Arcade mode** (Settings › General, or `--arcade` for one run) turns on the game's fun features:
- the F/A-18's Sidewinders (master arm, lock and fire on keys 3, 4 and 5);
- boost on aircraft that have none in reality: the piston singles and the 747. Real afterburners and reheat (F/A-18, F-4, F-22, SR-71, Concorde) work in both modes;
- the weather and time keys in flight: T/Y for the time, N/M for the clouds. The weather panel and Go to work in both modes.

# Language
The interface is in English or French. It follows the system language, can be changed in Settings (start screen or pause menu), and is remembered. For one run: `cargo run --release -- --lang fr`.

# Joysticks, pedals and key bindings

**Q / E** steer the nose wheel on the ground and move the rudder in flight. Hold **Backslash** with **Q / E** for finer corrections. Both keyboard rudder strengths can be adjusted under **Settings → Controls → Keyboard rudder**, and the fine-control key can be rebound. Hold **left / right Option** on Mac, or **left / right Alt** on other keyboards, for the corresponding toe brake. **B** applies both brakes. **Settings → Controls → Ground handling** has a runway steering assist. It is on by default for runway departures and helps hold the centreline during the takeoff roll. Steering and brake input take priority. Braking and cornering share the tires’ available grip, which matters when using the pedals during a crosswind rollout. The visual landing gates follow the nearby ILS tuned on NAV 1 or the selected runway. They appear ahead when lined up on final and can be hidden by terrain or the aircraft.

Press **Home** or click the right stick to recenter the cockpit or chase view. **Tab** opens the in-flight toolbar. Choose a section to open its side panel. Press **Tab** again to close both. In stable airborne flight, **Menu Hold** maintains heading and altitude while the toolbar is open, and holds airspeed if there is no external throttle input. Firm flight-control input cancels the hold for that session. An autopilot you engaged yourself keeps its selected modes. Open **Tab → Autopilot → Route and arrival**, add airports by ICAO or IATA code, and choose **Follow route**. The autopilot sequences the airports and climbs as needed toward an estimated cruise target. Enter a cruise altitude in feet MSL to request a different target. The terrain estimate uses sampled elevation, so check terrain and obstacles yourself. Select a runway from the arrival list. **Arm ILS approach** is available when a usable localizer is ahead and in range.

**Settings → General → Flight rules → Pilot assistance** remembers the optional stability aid across restarts. The Autopilot panel also includes this switch and offers **Automatic takeoff** for an engine-running Cessna 172RG lined up near the start of a paved runway. The helper steers the takeoff roll, rotates, and transitions to an autopilot climb. To try **Automatic landing**, fly a Cessna 172RG or Robin DR400 on a stabilized final with approach flaps set and the landing gear down. Select the runway and engage the helper for final tracking, flare, and braking to a stop. With a usable ILS ahead, you can also arm automatic landing before final. The system takes over after the localizer and glide slope are captured and the aircraft is configured and stable. Runway automation requires loaded scenery, a paved runway at least 1,100 m long and 20 m wide, and light crosswind and tailwind. Press **F5** or make a firm control input to take over. Taxi manually after the stop.

To continue another time, open **Tab → Flight → Save flight**. There are four local slots. Use an empty slot to keep an existing flight, or choose **Replace slot** to overwrite it. From the main menu, choose **Resume saved flight** and pick a checkpoint if you have more than one. The aircraft state and remaining route are restored. Re-engage autopilot manually after resuming.

Set departure fuel in the flight planner. Near an airport, choose **Tab → Flight → Mark nearest fuel** to mark the nearest suitable refueling spot on the moving map. These spots use the simulator’s stands or aprons when mapped. When you mark fuel from the ground and a mapped taxi route is available, the moving map draws a cyan line that avoids runways. If you mark from a runway, vacate it and mark fuel again for taxi guidance. Taxi manually to the marked spot, set the parking brake, shut down the engine, and select **Refuel to full**. You can clear the marker from the same panel. **Settings → General → Flight rules → Unlimited fuel** keeps piston and jet aircraft from running out of fuel.

The moving map shows mapped aprons and taxiways at airport zoom levels. On wider screens, fuel, angle of attack, vertical speed and aircraft configuration sit beside the main instruments. The flight HUD shows wind direction and speed, ground track, fuel remaining, angle of attack (AOA) and vertical speed. Wind direction is where the wind comes from. The arrow points where it is blowing relative to the aircraft. Urgent warnings such as **PULL UP** and **STALL** appear in the attitude display. Aircraft condition and recent failures appear in a compact panel. Press **H** to toggle the HUD in the current view.

Any joystick, throttle, rudder pedals or wheel can be used: in Bindings (pause menu, or Settings on the start screen), bind each axis by moving it the way that means "more" (nose up, right roll, right rudder, full throttle, brake pressed) and each button to any action. Each axis has an Invert switch and a dead zone. Pitch, roll and rudder also have a **Curve** slider for finer movement near centre with full travel at the ends. Presets set a device up in one click; the Turtle Beach VelocityOne Flightstick and rudder pedals get theirs on their own the first time they are plugged in. Keyboard keys can be moved too. Everything is kept in `bindings.json` in the config folder.

# Aircraft
The Cessna 172RG, the Robin DR 400/180 Régent, the Gulfstream G650, the 747, the F/A-18, the F-4, the F-22, Concorde, the SR-71 and Starship. The Robin is the French wooden four-seater and glider tug: bubble canopy, the cranked Jodel wing, an all-moving tailplane, fixed gear and 60° flaps (`--aircraft dr400`); 6 slides its canopy open and shut. Like the Cessna it is built in code (`src/aircraft/dr400`), in a made-up classic Robin livery.

# Fuel and engine start
The engines burn fuel as they work (the Cessna some 30 l/h at 75% power, the 747 about 10 t/h in the cruise) and the fuel weighs: full tanks fly heavier than empty ones. Choose how much to take on the start screen (Flight); the HUD, the flight panel (fuel, flow, endurance) and the cockpit gauges show what is left, with LOW FUEL / NO FUEL warnings, and a dry engine stops. Start on a runway with the engine off ("cold start", or `--cold`) and start it by hand (I magnetos, Page Up mixture, hold O for the starter; on the jets I is the fuel controls and O engages the starters) or with F4, which also shuts it down. A windmilling engine with fuel and ignition catches again in the air; shut down on the ground, compression stops it within seconds.

# Mass and balance
The fuel and the payload (crew, passengers, baggage: set on the start screen) make the aircraft heavier or lighter and move its centre of gravity, shown against its limits in % of the mean aerodynamic chord (start screen, flight panel; CG LIMIT on the HUD). Loaded aft, the nose is lighter and quicker to answer and a gust takes longer to settle; forward, it is stable but nose-heavy and the elevator runs out sooner (a harder flare). A Cessna with four aboard and full baggage is out of its aft limit, as the real one is.

# Electrics and failures
A battery behind the master switch (Ins) and an alternator (the jets' generators) that charges it once the engine turns fast enough: the starter, the radios, the electric gauges and the lights need power, the engine itself runs on its magnetos. The pause menu's Failures page sets failures by hand or at random (about every N minutes): engine, alternator, flat battery, vacuum pump (the attitude and heading gyros run down), blocked pitot (the airspeed indicator turns into an altimeter) or static port (altimeter and VSI freeze), fuel leak. The flight panel shows the volts, battery and current, and the HUD warns LOW VOLTS and VACUUM.

# Clickable cockpits

In cockpit view, hover over a working control to see its action and setting. Click a switch, roll the mouse wheel over a lever, or hold a toe brake. Drag elsewhere to look around. Middle-drag to lean sideways or down around a yoke or panel, and middle-click to sit upright again. The target follows the visible part as it moves.

| Aircraft | Controls available from the pilot seat |
| --- | --- |
| Cessna 172RG | Battery, magnetos and starter key, throttle, propeller RPM, mixture, cowl flaps, tank selector, landing lights, gear, flaps, parking brake, toe brakes, trim wheel and yoke trim, NAV and ADF tuning, OBS, heading sync and altimeter setting |
| Robin DR400/180 Régent | Battery, magnetos, separate starter button, throttle, mixture, tank selector, landing light, parking brake, toe brakes, trim wheel, flap lever and sliding canopy handle |
| Boeing 747-400 | Overhead battery pushbutton and engine start switches, four thrust levers and reverse levers, fuel control switches, landing gear, flaps, speedbrake, alternate stabiliser trim, parking brake and toe brakes |
| Concorde | Four thrust levers, gear, toe brakes and the nose/visor selector with up, visor down, 5° and 12.5° positions |
| F/A-18C | Twin throttles, landing gear, flap selector, canopy switch and toe brakes |
| F-4E | Twin throttles, landing gear, slats/flaps selector and toe brakes |
| F-22A | Twin throttles, landing gear, canopy switch and toe brakes |
| SR-71A | Twin throttles, landing gear, drag chute handle and toe brakes |

The multi-engine aircraft share a thrust setting across engines. The piston aircraft have four fuel selector positions and a shared fuel quantity. Selecting OFF cuts fuel to the engine. The navigator and systems-operator stations, many panel switches and avionics pages remain simplified. Starship is uncrewed. [Cockpit references and modeling notes](docs/cockpits.md)

# In-flight map

A round minimap in the lower-left corner, stacked above the info panel, follows the aircraft; ` (backquote, the tilde key) shows or hides it. Choose **Satellite** or **Map** (a light road map), and **North** to keep north at the top or **Heading** to keep the nose pointing up. The controls, hints and status messages follow the game language (English/French). Place names use the corresponding translation when supplied by the map data, falling back to the local name; changing language also updates already loaded labels. City, town and village names stay upright on both backgrounds, with smaller places appearing as you zoom in and overlapping labels hidden. Use **+ / -** or the mouse wheel over the map to zoom. The compact toolbar sits under the map: click the orientation/background buttons to toggle them, or **x** to collapse it to a **Map** button. Drag the knob on the upper-right of the rim to resize it (240-640 logical pixels across, shrunk to fit between the HUD panels); the size is remembered and can also be set in the pause menu Settings. The map follows HUD visibility and remains visible beside the flight panel and temporarily hides while the pause menu is open. Visibility, background, orientation and zoom are remembered.

Runways are highlighted in cyan using the loaded 3D airfield geometry when available, falling back to database endpoint coordinates for distant airfields. Near a loaded airfield, the active runway gets orange arrival arrows and a corridor on the map. In flight on its arrival side, orange gates show a visual 3-degree descent path toward the runway aiming point. This optional visual aid uses the game traffic runway selection (or wind-based selection), not an ATC clearance or terrain-clearance calculation. Toggle it in pause Settings; the choice is remembered.

The minimap shares the terrain imagery and OpenFreeMap vector data caches. It loads tiles in the background, with at most one imagery and one vector request in flight, 64 satellite textures and 32 road-map tiles in memory. The road map is rasterized once per tile on a worker, not every frame, and does not request satellite images. Neither mode renders the 3D world a second time. Lower-resolution satellite tiles stand in while detail loads. Offline it uses cached data; missing imagery is indicated, and place names require cached vector tiles. The map is unavailable beyond 85 degrees latitude. Map and label data: [OpenFreeMap](https://openfreemap.org/), [OpenMapTiles](https://openmaptiles.org/), [OpenStreetMap contributors](https://www.openstreetmap.org/copyright).

# Radio navigation
VORs and NDBs from [OurAirports](https://ourairports.com/data/) (public domain), and an ILS on every paved runway end of 1500 m or more at medium and large airports. Tune NAV 1 (VOR or ILS) and the ADF on the flight panel's Radio page (Tab); J shows the instruments: a course deviation indicator with TO/FROM, the glide slope on an ILS and DME, and an ADF needle. [ and ] turn the OBS. Starting on a final approach tunes the runway's ILS. The altimeter is set to the QNH (from the METAR when the weather is real) unless you set it yourself; X sets it back. Courses are true: the world has no magnetic variation. `assets/mknav.py` rebuilds `assets/navaids.tsv.gz` from OurAirports' `navaids.csv`.

# Autopilot
Every aircraft has one, on its page of the flight toolbar (the attitude-indicator icon) and on keys you can rebind:
- **F5**: engage or disconnect it. It starts by holding the heading and altitude of the moment.
- **7 HDG**: fly the heading bug, moved with **−** / **=**.
- **8 NAV**: track NAV 1, a VOR radial or a localizer.
- **9 APR**: fly an ILS, localizer then glide slope.
- **0 ALT**: hold the altitude.
- **; / '**: move the preselected altitude.
- On the page: **VS** climbs or descends at a set rate and levels off at the preselected altitude; **AT**, the autothrottle, holds an airspeed.

NAV and APR can be armed before the signal comes in: they fly the heading bug until they capture the course, and APR captures the glide slope from below or from above. The modes show on a strip above the instruments, green when flying them, grey when armed.

It gives up the vertical speed rather than stall the aircraft (MIN SPD), and climbs rather than pass the never-exceed speed (MAX SPD). A firm push on the controls, the ground, a stall or an unusual attitude disconnects it, with a warning tone. The AI copilot (docs/mcp.md) can set it too, with `set_autopilot`.

# More aircraft: FlightGear's collection
Click the aircraft picture on the start screen (or **All aircraft**) to see every aircraft you can fly as a gallery; **Get more…** opens [FlightGear's aircraft collection](https://wiki.flightgear.org/FGAddon) (its 2020 catalogue, about 650 aircraft). Pick one to read its files from FGAddon: its flight model, author, the license its JSBSim file states (else FGAddon's GNU GPL v2 or later), and what it becomes here; the ones that can't be flown are hidden once read (tick **Show all**). **Download** takes its package from a FlightGear mirror and unpacks it, without sounds, scripts and splash screens, into `~/.local/share/fablegame/aircraft/<folder>/` (with a `source.json` recording the source, revision and license), and it joins the list. An installed FlightGear aircraft with a compatible visual model can be rotated by dragging its large preview in the aircraft gallery. Nothing from FlightGear ships with the game.

A downloaded aircraft is **drawn with its own 3D model** (src/fgmodel.rs): the AC3D meshes and textures (PNG, JPEG, SGI .rgb) its model file names, with sub-models placed by their offsets. Its animations become our moving parts where we drive the same thing: control surfaces, flaps, slats, spoilers, gear retraction, steering, oleo compression, wheels and propellers. Others are taken at rest, and select animations hide what is off at rest or swap propeller blades for their blur. It sits on its own landing gear (the JSBSim contact points), sees from its pilot's view, and gets its picture from its previews.

It is not flown by JSBSim or YASim: its flight model is *fitted* to ours (src/hangar.rs). Its mass, inertia, wing, lift and drag curves, flaps, engines and fuel go into a `Spec` built on the closest of our types, which lends its instruments, sounds and damage rules: propeller aircraft the Cessna's, fighters and anything with afterburners the Hornet's, other jets of 25 t or more the 747's. A JSBSim aircraft's curves are read from its own aerodynamic tables. A YASim aircraft (most of the catalogue) has none: YASim builds it from its geometry and solves for the lift and drag that fly the approach and cruise its file gives, and src/yasim.rs does the same for a polar, with its mass spread over the airframe around its ballast, engines and fuel for the centre of gravity and inertia. Helicopters and autogyros, rockets, gliders, balloons and electric aircraft can't be flown yet.

**A word of caution:** the collection's aircraft are made by FlightGear's community and vary a lot. Some are not of high quality (rough models, odd files), and since each is fitted to our flight model rather than flown by its own, some don't fly realistically here. From the command line: `--fg-install SR22T` downloads, `--aircraft fg:sr22t` flies it, `--fg-survey [TEXT]` reads catalogue entries (those named JSBSim, or matching TEXT) and prints which can be flown, `--jsbsim-fit FILE...` prints what JSBSim (or YASim) files become, and `--fly-check KEY` flies any aircraft in an empty world (take-off, top speed, stall).

# Comparing with a JSBSim flight model
`cargo run --release -- --jsbsim-compare path/to/c172p.xml [--aircraft 747]` reads a JSBSim aircraft file (the XML flight models JSBSim and FlightGear use; none ship with the game) and prints how its aerodynamics compare with one of ours: geometry, mass and inertia, the lift and drag curves clean and with flaps, stall, best glide, the roll rate at full aileron, and the `Spec` values that would match its curves, as a start for tuning by hand.

# Live air traffic
Real aircraft around you, as reported by ADS-B receivers (data from [adsb.lol](https://adsb.lol), ODbL), fly in the sky with tags (callsign, type, altitude, speed, distance); the flight panel (Tab) lists the nearest. It can be switched off in the pause menu's Settings.

# Missiles
The F/A-18C carries its two wingtip Sidewinders. Press **3** to set Master Arm, **4** to find and lock a target (press it again to step to the next one), and **5** to fire. On a gamepad, hold **Y** and press **left** on the d-pad to arm, **Y** + **right** to lock or cycle, and release **Y** on its own at a locked target to fire. The seeker looks about 45° either side of the nose for airborne aircraft — the real ADS-B traffic and airport departures and arrivals — and needs a few seconds to build a lock; the HUD shows `TRACK` until it is solid, then `LOCK`. Armed with no lock, firing says `NO LOCK`; with Master Arm off it says `MASTER ARM OFF`. The missile leaves the rail, boosts, follows the target's heat and detonates on a proximity fuse. The rail stays, the empty wingtip is visible, and the second missile is still there. The keys can be moved on the Bindings page.

# Real weather
The weather follows the real conditions where you fly: the nearest METAR within 60 km ([aviationweather.gov](https://aviationweather.gov)) sets the wind, gusts, visibility, cloud layers and thunderstorms; with no airport near, [Open-Meteo](https://open-meteo.com)'s model stands in. It is looked up again every 10 minutes or 30 km, and the sky eases into each report. The gust field varies across the flight path and weakens close to the surface. Switch it off in the weather settings (start screen, flight panel or F3) to set the weather by hand.

The clock can follow real time too ("Real time" above the day sliders): the sun and the date are those where you fly, now (solar time, so the local hour can differ from the legal one by up to an hour or so). Pressing T / Y to set the time by hand leaves real time.

Settings (language, sound, camera, HUD, anti-aliasing) are kept in `settings.json` in the config folder (`~/.config/fablegame` on Linux).

# AI copilot (MCP)
The simulator can host an AI copilot through the [Model Context Protocol](https://modelcontextprotocol.io). Build the game and the small stdio bridge, run the game normally, and point your AI client at `target/release/fableflight-mcp`:

```sh
cargo build --release
./target/release/fablegame
```

For OpenCode, add a local server to `opencode.json` (or use `opencode mcp add fableflight -- /absolute/path/to/fableflight-mcp`):

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "fableflight": {
        "type": "local",
        "command": ["/absolute/path/to/fableflight-mcp"]
      }
    }
  }
}
```

For Claude Code: `claude mcp add fableflight /absolute/path/to/fableflight-mcp`. Claude Desktop uses the same command in its `mcpServers` configuration. The bridge attaches to the running game over `127.0.0.1:42761`; start a flight, then ask the AI to read the aircraft state, tune radios, set flaps or weather, or take a screenshot. `--mcp-port PORT` changes the game's port and `FABLEFLIGHT_MCP_PORT` tells the bridge where to look. `--no-mcp` disables the service.

The copilot can read the aircraft, navigation, weather, traffic and airports, and can set the throttle, flaps, gear, speedbrake, parking brake, mixture, trim, engine state, radios, weather, time and the aircraft's position. It cannot move the stick: attitude stays with the human pilot. See [docs/mcp.md](docs/mcp.md) for the full tool list, units and examples.

# Contributions
Please contribute anything if you want to! Improvements to flight sim realism, graphics, optimization, UI, and especially plane models are needed! (((=

For substantial AI-assisted contributions, use Claude Opus 5.5, GPT-6 Astra, Claude Fable, or a model with equivalent or greater capability. Sonnet, GPT-6.1 Sol, and less capable models should only handle simple fixes or very small features; switch to a stronger model for larger tasks, including aircraft creation and flight-model tuning. See [AGENTS.md](AGENTS.md) for the full guidance.

I personally recommend using Opus 5.5 high for most tasks. If you don't have a routine for conserving tokens a general rule of thumb is to give models specific tasks (e.g. decompose the cockpit and plane body into different prompts) and keep conversations to one prompt, starting a new chat if refinement is needed.

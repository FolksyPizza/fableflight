// Volumetric cloud layer: raymarched at half resolution with a fresh jitter every
// frame, accumulated over frames by reprojection, then upsampled onto the scene.
@group(1) @binding(0) var t_depth: texture_depth_2d;

const SIGMA = 0.05;   // extinction per unit density, 1/m
const MS_GAIN = 2.6;  // energy the octave sum misses in thick cloud

fn hg(c: f32, g: f32) -> f32 { return (1.0 - g * g) / (4.0 * PI * pow(1.0 + g * g - 2.0 * g * c, 1.5)); }
// Droplet (Mie) phase as three lobes: a sharp forward peak, a broad forward lobe that
// carries the silver lining out to ~45 degrees from the sun, and a weak back lobe.
// k < 1 flattens it for light that has already scattered a few times.
// A narrow fourth lobe stands in for the diffraction peak (a big share of a droplet's
// scattering stays within a few degrees of the sun): thin cloud over the sun blazes.
fn cloud_phase(c: f32, k: f32) -> f32 {
  return hg(c, 0.86 * k) * 0.42 + hg(c, 0.5 * k) * 0.3 + hg(c, -0.25 * k) * 0.2 + hg(c, 0.97 * k) * 0.08;
}
// Light that has diffused right through a cloud still mostly travels away from the
// sun, so a cloud in front of a low sun glows through its body instead of going grey.
// Diffuse transmission falls off slowly with depth, ~1/(1 + 0.75 (1 - g) od) for g ~ 0.85.
// Thin parts let more through than thick ones (second term), so the glow keeps the
// cloud's structure. The SILVER term is light scattered forward a few times near the
// thin edges; the outline itself gets the RIM term after the march (see fs).
const GLOW = 0.06;
const SILVER = 0.5;
const RIM = 1.5;
fn cloud_glow(cs: f32, od: f32) -> f32 {
  return GLOW * hg(cs, 0.85) * (1.0 / (1.0 + 0.3 * od) + exp(-0.3 * od))
        + SILVER * hg(cs, 0.9) * exp(-0.35 * od);
}

// ---------------------------------------------------------- mid-level cloud
// A thin layer at G.alto.y: altocumulus (rafts of small cloudlets, lined up in rows
// across the wind) that thickens into a grey altostratus sheet as the amount grows.
// Each cloudlet is a lens reaching from ALTO_BOT below the layer height to ALTO_TOP
// above it where it is thickest (render.rs ALTO_TOP).
const ALTO_TOP = 700.0;
const ALTO_BOT = 300.0;
// How much mid-level cloud this region has (0..1): the slider, in patches ~100 km across.
fn alto_amount(p: vec3f) -> f32 {
  let q = p + G.weather.xyz * 1.5;
  let m = textureSampleLevel(t_noise, s_repeat, q / 95000.0 + vec3f(0.27, 0.64, 0.11), 0.0);
  return clamp(G.alto.x * 1.5 - 0.3 + (m.r - 0.5) * 1.5 + (m.g - 0.6) * 0.7, 0.0, 1.0);
}
// The lens at p: x bottom, y top (metres from the layer height), z density inside;
// w how far it has thickened into altostratus. far = 1 when the cloudlets are
// smaller than a pixel (their mean is used instead).
fn alto_lens(p: vec3f, a: f32, far: f32) -> vec4f {
  let q = p + G.weather.xyz * 1.5;
  let st = smoothstep(0.6, 1.0, a);
  // Squeezed along the wind, so the cloudlets line up in rows across it.
  let w = G.storm.xyz;
  let qs = q + w * (dot(q, w) * 0.6);
  let big = textureSampleLevel(t_noise, s_repeat, qs / 24000.0 + vec3f(0.53, 0.18, 0.77), 0.0);
  let el = textureSampleLevel(t_noise, s_repeat, qs / 6500.0 + vec3f(0.12, 0.91, 0.38), 0.0);
  let cl = mix(el.b * 0.7 + el.a * 0.3, 0.6, far);
  let raft = big.g * 0.7 + big.r * 0.3;
  let v = mix(cl * 0.65 + raft * 0.45, 0.45 + raft * 0.4 + big.b * 0.15, st);
  let thr = 1.02 - a * 0.75;
  // Altostratus is not uniform either: thicker and thinner stretches, darker and
  // paler from below.
  let d = smoothstep(thr, thr + mix(0.12, 0.35, st), v) * mix(1.0, 0.35 + 0.9 * smoothstep(0.45, 0.85, raft * 0.75 + cl * 0.35), st);
  return vec4f(-ALTO_BOT * d * mix(0.5, 1.0, st), ALTO_TOP * d * mix(0.55, 1.0, st), mix(0.55, 0.3, st), st);
}
// Where a ray (from oc) is inside the layer's slab: x entry, y exit, x < 0 = never.
fn alto_range(oc: vec3f, rd: vec3f) -> vec2f {
  let rg = G.planet.w;
  let ah = G.alto.y;
  let s0 = ray_sphere(oc, rd, rg + ah - ALTO_BOT);
  let s1 = ray_sphere(oc, rd, rg + ah + ALTO_TOP);
  let alt = G.up.w;
  var t1 = s1.y;
  if (s0.x > 0.0) { t1 = s0.x; }
  if (alt < ah - ALTO_BOT) { return vec2f(s0.y, s1.y); }
  if (alt < ah + ALTO_TOP) { return vec2f(0.0, t1); }
  if (s1.x <= 0.0) { return vec2f(-1.0); }
  return vec2f(s1.x, t1);
}
// Premultiplied colour and transmittance of the layer between t0 and t1, lit as the
// cumulus are (same phase function and multiple-scattering octaves).
// The towns' light on a cloud at p (planet-relative), from below: the clouds
// over a city glow orange, brightest underneath.
fn city_amb(p: vec3f) -> vec3f {
  if (!glow_on()) { return vec3f(0.0); }
  // Light scattered many times inside the cloud comes out paler than it went in.
  let c = city_up(p + G.planet.xyz, length(p) - G.planet.w - G.glow_e.w);
  return mix(c, vec3f(luma(c)), 0.3) * (0.75 / PI);
}
fn alto_march(oc: vec3f, rd: vec3f, t0: f32, t1: f32, a: f32, jit: f32, cs: f32, sky_amb: vec3f, gnd_amb: vec3f) -> vec4f {
  var T = 1.0;
  var L = vec3f(0.0);
  let n = 6;
  let dt = (t1 - t0) / f32(n);
  let far = smoothstep(30000.0, 110000.0, 0.5 * (t0 + t1));
  let sun = G.sun_dir.xyz;
  for (var i = 0; i < n; i++) {
    let ts = t0 + dt * (f32(i) + jit);
    let p = oc + rd * ts;
    let r = length(p);
    let h = r - G.planet.w;
    let z = h - G.alto.y;
    let ln = alto_lens(p, a, far);
    let rho = ln.z * smoothstep(ln.x, ln.x + 40.0, z) * smoothstep(ln.y, ln.y - 60.0, z);
    if (rho <= 0.001) { continue; }
    let upn = p / r;
    let mu = dot(upn, sun);
    // Towards the sun the light crosses the rest of the lens above.
    let od = SIGMA * rho * (ln.y - z) / max(mu, 0.08);
    var ms = 0.0;
    var ka = 1.0;
    var kb = 1.0;
    var kc = 1.0;
    for (var k = 0; k < 4; k++) {
      ms += ka * exp(-od * kb) * cloud_phase(cs, kc);
      ka *= 0.55;
      kb *= 0.38;
      kc *= 0.5;
    }
    ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
    ms += cloud_glow(cs, od);
    let hn = clamp((z - ln.x) / max(ln.y - ln.x, 1.0), 0.0, 1.0);
    let sunc = sun_trans(h, mu) * G.sun_dir.w;
    let amb = sky_amb * mix(0.35, 1.0, hn) + gnd_amb * mix(1.0, 0.3, hn);
    let e = rho * SIGMA;
    let S = ((sunc + moon_light(upn, h)) * ms * MS_GAIN + amb) * e;
    let st = exp(-e * dt);
    L += T * S * (1.0 - st) / e;
    T *= st;
  }
  return vec4f(L, T);
}

// --------------------------------------------------------------- lightning
// A flash lights the cloud around its channel from inside: the light diffuses
// through a few km of cloud, blinding white close to the channel, a glow
// further out (on top of the plain falloff that lights the base from below).
fn flash_cloud(P: vec3f) -> vec3f {
  let n = u32(G.stormfx.z);
  var e = 0.0;
  for (var i = 0u; i < n; i++) {
    let a = G.flash[2u * i];
    let d = seg_dist(P, a.xyz, G.flash[2u * i + 1u].xyz);
    e += a.w * FLASH_C * (1.0 + 6.0 * exp(-d / 2500.0)) / (d * d + 1000.0 * 1000.0);
  }
  return FLASH_COL * e;
}

// ------------------------------------------------------------- rain shafts
// Under a storm or a shower the rain hangs from the base to the ground as a grey
// curtain, downwind of the updraft, streaked and thickest in the middle. Each
// flagged cell's shaft is a vertical cylinder; the ray's stretch through it (under
// the base, before the scene) is integrated in a few samples. Returns premultiplied
// colour and transmittance; z of dist = where along the ray it mostly is.
struct Rain { L: vec3f, T: f32, t: f32 };
fn seed_of(sp: vec4f) -> f32 { return sp.w - floor(sp.w); }
// How far back from a squall line's shelf-cloud nose a point p is (storm_sd's u,
// m; cen its rain's centre, X along it, Y downwind).
fn shelf_u(p: vec3f, cen: vec3f, X: vec3f, Y: vec3f, R: f32, sp: vec4f) -> f32 {
  let ph = p - cen;
  let cxl = dot(ph, X);
  return R * (SHELF_F + 0.12 * sin(cxl / 9000.0 + seed_of(sp) * 40.0) + 0.07 * sin(cxl / 3700.0 + seed_of(sp) * 17.0)) - dot(ph, Y) - 0.45 * R;
}
// A squall line's rain u metres back from its leading edge (line radius R):
// x how much (none under the shelf, heavy right behind it, then a long tail of
// lighter rain easing off), y how much of it is the heavy curtain. Must match
// storm.rs line_rain.
fn line_rain(u: f32, R: f32) -> vec2f {
  let ws = SHELF_W * R;
  let heavy = 1.0 - smoothstep(ws + 0.6 * R, ws + 1.2 * R, u);
  let tail = 0.35 * (1.0 - smoothstep(ws + 1.2 * R, ws + 3.6 * R, u));
  let on = smoothstep(0.7 * ws, 1.1 * ws, u);
  return vec2f(on * mix(tail, 1.0, heavy), on * heavy);
}
fn storm_rain(oc: vec3f, rd: vec3f, tmax: f32, mask: u32, jit: f32, sky_amb: vec3f, gnd_amb: vec3f) -> Rain {
  var o = Rain(vec3f(0.0), 1.0, 0.0);
  let n = u32(G.storm.w);
  let rg = G.planet.w;
  let base = G.cloud.y;
  var wsum = 0.0;
  for (var i = 0u; i < n; i++) {
    if ((mask & (1u << i)) == 0u) { continue; }
    let c = G.storm_cells[2u * i];
    let sp = G.storm_cells[2u * i + 1u];
    let R = c.w * 1000.0;
    // Rain once the tower is a few km deep: a shower's light to moderate, an
    // ordinary storm's heavy, a big mature one's the heaviest (storm.rs
    // shaft_amount).
    let htop = (sp.y - base) * storm_top(sp.x);
    let amt = smoothstep(1800.0, 3500.0, htop) * smoothstep(900.0, 2200.0, R) * (0.35 + 0.4 * smoothstep(0.45, 0.8, sp.x) + 0.25 * smoothstep(2500.0, 6000.0, R));
    if (amt <= 0.0) { continue; }
    let young = 1.0 - smoothstep(0.4, 0.6, sp.x);
    let up = normalize(c.xyz - G.weather.xyz * 0.001);
    // A squall line (its half-length L2; storm_sd): a curtain of heavy rain all
    // along it right behind the shelf cloud, then lighter rain trailing far
    // behind (storm.rs line_rain). X along it, Y downwind.
    let L2 = floor(sp.w) * 500.0;
    let line = L2 > 0.0;
    let X = normalize(cross(up, G.storm.xyz));
    let Y = cross(X, up);
    // A supercell's core (rain and hail) falls off to the downwind left of
    // its updraft, a dense, dark shaft with a sharp edge, leaving the base
    // over the updraft and its wall cloud rain-free (storm.rs shaft_at).
    let sup = storm_super(sp);
    let cen = up * rg + G.storm.xyz * (R * mix(0.45, 0.32, sup)) + X * (R * 0.28 * sup);
    let rr = R * mix(0.8, 0.42, sup);
    // (The test cylinder also holds the shaft's slant and its foot.)
    let rt = rr * 1.75 + base * 0.15;
    let v = cen - oc;
    let vh = v - up * dot(v, up);
    let rh = rd - up * dot(rd, up);
    let a = dot(rh, rh);
    let b = dot(vh, rh);
    let cc = dot(vh, vh) - rt * rt;
    var t0 = 0.0;
    var t1 = tmax;
    if (line) {
      // The box holding all of it: along the line, and from the gust front back
      // to the end of the trailing rain (centred 0.95 R upwind of cen).
      let o2 = vec2f(dot(-vh, X), dot(-vh, Y) + 1.4 * R);
      let d2 = vec2f(dot(rh, X), dot(rh, Y));
      let hx = vec2f(L2 + 1.2 * R, 2.6 * R + base * 0.15);
      let inv = 1.0 / select(d2, vec2f(1e-9), abs(d2) < vec2f(1e-9));
      let ta = (-hx - o2) * inv;
      let tb = (hx - o2) * inv;
      t0 = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), 0.0);
      t1 = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), tmax);
    } else if (a > 1e-7) {
      let disc = b * b - a * cc;
      if (disc <= 0.0) { continue; }
      let sq = sqrt(disc);
      t0 = max((b - sq) / a, 0.0);
      t1 = min((b + sq) / a, tmax);
    } else if (cc > 0.0) { continue; }
    // Only under the cloud base.
    let sb = ray_sphere(oc, rd, rg + base);
    if (G.up.w < base) { t1 = min(t1, sb.y); } else { t0 = max(t0, select(t1, sb.x, sb.x > 0.0)); }
    if (t1 <= t0) { continue; }
    let ns = select(6, 8, line);
    let dt = (t1 - t0) / f32(ns);
    // The sun reaching the shaft, looked up once in its middle. (A squall
    // line's rain lies under its towers and anvil, and the stretch through it
    // can be tens of km long: its middle says little, so per sample.)
    var vis = cloud_sun(oc + rd * (0.5 * (t0 + t1)), false).vis;
    for (var k = 0; k < ns; k++) {
      let ts = t0 + dt * (f32(k) + jit);
      let p = oc + rd * ts;
      let h = length(p) - rg;
      // The shaft slants: the rain lower down has been carried further
      // downwind, and spreads out in a foot where the downdraft meets the ground.
      let below = clamp(base - h, 0.0, base);
      let ph = p - cen - G.storm.xyz * (below * 0.15);
      let rad = length(ph - up * dot(ph, up)) / (rr * (1.0 + 0.3 * smoothstep(500.0, 0.0, h)));
      // Curtains and streaks: noise squashed flat along the vertical (columnar),
      // drifting with the wind; fading out towards the ground in places (virga).
      let q = p + G.weather.xyz;
      var qc = q - up * dot(q, up) * 0.93;
      let cxl = dot(ph, X);
      let us = shelf_u(p, cen, X, Y, R, sp) + below * 0.15;
      // A squall line's curtain is seen end-on, through kilometres of it: its
      // streaks are sheets across the line (the same all through it, varying
      // along it and slanting a little), or they averaged out along the ray
      // into one flat, even band. Finer than a lone shaft's: it is seen close.
      var sc = vec2f(9000.0, 2600.0);
      if (line) {
        qc = X * (cxl + h * 0.2) + up * (h * 0.07) + Y * (us * 0.08);
        sc = vec2f(2600.0, 800.0);
      }
      let sn = textureSampleLevel(t_noise, s_repeat, qc / sc.x + vec3f(0.6, 0.2, 0.4), 0.0);
      // Finer sheets of heavier rain within the shaft.
      let sf = textureSampleLevel(t_noise, s_repeat, qc / sc.y + vec3f(0.17, 0.83, 0.52), 0.0);
      let streak = smoothstep(0.25, 0.75, sn.g * 0.45 + sn.b * 0.3 + sf.g * 0.35 - 0.05);
      var core = (1.0 - smoothstep(mix(0.35, 0.55, sup), 1.0, rad + (sn.r - 0.5) * 0.5));
      var heavy = 0.75 * sup;
      var cores = 1.0;
      var vtop = base;
      if (line) {
        let u = us + (sn.r - 0.5) * 0.3 * R;
        // It falls from under the shelf cloud's underside (storm_sd), not
        // through the shelf: from under the base this rain is drawn in front
        // of the cloud, and inside the shelf it veiled the shelf's face.
        let ws = SHELF_W * R;
        let Dn = SHELF_D * shelf_depth(cxl, seed_of(sp));
        // (From a little inside it: the billows wear its underside away, and
        // between that and the rain grazing rays saw the bright sky far
        // behind the line.)
        vtop -= select(0.0, max(Dn * (1.0 - 0.55 * us / ws) - 600.0, 0.0), us < 1.9 * ws);
        let lr = line_rain(u, R) * (1.0 - smoothstep(L2 - 0.3 * R, L2 + 0.7 * R, abs(cxl)));
        heavy = lr.y;
        core = lr.x;
        // Heavier in cores along the line and thinner between them (as one even
        // curtain it stood as a flat grey slab under the shelf).
        cores = mix(0.6, 1.0, smoothstep(0.2, 0.8, 0.5 + 0.4 * sin(cxl / 6000.0 + seed_of(sp) * 13.0) + (sn.b - 0.5) * 0.8));
      }
      let vert = smoothstep(0.0, 250.0, vtop - h) * mix(0.55, 1.0, smoothstep(0.0, base, h) * sn.a + (1.0 - sn.a));
      // Heavy rain: a few km of it is all one sees through.
      // (A shower's shaft is narrow and its rain lighter, but still stands out
      // grey from afar: extinction goes as the square root of the amount.)
      // A young cell's (a shower's) rain falls as one denser, more even shaft.
      // (A squall line's heavy rain falls in denser, more even sheets.)
      let e = 0.0015 * (1.0 + 0.5 * sup) * sqrt(amt) * core * cores * mix(mix(mix(0.5, 0.1, young), mix(0.5, 0.12, f32(line)), heavy) * (1.0 - 0.4 * sup), 1.0, streak * streak) * vert * (1.0 + 2.0 * young);
      if (e <= 1e-7) { continue; }
      let upn = p / length(p);
      let mu = dot(upn, G.sun_dir.xyz);
      if (line) { vis = cloud_sun(p, false).vis; }
      // Under the storm the rain is in deep shade: the storm above takes most of
      // the sky light, and the sun only gets in where the shadow map lets it (low
      // sun under the anvil's edge). So it stands out dark against the sky.
      // Inside it, under the storm, the light is the storm's dull grey.
      // (Its own cell's shade only, as storm_shade has it: looping over every
      // cell per sample cost a few % fps whenever a shaft was in view.)
      let pc = storm_hoff(ph, up, L2) + G.storm.xyz * (0.2 * R);
      // (Even a shower's base hides half the sky from its rain.)
      let dark = max((1.0 - smoothstep(0.7, 1.7, length(pc) / R)) * max(smoothstep(0.3, 0.75, sp.x) * smoothstep(1500.0, 4000.0, R), 0.5), sup);
      // (A squall line's heavy core takes on the green-grey of hail-laden rain.)
      // (A supercell's core, dense with rain and hail, the darkest of all.)
      var amb = storm_light(sky_amb * 0.13 + gnd_amb * 0.3, dark) * (1.0 - 0.45 * sup);
      // A squall line's curtain hangs right behind the gust front, open to the
      // bright sky low ahead: lit from the front, a dull grey-green, paler
      // where it is dense (it sends that light back) and towards the ground,
      // darker deeper in under the storm. (In the storm's shade from above it
      // was a flat blue-black band, darker than the shelf's underside.)
      if (line) { amb = vec3f(luma(sky_amb)) * mix(0.26, 0.07, smoothstep(0.6 * SHELF_W * R, (SHELF_W + 1.4) * R, us)) * mix(1.25, 0.8, smoothstep(0.0, base * 0.8, h)); }
      var S = (sun_trans(h, mu) * G.sun_dir.w * vis * 0.03 + amb) * mix(vec3f(0.88, 0.93, 0.97), vec3f(0.8, 0.94, 0.86), heavy);
      // Lightning lights up the rain around it.
      if (G.stormfx.z > 0.0) { S += flash_light(p - oc) * 0.08; }
      let st = exp(-e * dt);
      o.L += o.T * S * (1.0 - st);
      let w = o.T * (1.0 - st);
      o.t += ts * w;
      wsum += w;
      o.T *= st;
    }
  }
  if (wsum > 0.0) { o.t /= wsum; }
  return o;
}

// ------------------------------------------------------ the rain deck's rain
// Under the rain deck (nimbostratus) rain fills the air from its ragged base to
// the ground: a grey veil (heavy rain cuts the view to a few km) in broad sheets
// drifting with the wind, ending where the deck does. Integrated in a few
// samples, closer together near the camera, along the stretch of the ray under
// the deck's base; col = the dull light in it, about what the base sends down.
const NIMBO_E = 0.0008;  // extinction of the heaviest rain, 1/m
const NIMBO_RAIN_K = 0.9;
fn nimbo_rain(oc: vec3f, rd: vec3f, tmax: f32, jit: f32, col: vec3f) -> Rain {
  var o = Rain(vec3f(0.0), 1.0, 0.0);
  let rg = G.planet.w;
  let top = G.nimbo[1].w - 0.5 * G.nimbo[3].z;
  let sb = ray_sphere(oc, rd, rg + top);
  var t0 = 0.0;
  var t1 = min(tmax, 90000.0);
  if (G.up.w < top) {
    t1 = min(t1, sb.y);
  } else {
    if (sb.x <= 0.0) { return o; }
    t0 = sb.x;
    t1 = min(t1, sb.y);
  }
  if (t1 <= t0) { return o; }
  let ext = NIMBO_E * pow(G.nimbo[0].w, 1.2);
  let len = t1 - t0;
  var tp = t0;
  var wsum = 0.0;
  let n = 6;
  for (var k = 0; k < n; k++) {
    let f = f32(k + 1) / f32(n);
    let tn = t0 + len * f * f;
    let dt = tn - tp;
    let ts = mix(tp, tn, jit);
    tp = tn;
    let p = oc + rd * ts;
    let a = nimbo_area(p);
    if (a <= 0.0) { continue; }
    let r = length(p);
    let h = r - rg;
    let up = p / r;
    // Sheets of heavier and lighter rain: noise squashed flat along the vertical.
    let q = p + G.weather.xyz;
    let qc = q - up * dot(q, up) * 0.93;
    let sn = textureSampleLevel(t_noise, s_repeat, qc / 7000.0 + vec3f(0.31, 0.77, 0.12), 0.0);
    let sheet = mix(0.45, 1.3, smoothstep(0.3, 0.75, sn.g * 0.6 + sn.b * 0.4));
    let e = ext * smoothstep(0.3, 0.8, a) * sheet * smoothstep(top, top - 250.0, h);
    if (e <= 1e-8) { continue; }
    let st = exp(-e * dt);
    o.L += o.T * col * (1.0 - st);
    let w = o.T * (1.0 - st);
    o.t += ts * w;
    wsum += w;
    o.T *= st;
  }
  if (wsum > 0.0) { o.t /= wsum; }
  return o;
}

// dist.x: distance (km) to the clouds this texel sees; dist.y: distance (km) to the
// scene surface the march stopped at, for the depth-aware passes below.
struct CloudOut { @location(0) col: vec4f, @location(1) dist: vec2f };

const SKY_KM = 10000.0;  // scene distance stored for sky texels
// How well two scene distances agree, 1 = same surface.
fn depth_match(a: f32, b: f32) -> f32 {
  let rel = abs(a - b) / max(max(a, b), 1e-6);
  return exp(-rel * rel * 200.0);
}

@fragment fn fs(in: FsIn) -> CloudOut {
  var o = CloudOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.0));
  let div = cloud_pixel_divisor();
  let ray = view_ray(min(in.pos.xy * f32(div) * G.screen.zw, vec2f(1.0)));
  let rd = normalize(ray);
  // Alternate texels march to the nearest and farthest sampled scene depth
  // (reverse Z: larger = nearer), so at a silhouette both surfaces
  // are represented in every 2x2 texel neighbourhood and the upsample can pick the
  // matching one instead of smearing cloud over the plane or clear sky around it.
  let hp = vec2i(in.pos.xy);
  let q = hp * div;
  let edge = vec2i(textureDimensions(t_depth)) - 1;
  let far = min(q + vec2i(div - 1), edge);
  var z = textureLoad(t_depth, q, 0);
  if (div > 1) {
    let z4 = vec4f(z, textureLoad(t_depth, vec2i(far.x, q.y), 0),
                   textureLoad(t_depth, vec2i(q.x, far.y), 0), textureLoad(t_depth, far, 0));
    z = min(min(z4.x, z4.y), min(z4.z, z4.w));
    if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  o.dist.y = min(tscene * 0.001, SKY_KM);
  // No clouds at all (the globe on the start screen): even at zero cover the
  // noise leaves scattered puffs and cirrus veils.
  if (G.start.x > 0.0) { return o; }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  let base = G.cloud.y;
  let alt = G.up.w;
  let sun = G.sun_dir.xyz;
  let cs = dot(rd, sun);
  let jit = fract(ign(in.pos.xy) + G.time.w * 0.618034);
  // A second, independent jitter for the light steps: tied to jit, the light offsets
  // moved in step with the sample positions and left rings of equal distance.
  // (Its pattern moves across the screen from frame to frame: with the same
  // pattern every frame, only shifted in value, the history kept a trace of
  // it, and where the jittered probes hit and missed (storm undersides in
  // shade) it showed as diagonal hatching.)
  let jit2 = ign(in.pos.xy + vec2f(23.0, 61.0) + 5.588238 * (G.time.w % 64.0));

  var T = 1.0;
  var L = vec3f(0.0);
  var tw = 0.0;
  var ws = 0.0;
  var sunw = vec3f(0.0);  // opacity-weighted sunlight and sunward transmittance, for the rim
  var litw = 0.0;
  var cw = 0.0;
  // Whether the last sample was in cloud: the ray then ended in it, not past it.
  var incloud = false;

  // Sky light for the ambient term: zenith and horizon from the sky table, plus
  // sunlight bounced off the ground below.
  let mu0 = dot(G.up.xyz, sun);
  let sky_up = sky_lut(G.up.xyz);
  let sky_side = 0.5 * (sky_lut(normalize(G.east.xyz + G.up.xyz * 0.15)) + sky_lut(normalize(-G.east.xyz + G.up.xyz * 0.15)));
  // Inside a cloud field much of the "sky" is other clouds: pull the ambient towards grey.
  let sky_raw = sky_up * 0.55 + sky_side * 0.45;
  let sky_amb = mix(sky_raw, vec3f(luma(sky_raw)), 0.45);
  var gnd_amb = vec3f(0.11, 0.11, 0.1) * sun_trans(0.0, mu0) * max(mu0, 0.0) * G.sun_dir.w / PI * mix(1.0, 0.45, G.cloud.x);
  // Under a storm the ground below is in its shade: little sunlight comes back up.
  gnd_amb *= 1.0 - 0.9 * G.stormfx.w;

  // Only rays that pass near a storm cell march up to the storm ceiling (and down
  // to the wall and shelf clouds under the base) and look the storms up; the rest
  // keep to the ordinary layer. Storms tower high enough to be seen much further.
  let smask = storm_mask(oc, rd, min(tscene, 320000.0));
  let topa = select(base + G.cloud.z, clamp(g_storm_top, base + G.cloud.z, G.cloud.w), smask != 0u);
  let bot = select(base, base - SHELF_BELOW, smask != 0u);
  let outer = ray_sphere(oc, rd, rg + topa);
  let inner = ray_sphere(oc, rd, rg + bot);
  var ta = 0.0;
  var tb = outer.y;
  var march = outer.y > 0.0;
  if (alt < bot) {
    if (z == 0.0 && ray_sphere(oc, rd, rg).x > 0.0) { march = false; }
    ta = inner.y;
  } else {
    if (alt > topa) { ta = outer.x; }
    if (inner.x > 0.0) { tb = inner.x; }
  }
  tb = min(min(tb, tscene), ta + select(160000.0, 320000.0, smask != 0u));
  // Past the last storm's bound only the ordinary layer is left: rays climbing
  // out of it need not march on up to the storm ceiling for hundreds of km.
  if (smask != 0u) { tb = min(tb, max(ray_sphere(oc, rd, rg + base + G.cloud.z).y, g_storm_end)); }
  if (tb <= ta || ta < 0.0) { march = false; }

  // The mid-level layer is composited where the march passes its middle (in front
  // of or behind the cumulus and storm towers it meets), or after it.
  var ar = vec2f(-1.0);
  var aamt = 0.0;
  if (G.alto.x > 0.0) {
    ar = alto_range(oc, rd);
    ar.y = min(min(ar.y, ar.x + 60000.0), tscene);
    if (ar.x >= 0.0 && ar.x < min(tscene, 250000.0) && ar.y > ar.x) { aamt = alto_amount(oc + rd * (0.5 * (ar.x + ar.y))); }
  }
  var alto_on = aamt > 0.0;
  let atm_ = 0.5 * (ar.x + ar.y);

  // Rain shafts under the storms: in front of the cloud seen from under the base,
  // behind it seen from above.
  var rain = Rain(vec3f(0.0), 1.0, 0.0);
  // The light on the rain deck's top (the same over the whole view, near enough).
  var nim_top = vec3f(0.0);
  if (G.nimbo[0].w > 0.0) { nim_top = nimbo_top(G.up.xyz); }
  if (smask != 0u) { rain = storm_rain(oc, rd, min(tscene, 320000.0), smask, jit, sky_amb, gnd_amb); }
  // The rain deck's rain: lit by what diffuses down through the deck (as its
  // base is, see the march), so the view into the rain fades into the base.
  if (G.nimbo[0].w > 0.0) {
    let ncol = (nimbo_down(nim_top, G.nimbo[1].w) + gnd_amb * 0.3) * NIMBO_RAIN_K;
    let nr = nimbo_rain(oc, rd, min(tscene, 320000.0), jit, ncol);
    if (nr.T < 1.0) {
      let w0 = 1.0 - rain.T;
      let w1 = rain.T * (1.0 - nr.T);
      rain.t = (rain.t * w0 + nr.t * w1) / (w0 + w1);
      rain.L += rain.T * nr.L;
      rain.T *= nr.T;
    }
  }
  // Near storms the rain goes in where the march passes it (its mean
  // distance), in front of or behind the cloud there (in the march, or after
  // it). From under the base all of it went in front, which veiled a squall
  // line's shelf cloud (the curtain hangs behind it) and cut it off in a
  // straight edge. (rain is kept through the march anyway: no extra cost.)
  let rain_front = alt < base;
  if (rain_front && smask == 0u && rain.T < 1.0) {
    L += rain.L;
    tw += rain.t * (1.0 - rain.T);
    ws += 1.0 - rain.T;
    T = rain.T;
  }

  if (march) {
    // Steps grow with distance (a pixel covers more ground further out), and
    // stretch while the ray is in clear air.
    // (the graphics settings scale the steps: longer for speed, shorter for detail)
    let qs = select(clamp(G.quality.x, 0.5, 2.0), 1.0, G.quality.x <= 0.0);
    let n_max = i32(200.0 / qs);
    var t = ta;
    var clear = 0.0;
    // How much further along the ray is sure to be clear of every storm cell's
    // envelope (and its turrets): the storms need not be looked up until then, and
    // no step may be longer.
    var sclear = 0.0;
    // Inside the ordinary layer the storms are looked up further out than
    // that, wherever they push the cumulus aside (Weather.cb): skipping them
    // there let cumulus puffs show through a storm's base. (Out to 2.6 km at
    // least, as far as Weather.cb thins them out: short of that, rays that
    // skipped the lookup and rays that made it drew blocky, dithered patches
    // of cumulus round the storms.)
    var slook = 0.0;
    var ufree = false;
    var pts = ta;        // the previous sample, and whether it was in clear air
    var pempty = true;
    for (var i = 0; i < n_max; i++) {
      // Never run out of steps before the end of the ray: long grazing rays through
      // the layer used to stop short, cutting off the clouds beyond. Late in the
      // budget, steps through clear air stretch to whatever reaches the end (never
      // inside cloud: coarse steps there turned near towers ghostly).
      let dt0 = clamp(t * select(0.016, 0.03, ufree), 40.0, 2500.0) * qs;
      ufree = false;
      var dt = dt0 * (1.0 + clear);
      // Near a storm the stride is held to the air sure to be clear, in whole base
      // steps. (Cut to that distance exactly, the steps ended where a lookup made
      // at the last jittered sample said, so the jitter cancelled out and the
      // march's banding stood on the towers' sunlit faces as contour-line rings.)
      if (smask != 0u) { dt = min(dt, dt0 * max(floor(sclear / dt0), 1.0)); }
      if (i >= n_max * 11 / 20 && clear > 0.0) { dt = max(dt, (tb - t) / f32(max(n_max - 4 - i, 4))); }
      // Never past the end of the ray: a sample short of the scene stands for
      // its whole step. Inside a cloud, rays ending a metre off (the cockpit,
      // the aircraft) took 40 m of cloud wherever the jitter put the sample in
      // front of them: a lattice of dots in the jitter's pattern, sweeping
      // across the screen.
      dt = min(dt, tb - t);
      let ts = t + dt * jit;
      t += dt;
      sclear -= dt;
      slook -= dt;
      if (alto_on && ts > atm_) {
        alto_on = false;
        let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
        L += T * la.rgb;
        let wa = T * (1.0 - la.a);
        tw += atm_ * wa;
        ws += wa;
        T *= la.a;
        if (T < 0.004) { T = 0.0; break; }
      }
      if (smask != 0u && rain.T < 1.0 && ts > rain.t) {
        L += T * rain.L;
        let wr = T * (1.0 - rain.T);
        tw += rain.t * wr;
        ws += wr;
        T *= rain.T;
        rain.T = 1.0;
        if (T < 0.004) { T = 0.0; break; }
      }
      if (ts >= tb) { break; }
      let p = oc + rd * ts;
      let r = length(p);
      let h = r - rg;
      let look = sclear <= 0.0 || (slook <= 0.0 && h >= base && h < base + G.cloud.z);
      let wx = cloud_weather_m(p, select(0u, smask, look));
      // The close-up erosion octave only pays off within a few km.
      let fine = 1.0 - smoothstep(1500.0, 4500.0, ts);
      g_erode = 1.0 - smoothstep(12000.0, 45000.0, ts);
      let d = cloud_shape_f(p, h, wx, true, fine);
      // (The lookups below overwrite it; the envelope's distance packed in, see
      // cloud_shape_f. Far from any storm, none.)
      let anvp = select(255.0, g_anvil, wx.sc.w > 0.0);
      // Clear-air strides used to step right over distant towers a few km wide
      // (they came out ghostly, half the samples missing them): near a storm cell
      // its envelope's distance bounds the stride, far from one it lets the ray
      // skip ahead safely.
      if (look && smask != 0u) {
        sclear = max(-wx.sd - storm_reach(wx.sc, h - base), 0.0) * 0.8;
        slook = max(-wx.sd - max(storm_bulge(wx.sc), 2600.0), 0.0) * 0.8;
      }
      // Stride faster through the empty storm-only air above the ordinary layer.
      if (d <= 0.002) { incloud = false; pempty = true; pts = ts; clear = min(clear + 0.2, select(1.0, 3.0, h > base + G.cloud.z && wx.cb <= 0.0)); if (h > base + G.cloud.z && smask != 0u) { clear = max(clear, floor(min(sclear, h - base - G.cloud.z + dt0) / dt0) - 1.0); }
        // Under a storm's base the clear air between the scud takes longer
        // steps (grazing rays cross kilometres of it).
        ufree = h < base && smask != 0u;
        continue; }
      clear = 0.0;
      incloud = true;
      // Light the cloud near where the ray went in, not where the (long, far
      // off) step happened to land: a sample hundreds of metres deep sees the sun
      // through that much cloud, which turned distant sunlit towers dull grey. A
      // few halvings between the last clear sample and this one find the surface.
      var pl = p;
      var hl = h;
      // The sample stands for its whole step, but after a long clear stride only
      // the stretch from the surface on is cloud: weighting it by the full stride
      // made the opacity jump with the stride length.
      // Refined whenever the step is longer than the 40 m the light is taken at:
      // switching on only past 150 m steps (~9 km) lit nearer and farther parts of
      // a tower differently, a seam that drew a box on its face and contour lines
      // where the stride count changed.
      var dl = dt;
      if (pempty && dt > 40.0) {
        var lo = pts;
        var hi = ts;
        // Far off the strides are km long: two more halvings, or the light is taken
        // a few hundred metres inside and distant towers go grey.
        // The surface found is that of the cloud as sampled (eroded, with the
        // close-up octave): the smooth, uneroded one lies further out, so the
        // light was taken in the clear air between the billows and they came out
        // flat. (One halving fewer pays for the eroded lookups; the jitter
        // averages out what is left.)
        for (var k = 0; k < select(3, 5, dt > 600.0); k++) {
          let tm = 0.5 * (lo + hi);
          let pm = oc + rd * tm;
          if (cloud_shape_f(pm, length(pm) - rg, wx, true, fine) > 0.002) { hi = tm; } else { lo = tm; }
        }
        pl = oc + rd * min(hi + 40.0, ts);
        hl = length(pl) - rg;
        dl = clamp(t - hi, 0.5 * (t - ts), dt);
      }
      pempty = false;
      pts = ts;
      let upn = pl / length(pl);
      let ctop = column_top(wx);
      // Optical depth towards the sun; the column's weather is reused along the way.
      // Tall columns take one more, long step (out to ~3 km) so towers shade their neighbours.
      var od = 0.0;
      // Far off (a pixel spans hundreds of metres) fewer, longer steps reach as far.
      let farl = ts > 30000.0;
      var ls = select(36.0, 90.0, farl);
      var lt = 0.0;
      // (A squall line's shelf and ceiling, under its base, take their sun
      // from the shadow map: a few short steps for their own cloud will do.)
      let nl = select(select(select(6, 7, ctop > 2500.0), 3, hl < base && wx.sp.w >= 1.0), 4, farl);
      let lg = select(1.85, 2.5, farl);
      // Storm towers: the light steps take the full jitter (TAA averages it), or
      // where they cross the anvil's flat underside the shadow steps into
      // horizontal bands down the tower.
      let jl = select(0.5 + 0.4 * (jit - 0.5), jit2, wx.cb > 0.0);
      for (var j = 0; j < nl; j++) {
        let lp = pl + sun * (lt + ls * jl);
        od += cloud_shape(lp, length(lp) - rg, wx, j < 2 && !farl) * ls;
        lt += ls;
        ls *= lg;
      }
      od *= SIGMA;
      // (After the light march, like everything below: any value carried
      // through it cost ~18% of the cloud pass in every view.)
      let sdq0 = floor(anvp);
      let anv = (anvp - sdq0) / 0.99;
      // (A supercell's rotating updraft: flagged +256, cloud_shape_f.)
      let meso = sdq0 >= 256.0 && sdq0 < 512.0;
      let sdq = sdq0 - select(0.0, 256.0, meso);
      // How much of the cloud here is the rain deck's, not a storm tower's
      // standing in it (lit with the deck's flat glow, the tower's lower half
      // came out as one pale drum up to the layer's top): not at the tower's
      // face, nor above the deck's top, raised where it swells up against it.
      var nsw = wx.ns;
      var ntop = G.nimbo[2].w;
      if (nsw > 0.0 && sdq < 255.0) {
        let sdp = -16.0 * sdq;
        ntop += deck_swell(sdp, wx);
        nsw *= 1.0 - max(smoothstep(-600.0, 0.0, sdp), smoothstep(ntop - 150.0, ntop + 150.0, hl));
      }
      let hn = clamp((hl - base - (1.0 - nsw) * G.nimbo[3].y) / column_top(wx), 0.0, 1.0);
      var occ = 1.0;
      // A squall line: u metres back from its shelf cloud's nose (storm_sd).
      let lsq = wx.sp.w >= 1.0;
      let Rq = wx.sc.w * 1000.0;
      let sseed = fract(wx.sp.w);
      var cxl = 0.0;
      var u = 1e9;
      if (lsq) {
        let dvq = (upn * 6371.0 + G.weather.xyz * 0.001 - wx.sc.xyz) * 1000.0;
        cxl = dot(dvq, normalize(cross(upn, G.storm.xyz)));
        u = Rq * (SHELF_F + 0.12 * sin(cxl / 9000.0 + sseed * 40.0) + 0.07 * sin(cxl / 3700.0 + sseed * 17.0)) - dot(dvq, G.storm.xyz);
      }
      // The light march climbs a km or two towards the sun, far short of all
      // the cloud it may have to get through near a storm (the tower, the
      // anvil overhead): the shadow map holds that, for what the march does
      // not reach. Under the base (a storm's wall or shelf cloud, its scud):
      // without it a squall line's shelf caught the sun in full and stood out
      // as a white stripe. Above it: the cumulus thinned out around a storm
      // and a squall line's towers' feet, in front of them under the anvil,
      // are in its shade (lit by the march alone, they stood in full sun
      // under it, bright specks and blobs on the dark storm base; cumulus
      // further off are left as they were: looking the map up for all of
      // them cost ~8% wherever a storm was in view). Not the towers
      // themselves: the march sees their own cloud, and an anvil's shade
      // on them turned a line's sunlit rampart into one flat dark wall.
      // (Above the base the map's first 2 km towards the sun are left to the
      // march: counted twice, cumulus near storms came out darker than the
      // rest.)
      // (The cumulus take only what lies above the layer, the anvil and the
      // towers' heads: the map's coarse texels shading whole clouds by others
      // kilometres away left them in dark, brownish blotches.)
      var vw = select(0.0, 1.0, hl < base && wx.cb > 0.0);
      var lift = 0.0;
      if (smask != 0u && hl >= base) {
        vw = select(0.0, 1.0 - smoothstep(0.15, 0.4, wx.cb), wx.cb > 0.0);
        lift = max(base + G.cloud.z - hl, 0.0) / max(dot(upn, sun), 0.1);
        if (lsq) {
          let vl = smoothstep(0.7 * Rq, 0.3 * Rq, u) * (1.0 - smoothstep(base + 800.0, base + 1300.0, hl));
          if (vl > vw) { vw = vl; lift = 2000.0; }
        }
        // A supercell's rotating updraft stands under the storm's head and
        // anvil: in their shade unless the sun is low enough to get in under
        // them (the march alone lit it in full).
        // (In full, where the sun got in under the anvil it lit a window on
        // the dark drum: the march's own light keeps a share.)
        if (meso) { vw = 0.8; }
      }
      var vsh = 1.0;
      // (Smoothly filtered for the rotating updraft: its smooth plates showed
      // the map's texels as teeth along the shadow's edge.)
      if (vw > 0.0) { vsh = mix(1.0, cloud_sun(pl + sun * lift, meso).vis, vw); }
      if (lsq && hl < base) {
        // A squall line's shelf cloud and the ceiling over it: what sky they
        // see follows from where on them the light is taken, no probes (the
        // ceiling over every sample made the probes' jittered heights hit or
        // miss, hatching the underside). The shelf's upper face looks out at
        // the bright sky low ahead, palest at its leading edge, the risers
        // of its laminar tiers brighter than the lips over them; its
        // underside and the ceiling look down at the shaded ground, a little
        // lighter where lumps hang down and darker in the hollows between.
        let lobe = shelf_lobe(cxl, sseed);
        let ub = u + shelf_bulge(cxl, sseed, Rq);
        let xc = clamp(ub / (SHELF_FACE * Rq), 0.0, 1.0);
        let f = fract(min(xc * 3.0, 2.999));
        let Ds = SHELF_D * shelf_depth(cxl, sseed);
        let ztop = shelf_top(ub, Rq, Ds);
        let z = hl - base;
        let dz = ztop - z;
        let front = smoothstep(-300.0, -60.0, ub) * (1.0 - smoothstep(120.0, 380.0, ub)) * smoothstep(-Ds + 20.0, -Ds + 140.0, z) * (1.0 - smoothstep(-Ds * (1.0 - SHELF_NOSE), -Ds * (1.0 - SHELF_NOSE) + 120.0, z));
        let face = max(smoothstep(-90.0, 0.0, dz) * (1.0 - smoothstep(60.0, 320.0, dz)), front) * (1.0 - smoothstep(-SHELF_CEIL * 1.1, -SHELF_CEIL * 0.4, z));
        // (Each riser darker at its foot, under the lip of the one in front,
        // than at its own rounded lip: stacked bands. Its blunt front palest.)
        let lip = smoothstep(0.45, 0.9, f);
        // (The lobes bulging out face more of the open sky than the cusps
        // tucked back between them.)
        let fo = 2.9 * mix(0.45, 1.2, lip) * mix(0.75, 1.15, lobe) * mix(1.3, 0.75, sqrt(xc)) * mix(1.35, 1.0, smoothstep(0.0, 0.08, xc));
        let uo = 0.24 * mix(0.6, 1.5, smoothstep(-SHELF_CEIL * 0.9, -SHELF_CEIL * 1.7, z));
        // (Seen from above, the ceiling's top looks up at the sky.)
        occ = mix(mix(1.0, uo, smoothstep(-0.3, 0.1, dot(rd, upn))) * mix(0.45, 1.0, vsh), fo, face);
      } else {
        // Cloud overhead blocks the sky light: a probe straight up, diffusion falloff.
        // (Near storms at a jittered height, averaged over frames: at one height
        // the probe was either in a billow or out of it, which left flat dark
        // blotches with hard edges all over the towers.)
        let hup = select(450.0, 150.0 + 1150.0 * fract(jit2 + 0.5), wx.cb > 0.0);
        let od_up = cloud_shape(pl + upn * hup, hl + hup, wx, false) * 900.0 * SIGMA;
        occ = mix(1.0 / (1.0 + 0.15 * od_up), 1.0, 0.12);
        // Under a storm's anvil (km overhead) the sky is mostly cloud: a second,
        // long probe greys and darkens the tower's shaded flanks, so they read as
        // massive cloud instead of fading into the sky behind.
        if (wx.cb > 0.0) {
          let hu2 = 2500.0 * (0.6 + 0.8 * jit2);
          let od2 = cloud_shape(pl + upn * hu2, hl + hu2, wx, false) * 3000.0 * SIGMA;
          occ *= mix(1.0 / (1.0 + 0.06 * od2), 1.0, 0.3);
        }
        // Where the storm hides the sun it hides much of the sky as well, and
        // then the cloud right overhead takes little more of it: the probe's
        // say is cut down. (In full, its jittered heights hit and missed the
        // cumulus overhead, which, with no sun on them to drown that, stood
        // out as pale blobs hatched with the jitter's diagonal pattern.)
        occ = mix(occ, 0.35, 0.75 * (1.0 - vsh)) * mix(0.45, 1.0, vsh);
      }
      // The rain deck is kilometres deep, and the light march stops ~1.5 km in:
      // its base came out as bright as a thin stratus's. Light gets down by
      // diffusing through all of the deck above (from its top, raised where it
      // swells up against a storm tower: from its mean top the swell came out
      // lit like thin cloud), and the sky above it is hidden.
      if (nsw > 0.0) {
        let tau = SIGMA * 0.55 * max(ntop - hl, 0.0) * nsw;
        od = max(od, tau / max(dot(upn, sun), 0.3));
        occ = min(occ, 0.1 + 0.9 / (1.0 + 0.1 * tau));
      }
      // Multiple scattering as a sum of ever dimmer, softer, more isotropic octaves.
      var ms = 0.0;
      var a = 1.0;
      var b = 1.0;
      var c = 1.0;
      for (var k = 0; k < 4; k++) {
        ms += a * exp(-od * b) * cloud_phase(cs, c);
        a *= 0.55;
        b *= 0.38;
        c *= 0.5;
      }
      // Deep inside, light arrives by diffusion, which falls off like 1/depth rather
      // than exponentially (two-stream approximation, g ~ 0.85).
      ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
      // (A storm's light march stops a few km in, far short of all the cloud
      // between: its glow toward the sun would light up the dark base.)
      ms += cloud_glow(cs, od * (1.0 + 4.0 * wx.cb));
      // In-scatter probability: wispy edges and the undersides of clouds collect
      // less multiply-scattered light, which gives the dark creases between puffs.
      let dprob = 0.08 + pow(d, remap(clamp(hn, 0.3, 0.85), 0.3, 0.85, 0.5, 2.0));
      let vprob = pow(clamp(remap(hn, 0.05, 0.2, 0.25, 1.0), 0.25, 1.0), 0.8);
      // Looking towards the sun thin edges glow instead (forward scattering): no powder.
      let powder = mix(1.0, clamp(dprob * vprob * 1.25, 0.0, 1.0), 0.55 * (1.0 - smoothstep(0.0, 0.6, cs)));
      let mu = dot(upn, sun);
      var sunc = sun_trans(hl, mu) * G.sun_dir.w;
      sunc *= vsh;
      // The anvil is a sheet high above the ground and open to the sky at its edges:
      // its underside still sees the sky around and the sunlit ground and cloud
      // below (taken by height in the column it got the tower top's deep shade and
      // read as a dark plate on a stem).
      occ = mix(occ, max(occ, 0.6), anv);
      // (The ground under a storm is in its shade: its base gets little bounce.)
      // The towns' light from below (not dimmed by the storm's shade, as the
      // sunlit ground's bounce is).
      var cg = vec3f(0.0);
      if (glow_on()) { cg = city_amb(p); }
      var amb = (sky_amb * mix(0.25, 0.95, hn) * occ + gnd_amb * mix(1.0, 0.25, hn * (1.0 - 0.7 * anv)) * (1.0 - 0.7 * wx.cb) + cg * (mix(1.0, 0.25, hn * (1.0 - 0.7 * anv)) + (1.0 - hn) * 0.5)) * mix(0.5, 1.0, powder);
      let moonc = moon_light(upn, hl);
      var S = ((sunc + moonc) * ms * MS_GAIN * powder + amb) * d * SIGMA;
      // Deep in the rain deck (and on its base) the light is what diffuses down
      // through it, a little lighter and darker where the deck is thinner and thicker.
      // The ragged scud hanging below it is darker (dense and in its shadow).
      if (nsw > 0.0) {
        let scud = 1.0 - smoothstep(0.0, G.nimbo[3].z * 1.3, hl - G.nimbo[1].w + G.nimbo[3].z);
        S = max(S, nimbo_down(nim_top, hl) * (mix(0.75, 1.2, wx.cell) * mix(0.55, 1.0, powder) * mix(1.0, 0.5, scud) * nsw * d * SIGMA));
      }
      // Lightning: the cloud lit from inside (diffuse, so an even phase). Per
      // sample: added once per ray at the cloud's mean distance it lit the whole
      // tower as one flat veil.
      if (G.stormfx.z > 0.0) { S += flash_cloud(pl - oc) * (0.1 * d * SIGMA); }
      let e = d * SIGMA;
      let st = exp(-e * dl);
      L += T * S * (1.0 - st) / e;
      let wgt = T * (1.0 - st);
      sunw += wgt * sunc;
      litw += wgt * exp(-od);
      cw += wgt;
      tw += ts * wgt;
      ws += wgt;
      T *= st;
      if (T < 0.004) { T = 0.0; break; }
    }
    // Silver lining. Looking towards the sun the sunward path from a sample runs on
    // along the line of sight, so only where the whole line of sight through the cloud
    // is thin does sunlight get through: a band at the outline metres wide, far finer
    // than the march resolves. Its partly transparent edge pixels stand in for it:
    // single scattering through a slab of optical depth tau = -ln T is tau e^-tau.
    // Only the share the march did not already light (litw) is added.
    // (Not through cloud the march found opaque: T was zeroed, and the floor's
    // tau e^-tau times the sharp lobe lit a disc round the sun on a storm's base.)
    // (Nor where the ray ends on something inside the cloud, the cockpit or
    // the aircraft flying through it: there is no sunlit far side, and the
    // metre of cloud in front of every pixel lit a glare round the sun over
    // the cabin, the march itself finding the sun hidden.)
    let cut = incloud && tb >= tscene;
    if (cw > 0.0 && cs > 0.5 && T > 0.0 && !cut) {
      let tau = -log(max(T, 1e-4));
      let lit = litw / cw;
      L += (sunw / cw) * (RIM * hg(cs, 0.94) * tau * exp(-tau) * (1.0 - lit));
    }
  }
  if (alto_on && T > 0.004) {
    let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
    L += T * la.rgb;
    let wa = T * (1.0 - la.a);
    tw += atm_ * wa;
    ws += wa;
    T *= la.a;
  }
  if ((!rain_front || smask != 0u) && rain.T < 1.0) {
    L += T * rain.L;
    let wr = T * (1.0 - rain.T);
    tw += rain.t * wr;
    ws += wr;
    T *= rain.T;
  }
  // Distant clouds sink into the haze.
  if (ws > 0.0) {
    let tc = tw / ws;
    o.dist.x = tc * 0.001;
    let top = ray_sphere(oc, rd, rg + ATM_H);
    let sc = atm_march(oc, rd, max(top.x, 0.0), tc, 6, jit);
    L = L * sc.T + sc.L * (1.0 - T);
  }

  // Cirrus sheet, behind the cumulus layer when seen from below.
  if (T > 0.01) {
    let ci = ray_sphere(oc, rd, rg + CIRRUS_H);
    var tci = -1.0;
    if (alt < CIRRUS_H) { tci = ci.y; } else if (ci.x > 0.0 && ray_sphere(oc, rd, rg).x < 0.0) { tci = ci.x; }
    if (tci > 0.0 && tci < tscene && tci < 400000.0) {
      let p = oc + rd * tci;
      let r = length(p);
      let upn = p / r;
      let slant = 1.0 / max(abs(dot(rd, upn)), 0.08);
      let tau = cirrus(p) * slant * 0.35;
      if (tau > 0.001) {
        let mu = dot(upn, sun);
        let sunc = sun_trans(CIRRUS_H, mu) * G.sun_dir.w;
        let ph = mix(hg(cs, 0.75), hg(cs, -0.1), 0.35) + 0.04;
        var Lc = ((sunc + moon_light(upn, CIRRUS_H)) * ph + sky_up * 0.5 + gnd_amb * 0.3) * (1.0 - exp(-tau)) * 0.95;
        let ac = 1.0 - exp(-tau);
        let top = ray_sphere(oc, rd, rg + ATM_H);
        let sc = atm_march(oc, rd, max(top.x, 0.0), tci, 4, jit);
        Lc = Lc * sc.T + sc.L * ac;
        L += T * Lc;
        if (ws <= 0.0) { o.dist.x = tci * 0.001; }
        T *= 1.0 - ac;
      }
    }
  }
  o.col = vec4f(L, T);
  return o;
}

// Temporal accumulation: blend this frame's noisy march into the reprojected
// history, clamped to the current neighbourhood so moving clouds do not smear.
@group(1) @binding(2) var t_cur: texture_2d<f32>;
@group(1) @binding(3) var t_cdist: texture_2d<f32>;
@group(1) @binding(4) var t_hist: texture_2d<f32>;
@fragment fn fs_temporal(in: FsIn) -> @location(0) vec4f {
  let dims = vec2i(textureDimensions(t_cur));
  let px = vec2i(in.pos.xy);
  let cur = textureLoad(t_cur, px, 0);
  let cd = textureLoad(t_cdist, px, 0).rg;
  // Neighbourhood statistics over texels on the same surface only: across a
  // silhouette the clear texels in front would otherwise widen the clamp and let
  // the history bleed cloud onto the plane (and clear sky into the cloud).
  // Behind cloud this texel sees as (nearly) opaque, surfaces well beyond it
  // count as one: at the horizon the texels marching to the ground and those
  // marching to the sky see the same cloud, but each got half a neighbourhood,
  // a tighter clamp and less smoothing, which drew a dotted seam right across
  // storm towers. (Not through thin cloud: what lies behind it shows.)
  let cap = select(1e9, cd.x * 3.0, cd.x > 0.0 && cur.a < 0.15);
  var m1 = vec4f(0.0);
  var m2 = vec4f(0.0);
  var n = 0.0;
  var mn = cur;
  var mx = cur;
  for (var y = -1; y <= 1; y++) {
    for (var x = -1; x <= 1; x++) {
      let c = clamp(px + vec2i(x, y), vec2i(0), dims - 1);
      if (depth_match(min(textureLoad(t_cdist, c, 0).g, cap), min(cd.y, cap)) < 0.5) { continue; }
      let s = textureLoad(t_cur, c, 0);
      m1 += s;
      m2 += s * s;
      n += 1.0;
      mn = min(mn, s);
      mx = max(mx, s);
    }
  }
  let mean = m1 / n;
  let sd = sqrt(max(m2 / n - mean * mean, vec4f(0.0)));
  let lo = max(mn, mean - sd * 1.6);
  let hi = min(mx, mean + sd * 1.6);
  if (G.cam_delta.w < 0.5) { return cur; }
  // Reproject through the distance of the cloud this pixel sees.
  var dist = cd.x * 1000.0;
  if (dist <= 0.0) { dist = 60000.0; }
  let P = normalize(view_ray(min(in.pos.xy * f32(cloud_pixel_divisor()) * G.screen.zw, vec2f(1.0)))) * dist;
  let pc = G.prev_vp * vec4f(P + G.cam_delta.xyz, 1.0);
  if (pc.w <= 0.0) { return cur; }
  let puv = pc.xy / pc.w * vec2f(0.5, -0.5) + 0.5;
  if (any(puv < vec2f(0.0)) || any(puv > vec2f(1.0))) { return cur; }
  let hist = clamp(textureSampleLevel(t_hist, s_clamp, puv, 0.0), lo, hi);
  return mix(hist, cur, 0.1);
}

// Upsample onto the scene: out = cloud + scene * transmittance (done by the blend state).
// Joint bilateral: the four nearest texels are weighted by bilinear position and by
// how well the scene distance they marched to matches the wanted one, falling back to
// the best-matching texel when none agrees.
@group(1) @binding(1) var t_cloud: texture_2d<f32>;
@group(1) @binding(5) var t_color: texture_2d<f32>;
fn cloud_at(pos: vec2f, d: f32) -> vec4f {
  let dims = vec2i(textureDimensions(t_cloud));
  let div = cloud_pixel_divisor();
  if (div == 1) {
    let pixel = clamp(vec2i(pos), vec2i(0), dims - 1);
    if (depth_match(textureLoad(t_cdist, pixel, 0).g, d) > 0.995) {
      return textureLoad(t_cloud, pixel, 0);
    }
  }
  let f = pos / f32(div) - 0.5;
  let b = vec2i(floor(f));
  let fr = f - floor(f);
  var acc = vec4f(0.0);
  var wsum = 0.0;
  var best = vec4f(0.0, 0.0, 0.0, 1.0);
  var bestw = -1.0;
  for (var j = 0; j < 2; j++) {
    for (var i = 0; i < 2; i++) {
      let c = clamp(b + vec2i(i, j), vec2i(0), dims - 1);
      let s = textureLoad(t_cloud, c, 0);
      let m = depth_match(textureLoad(t_cdist, c, 0).g, d);
      let bw = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
      if (m > bestw) { bestw = m; best = s; }
      acc += s * (bw * m);
      wsum += bw * m;
    }
  }
  return (acc + best * 0.01) / (wsum + 0.01);
}
fn reveal_clouds(cloud: vec4f) -> vec4f {
  let visible = clamp(1.0 + G.start.x, 0.0, 1.0);
  return mix(vec4f(0.0, 0.0, 0.0, 1.0), cloud, visible);
}

@fragment fn fs_apply(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  // The composite filled the part of an anti-aliased edge that geometry does not
  // cover (1 - alpha) with sky; that part needs the clouds in front of the sky,
  // the covered part those in front of the geometry.
  let cov = textureLoad(t_color, px, 0).a;
  if (z == 0.0 || cov < 0.004) { return reveal_clouds(cloud_at(in.pos.xy, SKY_KM)); }
  let ray = view_ray(in.uv);
  let cg = cloud_at(in.pos.xy, min(length(ray / z) * 0.001, SKY_KM));
  if (cov > 0.996) { return reveal_clouds(cg); }
  let cs = cloud_at(in.pos.xy, SKY_KM);
  // Same sky (and jitter) the composite blended in. With dst = cov*G + (1-cov)*S,
  // dst*cg.a + L = cov*(G*cg.a + cg.rgb) + (1-cov)*(S*cs.a + cs.rgb).
  let S = clamp(sky_color(normalize(ray), ign(in.pos.xy)), vec3f(0.0), vec3f(3000.0));
  return reveal_clouds(vec4f(cov * cg.rgb + (1.0 - cov) * (cs.rgb + S * (cs.a - cg.a)), cg.a));
}

// Cloud shadow map (render.rs CloudShadow, read by cloud_sun): per texel, a ray along
// the sun from a point on the cloud base, with the optical depth it crosses in three
// height bands (cumulative) and how closed the deck is there. The map being drawn
// is G.cs[6..12] (see cloud_sun).
fn cs_band(h: f32) -> vec3f {
  let f = sqrt(clamp((h - G.cs[9].x) / (G.cs[9].y - G.cs[9].x), 0.0, 1.0)) * 3.0;
  return select(select(vec3f(0.0, 0.0, 1.0), vec3f(0.0, 1.0, 0.0), f < 2.0), vec3f(1.0, 0.0, 0.0), f < 1.0);
}
fn cshadow_ray(px: vec2f, ext: f32) -> vec4f {
  // Shadows of the clouds as drawn (eroded), not of their whole envelopes.
  g_mean_erode = true;
  let base = G.cs[9].x;
  let rg = G.planet.w;
  let R = rg + base;
  let loc = (px / CS_RES * 2.0 - 1.0) * ext + G.cs[9].zw;
  let qn = normalize(G.cs[6].xyz + (G.cs[7].xyz * loc.x + G.cs[8].xyz * loc.y) / R);
  // The clouds as they were when this map was begun: its later slices are drawn
  // while the clouds drift on, and the lookups make up for the drift since.
  let dq = G.cs[11].xyz - G.weather.xyz;
  let q = qn * R + dq;
  let sun = G.cs[10].xyz;
  var closed = smoothstep(0.45, 0.95, cloud_weather_m(q, 0u).cov);
  if (dot(qn, sun) <= 0.0) { return vec4f(0.0, 0.0, 0.0, closed); }
  var b = vec3f(0.0);
  // Only rays that pass near a storm cell climb to the storm ceiling.
  let smask = storm_mask(q, sun, 120000.0);
  let lid = select(base + G.cloud.z, clamp(g_storm_top, base + G.cloud.z, G.cloud.w), smask != 0u);
  let tl = min(ray_sphere(q, sun, rg + lid).y, 120000.0);
  // Long slanting rays (a low sun) take more steps, so the puffs they cross are not
  // skipped over. A per-texel offset along the ray turns the steps' banding into
  // fine noise, which the lookup's smooth filter averages away; it is tied to the
  // texel, which stays put on the ground from map to map, so it does not shimmer.
  let n = i32(clamp(tl / 300.0, 32.0, 56.0));
  let nf = f32(n);
  let jit = ign(px);
  // Steps packed towards the base, t = tl u (u + K) / (1 + K): fair-weather puffs
  // are only 60-150 m thick there, and evenly spaced steps (60-90 m apart in
  // height) hit or missed them by the jitter alone, so neighbouring texels
  // alternated dark and light and the shadows showed as a grid of dots. Now ~6x
  // finer at the base, under 2x coarser at the top (towers, km across), the
  // same step count; it follows the bands' squeeze towards the base too.
  const K = 0.2;
  let ts = tl / (1.0 + K);
  // The column weather changes slowly along the ray: look it up every other step.
  var wx = no_weather();
  for (var i = 0; i < n; i++) {
    let u = (f32(i) + jit) / nf;
    let p = q + sun * (ts * u * (u + K));
    let h = length(p) - rg;
    if ((i & 1) == 0) {
      let uw = (f32(i) + 1.0) / nf;
      wx = cloud_weather_m(q + sun * (ts * uw * (uw + K)), smask);
    }
    let d = cloud_shape(p, h, wx, false);
    if (d > 0.0) { b += cs_band(h) * (d * ts * (2.0 * u + K) / nf); }
  }
  // The mid-level layer (which drifts half as fast again): one crossing of its lens.
  if (G.alto.x > 0.0) {
    let pa = q + sun * ray_sphere(q, sun, rg + G.alto.y).y + dq * 0.5;
    let aa = alto_amount(pa);
    if (aa > 0.0) {
      let ln = alto_lens(pa, aa, 0.0);
      b += cs_band(G.alto.y) * (ln.z * (ln.y - ln.x) / max(dot(normalize(pa), sun), 0.08));
      closed = max(closed, ln.w * smoothstep(0.0, 300.0, ln.y - ln.x) * 0.85);
    }
  }
  b *= SIGMA;
  return vec4f(b.x, b.x + b.y, b.x + b.y + b.z, closed);
}
@fragment fn fs_cshadow_in(in: FsIn) -> @location(0) vec4f { return cshadow_ray(in.pos.xy, G.cs[6].w); }
@fragment fn fs_cshadow_out(in: FsIn) -> @location(0) vec4f { return cshadow_ray(in.pos.xy, G.cs[7].w); }

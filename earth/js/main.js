// App shell: scene, UI state, data refresh scheduling and the render loop.
// Heavy lifting (parsing, SGP4, interpolation, particle physics, procedural fallbacks) is in earth.wasm.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/OrbitControls.js';
import { loadWasm, julian } from './wasm.js';
import { createEngine } from './orbit-engine.js';
import { settings, saveSettings, detectBackend, hasBackend, sources, loadImage, connectStream } from './data.js';
import * as L from './layers.js';
import { t, applyStatic, setLang, getLang, onLangChange } from './i18n.js';

const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];
const MOBILE = matchMedia('(pointer: coarse)').matches || innerWidth < 760;
const GOTHENBURG = { lat: 57.7089, lon: 11.9746, label: 'Göteborg, Västra Götaland, Sweden' };
const MIN = 60e3, HOUR = 3600e3;

// ------------------------------------------------------------------ status helpers
// `text` may be a function so the chip can be re-rendered when the language changes.
const statusMem = {};
function status(key, state, text = '') {
  statusMem[key] = [state, text];
  const el = $(`[data-status="${key}"]`);
  if (!el) return;
  const s = typeof text === 'function' ? text() : text;
  el.className = `st ${state}`;
  el.textContent = s;
  el.title = s;
}
const detail = (k) => { const v = state.details[k]; return typeof v === 'function' ? v() : v; };
const fail = (e) => (e?.message || String(e)).slice(0, 60);

// ------------------------------------------------------------------ renderer & scene
const canvas = $('#scene');
const renderer = new THREE.WebGLRenderer({ canvas, antialias: !MOBILE, powerPreference: 'high-performance' });
renderer.setPixelRatio(Math.min(devicePixelRatio, MOBILE ? 1.5 : 2));
renderer.outputColorSpace = THREE.LinearSRGBColorSpace; // textures are sampled raw (sRGB in, sRGB out)
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 2000);
const controls = new OrbitControls(camera, canvas);
controls.enableDamping = true;
controls.dampingFactor = 0.06;
controls.minDistance = 1.15;
controls.maxDistance = 60;
controls.rotateSpeed = 0.5;
controls.zoomSpeed = 0.8;

const texLoader = new THREE.TextureLoader();
const tex = (url) => new Promise((res, rej) => texLoader.load(url, (t) => { t.anisotropy = 4; res(t); }, undefined, rej));
const texFirst = async (...urls) => { for (const u of urls) { try { return await tex(u); } catch { /* next */ } } throw new Error('texture'); };

// ------------------------------------------------------------------ app state
const state = {
  w: null, engine: null, ref: Date.now(),
  clock: { simBase: Date.now(), absBase: performance.timeOrigin + performance.now(), speed: 1 },
  layers: {}, visible: {}, sel: -1, selNeo: -1, lodCap: MOBILE ? 15000 : 120000,
  neoMeta: [], counts: [0, 0, 0, 0, 0, 0], details: {}, loc: null,
};
const absNow = () => performance.timeOrigin + performance.now();
const simMs = () => state.clock.simBase + (absNow() - state.clock.absBase) * state.clock.speed;

function setClock(simBase, speed) {
  state.clock = { simBase, absBase: absNow(), speed };
  state.engine?.setClock(state.clock);
  state.engine?.tick(true);
}

// ------------------------------------------------------------------ boot
async function boot() {
  const bootMsg = $('#bootMsg');
  const [w, backend] = await Promise.all([loadWasm(), detectBackend()]);
  state.w = w;
  if (!settings.debris) settings.debris = MOBILE ? 9000 : 30000;

  bootMsg.textContent = t('bootTextures');
  const [day, night, bump, spec, clouds, stars] = await Promise.all([
    texFirst('textures/8k_earth_daymap.jpg', 'textures/earth-blue-marble.jpg'),
    texFirst('textures/8k_earth_nightmap.jpg', 'textures/earth-night.jpg'),
    tex('textures/earth-topology.png'),
    tex('textures/earth-water.png'),
    texFirst('textures/8k_earth_clouds.jpg', 'textures/earth-clouds.png'),
    tex('textures/night-sky.png').catch(() => null),
  ]);
  for (const t of [bump, spec]) t.wrapS = THREE.RepeatWrapping;
  if (stars) scene.background = stars, stars.mapping = THREE.EquirectangularReflectionMapping;

  const Ly = state.layers;
  Ly.earth = L.makeEarth({ day, night, bump, spec });
  Ly.atmo = L.makeAtmosphere();
  Ly.clouds = L.makeClouds(clouds);
  Ly.clouds.userData.staticTex = clouds;
  Ly.sst = L.makeOverlay(1.0015, null, { waterMask: spec, pulse: 0.35, opacity: 0.85 });
  Ly.air = L.makeOverlay(1.004, null, { opacity: 1 });
  Ly.dust = L.makeParticles('dust', 1);
  Ly.aurora = L.makeParticles('aurora', 1);
  Ly.fires = L.makeParticles('fire', 1);
  Ly.ice = L.makeIce(spec);
  Ly.orbitals = L.makeOrbitals();
  Ly.neo = L.makeNeo();
  Ly.pin = L.makePin();
  Ly.earth.renderOrder = 0;
  scene.add(Ly.earth, Ly.sst, Ly.ice, Ly.air, Ly.dust, Ly.fires, Ly.clouds, Ly.atmo, Ly.aurora, Ly.orbitals, Ly.neo, Ly.pin);
  Ly.sst.visible = Ly.air.visible = Ly.dust.visible = Ly.fires.visible = Ly.ice.visible = false;

  state.engine = createEngine(w, state.ref);
  await state.engine.init();
  state.engine.setClock(state.clock);

  bootMsg.textContent = t('bootSynth');
  status('tle', 'loading', () => t('synthPop'));
  await state.engine.synth(settings.debris, 1, 7);

  camera.position.set(0, 1.2, innerWidth / innerHeight < 0.8 ? 5.6 : 3.6);
  bindUI();
  resize();
  addEventListener('resize', resize);
  requestAnimationFrame(frame);
  $('#boot').classList.add('done');

  status('night', 'ok', 'Black Marble');
  status('clouds', 'ok', () => t('staticTex'));
  locate();
  loadAll();
  schedule();
  connectStream(onStream, (s) => (state.details.ws = s));
  state.details.backend = () => t(backend ? 'backendOn' : 'noBackend');
}

function loadAll() {
  loadTLE(); loadAurora(); loadClouds(); loadNEO();
  // heavier optional layers load lazily on first toggle, except on desktop where we prefetch
  if (!MOBILE) { loadAir(); loadSST(); loadFires(); loadIce(); }
}

function schedule() {
  setInterval(loadAurora, 10 * MIN);
  setInterval(() => state.layers.air.userData.loaded && loadAir(), HOUR);
  setInterval(() => state.layers.fires.userData.loaded && loadFires(), HOUR);
  setInterval(loadClouds, HOUR);
  setInterval(loadTLE, 6 * HOUR);
  setInterval(loadNEO, 6 * HOUR);
  setInterval(() => state.layers.ice.userData.loaded && loadIce(), 24 * HOUR);
  setInterval(() => state.layers.sst.userData.loaded && loadSST(), 24 * HOUR);
  setInterval(refreshCounts, 1000);
}

function onStream(m) {
  // backend pushes tiny "refresh" notices when its cache renews; payloads come from /api (cached)
  const fn = { aurora: loadAurora, air: loadAir, fires: loadFires, tle: loadTLE, neo: loadNEO, seaice: loadIce, sst: loadSST, clouds: loadClouds }[m.layer];
  if (m.type === 'refresh' && fn) fn();
}

// ------------------------------------------------------------------ location
async function locate() {
  status('pin', 'loading', () => t('locating'));
  const geo = () => new Promise((res, rej) => {
    if (!navigator.geolocation) return rej(new Error('no geolocation'));
    navigator.geolocation.getCurrentPosition(
      (p) => res({ lat: p.coords.latitude, lon: p.coords.longitude, label: `${p.coords.latitude.toFixed(3)}°, ${p.coords.longitude.toFixed(3)}°`, source: 'GPS/Geolocation' }),
      rej, { timeout: 8000, maximumAge: 10 * MIN });
  });
  let loc;
  try { loc = await geo(); } catch {
    try { loc = await sources.ipLocation(); } catch { loc = { ...GOTHENBURG, source: null }; }
  }
  state.loc = loc;
  const n = L.placeOnSurface(state.layers.pin, loc.lat, loc.lon);
  const aqi = state.w.x.air_sample(loc.lat, loc.lon);
  $('#pinLabel').textContent = `📍 ${loc.label}${Number.isFinite(aqi) ? ` · AQI ${Math.round(aqi)}` : ''}`;
  status('pin', loc.source ? 'ok' : 'warn', loc.source || (() => t('defaultLoc')));
  // fly the camera above the user
  const dist = camera.position.length();
  camera.position.copy(n.clone().multiplyScalar(dist).add(new THREE.Vector3(0, 0.4, 0)));
}

// ------------------------------------------------------------------ TLE
async function loadTLE() {
  const e = state.engine;
  status('tle', 'loading', () => t('tleLoading'));
  try {
    const parts = await sources.tle();
    await e.clear(0);
    let added = 0;
    for (const p of parts) added += (await e.loadTLE(p.text, p.kind)).added;
    if (added < 100) throw new Error('TLE empty');
    if (settings.debris) await e.synth(settings.debris, 0, 7);
    status('tle', 'ok', `${parts[0].source} ${added.toLocaleString()}`);
    const srcs = parts.map((p) => p.source).filter((v, i, a) => a.indexOf(v) === i).join(', ');
    const cached = parts.some((p) => p.cached), debris = settings.debris;
    state.details.tle = () => t('tleOk', added, srcs, cached, debris);
  } catch (err) {
    if (e.count === 0) await e.synth(settings.debris, 1, 7);
    status('tle', 'warn', () => t('synthPop'));
    const msg = fail(err), debris = settings.debris;
    state.details.tle = () => t('tleFail', msg, debris);
  }
  refreshCounts();
}

async function refreshCounts() {
  if (!state.engine) return;
  state.counts = await state.engine.counts();
  $$('[data-count]').forEach((el) => (el.textContent = Math.round(state.counts[+el.dataset.count] + (el.dataset.count === '3' ? state.counts[4] : 0)).toLocaleString()));
}

// ------------------------------------------------------------------ aurora
async function loadAurora() {
  const w = state.w;
  status('aurora', 'loading', 'OVATION');
  let src;
  try {
    const r = await sources.aurora();
    if (w.x.aurora_parse(w.put(r.text)) < 1000) throw new Error('grid');
    const ft = r.text.match(/"Forecast Time"\s*:\s*"([^"]+)"/);
    const name = `NOAA OVATION ${ft ? ft[1].replace('T', ' ').slice(0, 16) + 'Z' : ''}`;
    src = () => name;
    status('aurora', 'ok', 'NOAA SWPC');
  } catch (err) {
    w.x.aurora_synth(3, julian(simMs()));
    const msg = fail(err);
    src = () => t('auroraFail', msg);
    status('aurora', 'warn', () => t('kp3'));
  }
  const n = w.x.aurora_generate(MOBILE ? 15000 : 45000, 11);
  L.setParticles(state.layers.aurora, w.f32(w.x.aurora_ptr(), n * 5), n);
  w.x.aurora_stats();
  const s = w.scratch(3);
  const pmax = Math.round(s[0]);
  state.details.aurora = () => t('auroraStats', src(), pmax, n);
}

// ------------------------------------------------------------------ air quality
async function loadAir() {
  const w = state.w, Ly = state.layers;
  status('air', 'loading', 'AQI');
  w.x.air_clear();
  let src;
  try {
    const r = await sources.air();
    const kind = r.kind || (r.source || '').toLowerCase();
    const n = kind.includes('waqi') ? w.x.air_parse_waqi(w.put(r.text)) : w.x.air_parse_openmeteo(w.put(r.text));
    if (n < 20) throw new Error('too few stations');
    src = () => t('airSrc', r.source, n);
    status('air', 'ok', r.source);
  } catch (err) {
    w.x.air_clear();
    w.x.air_synth(5);
    const msg = fail(err);
    src = () => t('airFail', msg);
    status('air', 'warn', () => t('synth'));
  }
  const W = 512, H = 256;
  const rgba = new Uint8Array(w.u8(w.x.air_build(W, H), W * H * 4));
  const t = new THREE.DataTexture(rgba, W, H, THREE.RGBAFormat);
  t.flipY = true; t.magFilter = t.minFilter = THREE.LinearFilter; t.needsUpdate = true;
  Ly.air.material.uniforms.map.value?.dispose();
  Ly.air.material.uniforms.map.value = t;
  w.x.air_particles_init(MOBILE ? 5000 : 14000, 3);
  w.x.air_stats();
  const s = w.scratch(3);
  const avg = Math.round(s[1]), max = Math.round(s[2]);
  state.details.air = () => t('airStats', src(), avg, max);
  Ly.air.userData.loaded = true;
  if (state.loc) {
    const a = w.x.air_sample(state.loc.lat, state.loc.lon);
    $('#pinLabel').textContent = `📍 ${state.loc.label}${Number.isFinite(a) ? ` · AQI ${Math.round(a)}` : ''}`;
  }
}

// ------------------------------------------------------------------ clouds
async function loadClouds() {
  const w = state.w, mat = state.layers.clouds.material;
  if (!sources.owmTile(0, 0, 0)) {
    status('clouds', 'ok', () => t('staticTex'));
    state.details.clouds = () => t('cloudsNoKey');
    return;
  }
  status('clouds', 'loading', () => t('owmTiles'));
  try {
    const Z = 2, N = 1 << Z, T = 256;
    const cv = new OffscreenCanvas(N * T, N * T), ctx = cv.getContext('2d');
    await Promise.all(Array.from({ length: N * N }, async (_, i) => {
      const x = i % N, y = (i / N) | 0;
      ctx.drawImage(await loadImage(sources.owmTile(Z, x, y)), x * T, y * T, T, T);
    }));
    const px = ctx.getImageData(0, 0, N * T, N * T).data;
    w.put(new Uint8Array(px.buffer));
    const DW = 2048, DH = 1024;
    const out = new Uint8Array(w.u8(w.x.img_merc_to_equirect(N * T, N * T, DW, DH), DW * DH * 4));
    const t = new THREE.DataTexture(out, DW, DH, THREE.RGBAFormat);
    t.flipY = true; t.magFilter = t.minFilter = THREE.LinearFilter; t.needsUpdate = true;
    mat.uniforms.map.value = t;
    mat.uniforms.useAlpha.value = 1;
    status('clouds', 'ok', 'OpenWeatherMap');
    const at = new Date();
    state.details.clouds = () => t('cloudsOk', Z, at.toLocaleTimeString());
  } catch (err) {
    mat.uniforms.map.value = state.layers.clouds.userData.staticTex;
    mat.uniforms.useAlpha.value = 0;
    status('clouds', 'warn', () => t('staticTex'));
    const msg = fail(err);
    state.details.clouds = () => t('cloudsFail', msg);
  }
}

// ------------------------------------------------------------------ SST anomaly
async function loadSST() {
  const w = state.w, Ly = state.layers;
  status('sst', 'loading', 'GIBS');
  let t;
  try {
    let img;
    for (const d of [2, 3, 5]) { try { img = await loadImage(sources.sstUrl(d)); break; } catch { /* older day */ } }
    if (!img) throw new Error('no GIBS image');
    t = new THREE.Texture(img); t.needsUpdate = true;
    status('sst', 'ok', 'NASA GIBS MUR');
    state.details.sst = () => t('sstOk');
  } catch (err) {
    const W = 1024, H = 512;
    const rgba = new Uint8Array(w.u8(w.x.sst_synth(W, H, 1.1, 9), W * H * 4));
    t = new THREE.DataTexture(rgba, W, H, THREE.RGBAFormat); t.flipY = true; t.magFilter = t.minFilter = THREE.LinearFilter; t.needsUpdate = true;
    status('sst', 'warn', () => t('model'));
    const msg = fail(err);
    state.details.sst = () => t('sstFail', msg);
  }
  Ly.sst.material.uniforms.map.value?.dispose?.();
  Ly.sst.material.uniforms.map.value = t;
  Ly.sst.userData.loaded = true;
}

// ------------------------------------------------------------------ fires
async function loadFires() {
  const w = state.w;
  status('fires', 'loading', 'FIRMS');
  w.x.fire_clear();
  try {
    const r = await sources.fires();
    const n = w.x.fire_parse_csv(w.put(r.text));
    if (n === 0) throw new Error('empty CSV');
    status('fires', 'ok', `FIRMS ${n.toLocaleString()}`);
    state.details.fires = () => t('firesOk', n);
  } catch (err) {
    const doy = (simMs() - Date.UTC(new Date(simMs()).getUTCFullYear(), 0, 0)) / 86400e3;
    const n = w.x.fire_synth(doy, 4);
    status('fires', 'warn', () => t('seasonal'));
    const msg = fail(err);
    state.details.fires = () => t('firesFail', msg, n);
  }
  const n = w.x.fire_build();
  L.setParticles(state.layers.fires, w.f32(w.x.fire_ptr(), n * 5), n);
  state.layers.fires.userData.loaded = true;
}

// ------------------------------------------------------------------ sea ice
async function loadIce() {
  const w = state.w;
  status('ice', 'loading', 'NSIDC');
  const now = new Date(simMs());
  let year = now.getUTCFullYear(), doy = (now - Date.UTC(year, 0, 0)) / 86400e3;
  const ext = { north: 0, south: 0 };
  let live = 0;
  for (const h of ['north', 'south']) {
    try {
      const r = await sources.seaIce(h);
      if (w.x.ice_parse_csv(w.put(r.text))) {
        const s = w.scratch(3);
        ext[h] = s[0]; year = s[1]; doy = s[2]; live++;
      }
    } catch { /* model */ }
  }
  const verts = new Float32Array(w.f32(w.x.ice_build(ext.north, ext.south, year, doy), 4 * 13 * 181 * 3));
  const s = Array.from(w.scratch(4));
  const dims = w.x.ice_dims();
  L.setIce(state.layers.ice, verts, dims >> 16, dims & 0xffff);
  const pct = (a, b) => `${a.toFixed(2)} (${(((a - b) / b) * 100).toFixed(0)}%)`;
  status('ice', live === 2 ? 'ok' : 'warn', live === 2 ? 'NSIDC' : () => t('trend'));
  state.details.ice = () => t('iceBody', t('iceSrc', live === 2), pct(s[0], s[1]), s[1].toFixed(2), pct(s[2], s[3]), s[3].toFixed(2));
  state.layers.ice.userData.loaded = true;
}

// ------------------------------------------------------------------ NEO
async function loadNEO() {
  const w = state.w;
  status('neo', 'loading', 'NeoWs');
  try {
    const r = await sources.neo();
    const n = w.x.neo_parse(w.put(r.text));
    if (!n) throw new Error('empty list');
    state.neoMeta = Array.from({ length: n }, (_, i) => {
      w.x.neo_info(i);
      const s = Array.from(w.scratch(7));
      return { name: w.str(w.x.neo_name_ptr(i), w.x.neo_name_len(i)), missKm: s[0], ld: s[1], v: s[2], dia: s[3], haz: s[4] > 0, ca: s[5] * 86400e3 + s[6] * 1000 };
    });
    buildNeo();
    status('neo', 'ok', `NeoWs ${n}`);
    const top = [...state.neoMeta].sort((a, b) => a.ld - b.ld).slice(0, 5);
    const haz = state.neoMeta.filter((m) => m.haz).length;
    const list = '<ul>' + top.map((m) => `<li>${m.haz ? '⚠️ ' : ''}${m.name} · ${m.ld.toFixed(1)} LD · ${m.v.toFixed(1)} km/s · ${new Date(m.ca).toISOString().slice(5, 16).replace('T', ' ')}Z</li>`).join('') + '</ul>';
    state.details.neo = () => t('neoOk', n, haz) + list;
  } catch (err) {
    status('neo', 'err', fail(err));
    const msg = fail(err);
    state.details.neo = () => t('neoFail', msg);
  }
}

function buildNeo() {
  const w = state.w, n = state.neoMeta.length;
  if (!n) return;
  w.x.neo_build(simMs());
  const per = w.x.neo_points();
  const lines = new Float32Array(w.f32(w.x.neo_lines_ptr(), n * per * 3));
  const now = new Float32Array(w.f32(w.x.neo_now_ptr(), n * 3));
  L.setNeo(state.layers.neo, lines, now, state.neoMeta.map((m) => m.haz), per);
}

// ------------------------------------------------------------------ UI
const LAZY = { air: loadAir, sst: loadSST, fires: loadFires, ice: loadIce };

function applyVisibility() {
  const v = state.visible, Ly = state.layers;
  Ly.pin.visible = v.pin;
  $('#pinLabel').hidden = !v.pin || !state.loc;
  Ly.clouds.visible = v.clouds;
  Ly.air.visible = Ly.dust.visible = v.air;
  Ly.aurora.visible = v.aurora;
  Ly.sst.visible = v.sst;
  Ly.fires.visible = v.fires;
  Ly.ice.visible = v.ice;
  Ly.earth.material.uniforms.uNight.value = v.night ? 1 : 0;
  Ly.neo.visible = v.neo;
  Ly.orbitals.material.uniforms.uMask.value.set(+v.leo, +v.meo, +v.geo, +v.sats);
  renderDetails();
}

function bindUI() {
  $$('[data-layer]').forEach((cb) => {
    state.visible[cb.dataset.layer] = cb.checked;
    cb.addEventListener('change', () => {
      state.visible[cb.dataset.layer] = cb.checked;
      const lazy = LAZY[cb.dataset.layer];
      if (cb.checked && lazy && !state.layers[cb.dataset.layer].userData.loaded) lazy();
      applyVisibility();
    });
  });
  applyVisibility();

  const panel = $('#panel'), toggle = $('#panelToggle');
  const setPanel = (open) => {
    panel.classList.toggle('closed', !open);
    toggle.classList.toggle('shift', !open);
    toggle.setAttribute('aria-expanded', open);
  };
  if (MOBILE) setPanel(false);
  toggle.addEventListener('click', () => setPanel(panel.classList.contains('closed')));

  $('#langBtn').addEventListener('click', () => setLang(getLang() === 'ko' ? 'en' : 'ko'));
  onLangChange(() => {
    for (const [k, [st, text]] of Object.entries(statusMem)) status(k, st, text);
    renderDetails();
    if (!tip.hidden && state.sel < 0) closeTip(); // NEO tooltip is static; satellite tooltips re-render every second
    hud();
  });

  $('#speed').addEventListener('change', (e) => setClock(simMs(), +e.target.value));
  $('#nowBtn').addEventListener('click', () => setClock(Date.now(), state.clock.speed));

  const form = $('#settings');
  for (const el of form.elements) if (el.name) el.value = settings[el.name] ?? '';
  const out = $('#debrisOut');
  const showDebris = () => (out.textContent = (+form.elements.debris.value).toLocaleString());
  form.elements.debris.addEventListener('input', showDebris);
  showDebris();
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    for (const el of form.elements) if (el.name) settings[el.name] = el.type === 'range' ? +el.value : el.value.trim();
    saveSettings();
    loadAll();
  });

  // click picking (ignore drags)
  let down = null;
  canvas.addEventListener('pointerdown', (e) => (down = { x: e.clientX, y: e.clientY }));
  canvas.addEventListener('pointerup', (e) => {
    if (!down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > 6) return;
    pickAt(e.clientX, e.clientY);
  });
}

function renderDetails() {
  const v = state.visible, wait = t('loading');
  const parts = [];
  if (v.air) parts.push(`<div><b>${t('dAir')}</b><div class="bar" style="background:linear-gradient(90deg,#00d959,#5ae600,#ffeb00,#ff8000,#ff0000,#8f4099,#800024)"></div><div class="scale"><span>0</span><span>50</span><span>100</span><span>150</span><span>200</span><span>300+</span></div>${detail('air') || wait}</div>`);
  if (v.sst) parts.push(`<div><b>${t('dSst')}</b><div class="bar" style="background:linear-gradient(90deg,#1a4dff,#fff,#ff1f0d)"></div><div class="scale"><span>−3°C</span><span>0</span><span>+3°C</span></div>${detail('sst') || wait}</div>`);
  if (v.fires) parts.push(`<div><b>${t('dFires')}</b> ${detail('fires') || wait}</div>`);
  if (v.ice) parts.push(`<div><b>${t('dIce')}</b> ${detail('ice') || wait}</div>`);
  if (v.aurora) parts.push(`<div><b>${t('dAurora')}</b> ${detail('aurora') || wait}</div>`);
  if (v.clouds) parts.push(`<div><b>${t('dClouds')}</b> ${detail('clouds') || ''}</div>`);
  parts.push(`<div><b>${t('dOrbit')}</b> ${detail('tle') || ''}<br><small>${t('orbitNote')}</small></div>`);
  if (v.neo) parts.push(`<div><b>${t('dNeo')}</b> ${detail('neo') || wait}</div>`);
  parts.push(`<div><b>${t('dBackend')}</b> ${detail('backend') || ''} · WebSocket ${detail('ws') || 'off'}</div>`);
  $('#details').innerHTML = parts.join('');
}
setInterval(() => $('#legendBox')?.open && renderDetails(), 2000);

// ------------------------------------------------------------------ picking & tooltip
const tip = $('#tooltip');
let tipTimer = 0;

function placeTip(x, y) {
  const r = tip.getBoundingClientRect();
  tip.style.left = Math.min(innerWidth - r.width - 12, x + 14) + 'px';
  tip.style.top = Math.min(innerHeight - r.height - 12, Math.max(12, y - 10)) + 'px';
}

function closeTip() {
  tip.hidden = true;
  clearInterval(tipTimer);
  state.sel = -1;
  state.layers.orbitals.material.uniforms.uSel.value = -1;
}

async function pickAt(cx, cy) {
  const rect = canvas.getBoundingClientRect();
  const nx = ((cx - rect.left) / rect.width) * 2 - 1, ny = -((cy - rect.top) / rect.height) * 2 + 1;
  // NEO markers first (few objects, done in JS)
  if (state.visible.neo && state.neoMeta.length) {
    const now = state.layers.neo.userData.now;
    let best = -1, bd = 14 * 14;
    const v = new THREE.Vector3();
    for (let i = 0; i < state.neoMeta.length; i++) {
      v.set(now[i * 3], now[i * 3 + 1], now[i * 3 + 2]).project(camera);
      const dx = (v.x - nx) * rect.width / 2, dy = (v.y - ny) * rect.height / 2, d = dx * dx + dy * dy;
      if (v.z < 1 && d < bd) { bd = d; best = i; }
    }
    if (best >= 0) return showNeoTip(best, cx, cy);
  }
  const vis = state.visible;
  const mask = (vis.leo ? 1 : 0) | (vis.meo ? 2 : 0) | (vis.geo ? 4 : 0) | (vis.sats ? 8 : 0);
  camera.updateMatrixWorld();
  const vp = new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
  const vpCam = [...vp.elements, camera.position.x, camera.position.y, camera.position.z];
  const idx = await state.engine.pick(vpCam, [nx, ny, rect.width, rect.height, MOBILE ? 22 : 12, mask, (simMs() - state.ref) / 1000]);
  if (idx < 0) return closeTip();
  state.sel = idx;
  state.layers.orbitals.material.uniforms.uSel.value = idx;
  const render = async () => {
    const r = await state.engine.info(idx);
    if (!r.ok) return closeTip();
    const [alt, spd, lat, lon, inc, per, pg, ap, norad, cat, syn, ecc] = r.vals;
    const catName = t('cats')[cat] || '-';
    tip.innerHTML = `<h3><span>${escapeHtml(r.name)}</span><button aria-label="${t('close')}">×</button></h3>
      <dl>
        <dt>${t('tCat')}</dt><dd>${catName} ${syn ? `<span class="tag">${t('synth')}</span>` : `<span class="tag">NORAD ${norad}</span>`}</dd>
        <dt>${t('tAlt')}</dt><dd>${alt.toLocaleString(undefined, { maximumFractionDigits: 0 })} km</dd>
        <dt>${t('tSpeed')}</dt><dd>${spd.toFixed(3)} km/s (${(spd * 3600).toLocaleString(undefined, { maximumFractionDigits: 0 })} km/h)</dd>
        <dt>${t('tPos')}</dt><dd>${lat.toFixed(2)}°, ${lon.toFixed(2)}°</dd>
        <dt>${t('tInc')}</dt><dd>${inc.toFixed(2)}° · e ${ecc.toFixed(4)}</dd>
        <dt>${t('tPeriod')}</dt><dd>${per.toFixed(1)} ${t('min')}</dd>
        <dt>${t('tPgAp')}</dt><dd>${pg.toFixed(0)} / ${ap.toFixed(0)} km</dd>
      </dl>`;
    tip.querySelector('button').onclick = closeTip;
  };
  await render();
  tip.hidden = false;
  placeTip(cx, cy);
  clearInterval(tipTimer);
  tipTimer = setInterval(render, 1000);
}

function showNeoTip(i, cx, cy) {
  const m = state.neoMeta[i];
  clearInterval(tipTimer);
  tip.innerHTML = `<h3><span>${m.haz ? '⚠️ ' : ''}${escapeHtml(m.name)}</span><button aria-label="${t('close')}">×</button></h3>
    <dl>
      <dt>${t('tCat')}</dt><dd>${t('asteroid')} ${m.haz ? `<span class="tag">${t('hazard')}</span>` : ''}</dd>
      <dt>${t('tClosest')}</dt><dd>${new Date(m.ca).toISOString().replace('T', ' ').slice(0, 16)} UTC</dd>
      <dt>${t('tDist')}</dt><dd>${m.ld.toFixed(2)} LD · ${(m.missKm / 1e6).toFixed(2)} ${t('mkm')}</dd>
      <dt>${t('tRelV')}</dt><dd>${m.v.toFixed(2)} km/s</dd>
      <dt>${t('tDia')}</dt><dd>≤ ${(m.dia * 1000).toFixed(0)} m</dd>
    </dl>`;
  tip.querySelector('button').onclick = closeTip;
  tip.hidden = false;
  placeTip(cx, cy);
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

// ------------------------------------------------------------------ render loop
const sun = new THREE.Vector3();
const tmp = new THREE.Vector3();
let last = performance.now(), fpsAcc = 0, fpsN = 0, fps = 60, lodTimer = 0, neoTimer = 0;

function resize() {
  const w = innerWidth, h = innerHeight;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  const pr = renderer.getPixelRatio();
  state.layers.orbitals.material.uniforms.uViewport.value.set(w * pr, h * pr);
  state.layers.orbitals.material.uniforms.uSize.value = (MOBILE ? 1.9 : 1.6) * pr;
  for (const k of ['aurora', 'dust', 'fires']) state.layers[k].material.uniforms.uScale.value = pr;
}

function syncOrbitals() {
  const e = state.engine, mesh = state.layers.orbitals;
  e.tick();
  const arr = e.array;
  if (mesh.userData.version !== e.version || mesh.userData.arr !== arr) {
    L.bindOrbitals(mesh, arr);
    mesh.userData.version = e.version;
    mesh.userData.arr = arr;
  }
  const n = e.count;
  const ib = mesh.userData.ib;
  if (ib.clearUpdateRanges) { ib.clearUpdateRanges(); ib.addUpdateRange(0, n * 8); }
  ib.needsUpdate = true;
  mesh.count = Math.min(n, state.lodCap, arr.length / 8);
}

function frame(now) {
  requestAnimationFrame(frame);
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  const sm = simMs();
  const jd = julian(sm);
  const t = (sm - state.ref) / 1000;
  const w = state.w, Ly = state.layers;

  w.x.sun_dir(jd);
  const s = w.scratch(3);
  sun.set(s[0], s[1], s[2]);
  for (const m of [Ly.earth, Ly.atmo, Ly.clouds]) m.material.uniforms.sunDir.value.copy(sun);
  Ly.ice.userData.mat.uniforms.sunDir.value.copy(sun);

  syncOrbitals();
  Ly.orbitals.material.uniforms.uTime.value = t;

  // clouds drift eastward relative to the surface (~ mid-latitude westerlies, visually scaled)
  if (Ly.clouds.visible) Ly.clouds.rotation.y += dt * 2.5e-4 * Math.min(state.clock.speed, 600) ** 0.5;
  for (const k of ['aurora', 'fires', 'dust']) Ly[k].material.uniforms.uTime.value = now / 1000;
  Ly.sst.material.uniforms.uTime.value = now / 1000;

  if (Ly.dust.visible && Ly.air.userData.loaded) {
    const n = MOBILE ? 5000 : 14000;
    const ptr = w.x.air_step(dt, 35000 * Math.min(state.clock.speed, 60) ** 0.35);
    L.setParticles(Ly.dust, w.f32(ptr, n * 5), n);
  }

  if ((neoTimer += dt) > 2 && Ly.neo.visible) { neoTimer = 0; buildNeo(); }

  // user pin: pulse + screen-constant size + HTML label
  if (Ly.pin.visible && state.loc) {
    const dist = camera.position.distanceTo(Ly.pin.position);
    Ly.pin.scale.setScalar(Math.max(0.35, Math.min(2.5, dist / 2.2)));
    const ph = (now / 1600) % 1;
    Ly.pin.userData.ring.scale.setScalar(0.01 + ph * 0.06);
    Ly.pin.userData.ring.material.opacity = 1 - ph;
    Ly.pin.userData.glow.material.opacity = 0.25 + 0.2 * Math.sin(now / 250);
    tmp.copy(Ly.pin.position).multiplyScalar(1.0 + 0.05 * Ly.pin.scale.x);
    const facing = tmp.clone().normalize().dot(tmp.clone().sub(camera.position).negate().normalize()) > 0.05;
    tmp.project(camera);
    const lbl = $('#pinLabel');
    lbl.hidden = !facing;
    lbl.style.left = ((tmp.x + 1) / 2) * innerWidth + 'px';
    lbl.style.top = ((1 - tmp.y) / 2) * innerHeight + 'px';
  }

  controls.update();
  renderer.render(scene, camera);

  // FPS + LOD: shrink the rendered instance count when frames drop
  fpsAcc += dt; fpsN++;
  if (fpsAcc > 0.5) { fps = fpsN / fpsAcc; fpsAcc = 0; fpsN = 0; hud(); }
  if ((lodTimer += dt) > 2) {
    lodTimer = 0;
    if (fps < 45 && state.lodCap > 8000) state.lodCap = Math.round(Math.min(state.lodCap, state.engine.count) * 0.8);
    else if (fps > 57 && state.lodCap < 120000) state.lodCap = Math.round(state.lodCap * 1.15);
  }
}

function hud() {
  const e = state.engine;
  const d = new Date(simMs());
  $('#simTime').textContent = `${d.toISOString().replace('T', ' ').slice(0, 19)} UTC`;
  $('#hud').textContent =
    `FPS ${fps.toFixed(0)} · ${t('hudObjects')} ${state.layers.orbitals.count.toLocaleString()} / ${e.count.toLocaleString()}\n`
    + `SGP4 ${e.computeMs.toFixed(1)} ms · ${e.mode}\n`
    + `crossOriginIsolated=${self.crossOriginIsolated} · LOD cap ${state.lodCap.toLocaleString()}${MOBILE ? ` (${t('mobile')})` : ''}`;
}

applyStatic();
$('#bootMsg').textContent = t('bootLoading');
boot().catch((err) => {
  console.error(err);
  $('#bootMsg').textContent = t('bootFail', err.message);
});

// Data pipeline: prefers the caching backend (server/server.mjs, /api/*) when present,
// otherwise calls the public APIs directly. Every source has a TTL so reloads respect
// upstream rate limits; failures are reported so the caller can fall back to WASM models.

const LS = 'earth.settings';
export const settings = Object.assign(
  { owmKey: '', waqiToken: '', firmsKey: '', nasaKey: '', tleUrl: '', wsUrl: '', debris: 0 },
  safeJSON(lsGet(LS)),
);
export function saveSettings() {
  lsSet(LS, JSON.stringify(settings));
}

function lsGet(k) {
  try { return localStorage.getItem(k); } catch { return null; }
}
function lsSet(k, v) {
  try { localStorage.setItem(k, v); } catch { /* quota or privacy mode */ }
}
function safeJSON(s) {
  try { return JSON.parse(s) || {}; } catch { return {}; }
}

let backend = null;
export async function detectBackend() {
  try {
    const r = await fetch('api/health', { cache: 'no-store' });
    backend = r.ok ? await r.json() : false;
  } catch {
    backend = false;
  }
  return backend;
}
export const hasBackend = () => !!backend;

async function getText(url, { timeout = 20000 } = {}) {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), timeout);
  try {
    const r = await fetch(url, { signal: ctl.signal });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    return { text: await r.text(), source: r.headers.get('x-source') || new URL(url, location.href).host };
  } finally {
    clearTimeout(t);
  }
}

/** Client-side TTL cache (localStorage) for medium-sized text payloads. */
async function cached(key, ttlMs, fn) {
  const hit = safeJSON(lsGet('earth.cache.' + key));
  if (hit.t && Date.now() - hit.t < ttlMs && hit.text) return { ...hit, cached: true };
  const v = await fn();
  if (v.text.length < 2_500_000) lsSet('earth.cache.' + key, JSON.stringify({ ...v, t: Date.now() }));
  return v;
}

const ymd = (d) => d.toISOString().slice(0, 10);
const H = 3600e3;

export const sources = {
  aurora: () => getText(backend ? 'api/aurora' : 'https://services.swpc.noaa.gov/json/ovation_aurora_latest.json'),

  async air() {
    if (backend) return getText('api/air');
    if (settings.waqiToken) {
      return getText(`https://api.waqi.info/v2/map/bounds?latlng=-60,-180,85,180&networks=all&token=${encodeURIComponent(settings.waqiToken)}`)
        .then((r) => ({ ...r, kind: 'waqi', source: 'WAQI' }));
    }
    return cached('air', H, async () => {
      // Copernicus CAMS via Open-Meteo, 10° grid in 3 multi-location requests
      const lats = [], lons = [];
      for (let la = -60; la <= 80; la += 10) for (let lo = -180; lo < 180; lo += 10) { lats.push(la); lons.push(lo); }
      const parts = [];
      for (let i = 0; i < lats.length; i += 180) {
        const u = `https://air-quality-api.open-meteo.com/v1/air-quality?latitude=${lats.slice(i, i + 180)}&longitude=${lons.slice(i, i + 180)}&current=us_aqi,pm2_5`;
        parts.push((await getText(u)).text);
      }
      return { text: parts.join('\n'), kind: 'openmeteo', source: 'Open-Meteo / CAMS' };
    });
  },

  async tle() {
    if (settings.tleUrl) return [{ ...(await getText(settings.tleUrl, { timeout: 60000 })), kind: 2 }];
    if (backend) {
      const out = [{ ...(await getText('api/tle?group=active', { timeout: 60000 })), kind: 1 }];
      try { out.push({ ...(await getText('api/tle?group=debris', { timeout: 60000 })), kind: 0 }); } catch { /* optional */ }
      return out;
    }
    const ct = (g) => cached('tle-' + g, 6 * H, () => getText(`https://celestrak.org/NORAD/elements/gp.php?GROUP=${g}&FORMAT=tle`, { timeout: 60000 }));
    const out = [{ ...(await ct('active')), kind: 1 }];
    for (const g of ['cosmos-1408-debris', 'fengyun-1c-debris', 'iridium-33-debris', 'cosmos-2251-debris']) {
      try { out.push({ ...(await ct(g)), kind: 0 }); } catch { /* keep going */ }
    }
    return out;
  },

  async fires() {
    if (backend) return getText('api/fires', { timeout: 90000 });
    if (!settings.firmsKey) throw new Error('no FIRMS MAP_KEY');
    return getText(`https://firms.modaps.eosdis.nasa.gov/api/area/csv/${settings.firmsKey}/VIIRS_SNPP_NRT/world/1`, { timeout: 90000 });
  },

  async seaIce(hemi) {
    if (backend) return getText(`api/seaice?hemi=${hemi}`);
    const p = hemi === 'north' ? 'north/daily/data/N' : 'south/daily/data/S';
    return cached('ice-' + hemi, 24 * H, () => getText(`https://noaadata.apps.nsidc.org/NOAA/G02135/${p}_seaice_extent_daily_v3.0.csv`));
  },

  async neo() {
    if (backend) return getText('api/neo');
    const d0 = new Date();
    const d1 = new Date(Date.now() + 6 * 86400e3);
    return cached('neo-' + ymd(d0), 6 * H, () =>
      getText(`https://api.nasa.gov/neo/rest/v1/feed?start_date=${ymd(d0)}&end_date=${ymd(d1)}&api_key=${settings.nasaKey || 'DEMO_KEY'}`));
  },

  /** NASA GIBS MUR SST anomaly (EPSG:4326 WMS) as an image URL. */
  sstUrl(daysAgo = 2) {
    const d = ymd(new Date(Date.now() - daysAgo * 86400e3));
    if (backend) return `api/sst?date=${d}`;
    return 'https://gibs.earthdata.nasa.gov/wms/epsg4326/best/wms.cgi?SERVICE=WMS&REQUEST=GetMap&VERSION=1.3.0'
      + '&LAYERS=GHRSST_L4_MUR_Sea_Surface_Temperature_Anomalies&CRS=EPSG:4326&BBOX=-90,-180,90,180'
      + `&WIDTH=2048&HEIGHT=1024&FORMAT=image/png&TRANSPARENT=true&TIME=${d}`;
  },

  /** OpenWeatherMap clouds_new tile URL, or null without key/backend. */
  owmTile(z, x, y) {
    if (backend?.owm) return `api/owm/${z}/${x}/${y}`;
    if (!settings.owmKey) return null;
    return `https://tile.openweathermap.org/map/clouds_new/${z}/${x}/${y}.png?appid=${settings.owmKey}`;
  },

  async ipLocation() {
    if (backend) {
      const j = JSON.parse((await getText('api/ipgeo')).text);
      if (j.lat != null) return { lat: j.lat, lon: j.lon, label: [j.city, j.regionName, j.country].filter(Boolean).join(', '), source: 'IP-API' };
    }
    const j = JSON.parse((await getText('https://get.geojs.io/v1/ip/geo.json')).text);
    return { lat: +j.latitude, lon: +j.longitude, label: [j.city, j.region, j.country].filter(Boolean).join(', '), source: 'GeoJS (IP)' };
  },
};

export function loadImage(url) {
  return new Promise((res, rej) => {
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => res(img);
    img.onerror = () => rej(new Error('image load failed'));
    img.src = url;
  });
}

/** WebSocket stream from the backend (or a user-supplied URL): { type, text, source }. */
export function connectStream(onMessage, onState) {
  const url = settings.wsUrl || (backend ? `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws` : '');
  if (!url) return onState?.('off');
  let retry = 2000;
  const open = () => {
    let ws;
    try { ws = new WebSocket(url); } catch { return onState?.('error'); }
    ws.onopen = () => { retry = 2000; onState?.('live'); };
    ws.onmessage = (e) => { try { onMessage(JSON.parse(e.data)); } catch { /* ignore malformed */ } };
    ws.onclose = () => { onState?.('reconnecting'); setTimeout(open, (retry = Math.min(retry * 2, 60000))); };
  };
  open();
}

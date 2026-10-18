#!/usr/bin/env node
// Earth Monitor backend (zero dependencies, Node ≥ 18):
//  • serves the static app with COOP/COEP headers → crossOriginIsolated → SharedArrayBuffer
//  • /api/* caching proxy for upstream APIs (TTL per source, memory + disk) — NFR-3 rate limits
//  • /ws WebSocket that pushes {type:"refresh", layer} when a cached source renews
//
// Env: PORT, SPACETRACK_USER, SPACETRACK_PASS, WAQI_TOKEN, FIRMS_MAP_KEY, NASA_API_KEY, OWM_KEY
import http from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CACHE_DIR = path.join(ROOT, 'server', '.cache');
const PORT = +process.env.PORT || 8080;
const env = process.env;
const MIN = 60e3, HOUR = 3600e3;

const MIME = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.json': 'application/json', '.wasm': 'application/wasm',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.svg': 'image/svg+xml', '.md': 'text/markdown; charset=utf-8',
};
const ISOLATION = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
  'Cross-Origin-Resource-Policy': 'same-origin',
};

// ------------------------------------------------------------------ cache
const mem = new Map(); // key → { t, body, type, source }
const inflight = new Map();

async function cached(key, ttl, fetcher) {
  const hit = mem.get(key) || (await readDisk(key));
  if (hit && Date.now() - hit.t < ttl) return hit;
  if (inflight.has(key)) return inflight.get(key);
  const p = (async () => {
    try {
      const v = { ...(await fetcher()), t: Date.now() };
      mem.set(key, v);
      writeDisk(key, v);
      broadcast({ type: 'refresh', layer: key.split(':')[0] });
      return v;
    } catch (e) {
      if (hit) return { ...hit, stale: true }; // serve stale on upstream failure
      throw e;
    } finally {
      inflight.delete(key);
    }
  })();
  inflight.set(key, p);
  return p;
}

const diskName = (k) => path.join(CACHE_DIR, crypto.createHash('sha1').update(k).digest('hex'));
async function readDisk(k) {
  try {
    const meta = JSON.parse(await fs.readFile(diskName(k) + '.json', 'utf8'));
    const v = { ...meta, body: await fs.readFile(diskName(k) + '.bin') };
    mem.set(k, v);
    return v;
  } catch { return null; }
}
async function writeDisk(k, v) {
  try {
    await fs.mkdir(CACHE_DIR, { recursive: true });
    await fs.writeFile(diskName(k) + '.bin', v.body);
    await fs.writeFile(diskName(k) + '.json', JSON.stringify({ t: v.t, type: v.type, source: v.source }));
  } catch (e) { console.warn('cache write', e.message); }
}

async function up(url, opts = {}) {
  const r = await fetch(url, { ...opts, signal: AbortSignal.timeout(opts.timeout || 60000) });
  if (!r.ok) throw new Error(`${new URL(url).host} HTTP ${r.status}`);
  return { body: Buffer.from(await r.arrayBuffer()), type: r.headers.get('content-type') || 'text/plain' };
}

// ------------------------------------------------------------------ sources
let stCookie = null;
async function spaceTrack(query) {
  if (!stCookie) {
    const r = await fetch('https://www.space-track.org/ajaxauth/login', {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ identity: env.SPACETRACK_USER, password: env.SPACETRACK_PASS }),
    });
    if (!r.ok) throw new Error(`space-track login ${r.status}`);
    stCookie = (r.headers.getSetCookie?.() || [r.headers.get('set-cookie')]).map((c) => c.split(';')[0]).join('; ');
  }
  const r = await up(`https://www.space-track.org${query}`, { headers: { cookie: stCookie }, timeout: 120000 });
  return r;
}

const ymd = (d) => d.toISOString().slice(0, 10);
const API = {
  async health() {
    return { body: Buffer.from(JSON.stringify({ ok: true, owm: !!env.OWM_KEY, spacetrack: !!env.SPACETRACK_USER, waqi: !!env.WAQI_TOKEN, firms: !!env.FIRMS_MAP_KEY })), type: 'application/json', source: 'local' };
  },
  aurora: () => cached('aurora', 10 * MIN, async () => ({ ...(await up('https://services.swpc.noaa.gov/json/ovation_aurora_latest.json')), source: 'NOAA SWPC' })),
  air: () => cached('air', HOUR, async () => {
    if (env.WAQI_TOKEN) return { ...(await up(`https://api.waqi.info/v2/map/bounds?latlng=-60,-180,85,180&networks=all&token=${env.WAQI_TOKEN}`)), source: 'waqi' };
    const lats = [], lons = [];
    for (let la = -60; la <= 80; la += 10) for (let lo = -180; lo < 180; lo += 10) { lats.push(la); lons.push(lo); }
    const parts = [];
    for (let i = 0; i < lats.length; i += 180)
      parts.push((await up(`https://air-quality-api.open-meteo.com/v1/air-quality?latitude=${lats.slice(i, i + 180)}&longitude=${lons.slice(i, i + 180)}&current=us_aqi,pm2_5`)).body);
    return { body: Buffer.concat(parts.flatMap((p) => [p, Buffer.from('\n')])), type: 'application/json', source: 'openmeteo' };
  }),
  tle: (q) => {
    const group = q.get('group') === 'debris' ? 'debris' : 'active';
    if (env.SPACETRACK_USER) {
      // Space-Track asks for GP queries at most hourly; we refresh every 6 h.
      return cached(`tle:st-${group}`, 6 * HOUR, async () => {
        const type = group === 'debris' ? 'DEBRIS' : 'PAYLOAD';
        const r = await spaceTrack(`/basicspacedata/query/class/gp/OBJECT_TYPE/${type}/decay_date/null-val/epoch/%3Enow-30/orderby/norad_cat_id/format/3le`);
        return { ...r, source: 'Space-Track' };
      });
    }
    return cached(`tle:ct-${group}`, 6 * HOUR, async () => {
      const groups = group === 'debris' ? ['cosmos-1408-debris', 'fengyun-1c-debris', 'iridium-33-debris', 'cosmos-2251-debris'] : ['active'];
      const bodies = [];
      for (const g of groups) bodies.push((await up(`https://celestrak.org/NORAD/elements/gp.php?GROUP=${g}&FORMAT=tle`)).body);
      return { body: Buffer.concat(bodies), type: 'text/plain', source: 'CelesTrak' };
    });
  },
  fires: () => {
    if (!env.FIRMS_MAP_KEY) throw Object.assign(new Error('FIRMS_MAP_KEY not set'), { status: 503 });
    return cached('fires', HOUR, async () => ({ ...(await up(`https://firms.modaps.eosdis.nasa.gov/api/area/csv/${env.FIRMS_MAP_KEY}/VIIRS_SNPP_NRT/world/1`, { timeout: 120000 })), source: 'NASA FIRMS' }));
  },
  seaice: (q) => {
    const h = q.get('hemi') === 'south' ? 'south' : 'north';
    return cached(`seaice:${h}`, 24 * HOUR, async () => ({
      ...(await up(`https://noaadata.apps.nsidc.org/NOAA/G02135/${h}/daily/data/${h === 'north' ? 'N' : 'S'}_seaice_extent_daily_v3.0.csv`)), source: 'NSIDC',
    }));
  },
  neo: () => cached(`neo:${ymd(new Date())}`, 6 * HOUR, async () => {
    const d0 = new Date(), d1 = new Date(Date.now() + 6 * 86400e3);
    return { ...(await up(`https://api.nasa.gov/neo/rest/v1/feed?start_date=${ymd(d0)}&end_date=${ymd(d1)}&api_key=${env.NASA_API_KEY || 'DEMO_KEY'}`)), source: 'NASA NeoWs' };
  }),
  sst: (q) => {
    const d = /^\d{4}-\d{2}-\d{2}$/.test(q.get('date') || '') ? q.get('date') : ymd(new Date(Date.now() - 2 * 86400e3));
    return cached(`sst:${d}`, 24 * HOUR, async () => ({
      ...(await up('https://gibs.earthdata.nasa.gov/wms/epsg4326/best/wms.cgi?SERVICE=WMS&REQUEST=GetMap&VERSION=1.3.0'
        + `&LAYERS=GHRSST_L4_MUR_Sea_Surface_Temperature_Anomalies&CRS=EPSG:4326&BBOX=-90,-180,90,180&WIDTH=2048&HEIGHT=1024&FORMAT=image/png&TRANSPARENT=true&TIME=${d}`)),
      source: 'NASA GIBS',
    }));
  },
  owm: (q, rest) => {
    const [z, x, y] = rest.map(Number);
    if (!env.OWM_KEY || ![z, x, y].every(Number.isInteger) || z > 6) throw Object.assign(new Error('OWM unavailable'), { status: 503 });
    return cached(`clouds:${z}/${x}/${y}`, HOUR, async () => ({ ...(await up(`https://tile.openweathermap.org/map/clouds_new/${z}/${x}/${y}.png?appid=${env.OWM_KEY}`)), source: 'OpenWeatherMap' }));
  },
  async ipgeo(q, rest, req) {
    const ip = (req.headers['x-forwarded-for'] || req.socket.remoteAddress || '').split(',')[0].trim().replace('::ffff:', '');
    const local = /^(127\.|10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|::1$|$)/.test(ip);
    return { ...(await up(`http://ip-api.com/json/${local ? '' : ip}?fields=status,country,regionName,city,lat,lon`)), source: 'IP-API' };
  },
};

// ------------------------------------------------------------------ http
const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  try {
    if (url.pathname.startsWith('/api/')) {
      const [name, ...rest] = url.pathname.slice(5).split('/');
      const fn = API[name];
      if (!fn) return send(res, 404, 'unknown api');
      const v = await fn(url.searchParams, rest, req);
      res.writeHead(200, { ...ISOLATION, 'content-type': v.type, 'x-source': v.source || '', 'cache-control': 'no-cache', ...(v.stale ? { 'x-stale': '1' } : {}) });
      return res.end(v.body);
    }
    let file = path.normalize(path.join(ROOT, decodeURIComponent(url.pathname)));
    if (!file.startsWith(ROOT) || file.includes(`${path.sep}server${path.sep}.cache`)) return send(res, 403, 'forbidden');
    if ((await fs.stat(file).catch(() => null))?.isDirectory()) file = path.join(file, 'index.html');
    const body = await fs.readFile(file);
    res.writeHead(200, { ...ISOLATION, 'content-type': MIME[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-cache' });
    res.end(body);
  } catch (e) {
    send(res, e.status || (e.code === 'ENOENT' ? 404 : 502), e.message);
  }
});

function send(res, code, msg) {
  res.writeHead(code, { ...ISOLATION, 'content-type': 'text/plain; charset=utf-8' });
  res.end(msg);
}

// ------------------------------------------------------------------ minimal WebSocket (RFC 6455, server→client text)
const clients = new Set();
server.on('upgrade', (req, sock) => {
  if (new URL(req.url, 'http://x').pathname !== '/ws' || req.headers.upgrade?.toLowerCase() !== 'websocket') return sock.destroy();
  const accept = crypto.createHash('sha1').update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
  sock.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  clients.add(sock);
  sock.on('data', (buf) => {
    const op = buf[0] & 0x0f;
    if (op === 0x8) { sock.end(Buffer.from([0x88, 0])); clients.delete(sock); }
    else if (op === 0x9) sock.write(Buffer.from([0x8a, 0]));
  });
  sock.on('close', () => clients.delete(sock));
  sock.on('error', () => clients.delete(sock));
  wsSend(sock, { type: 'hello', sources: Object.keys(API) });
});

function wsSend(sock, obj) {
  const data = Buffer.from(JSON.stringify(obj));
  const len = data.length;
  const head = len < 126 ? Buffer.from([0x81, len]) : len < 65536 ? Buffer.from([0x81, 126, len >> 8, len & 255]) : (() => {
    const b = Buffer.alloc(10); b[0] = 0x81; b[1] = 127; b.writeBigUInt64BE(BigInt(len), 2); return b;
  })();
  sock.write(Buffer.concat([head, data]));
}
function broadcast(obj) {
  for (const s of clients) wsSend(s, obj);
}

// keep the hot caches warm so clients get pushes
setInterval(() => API.aurora().catch(() => {}), 10 * MIN);
setInterval(() => API.air().catch(() => {}), HOUR);
setInterval(() => { try { API.fires().catch(() => {}); } catch { /* no key */ } }, HOUR);
setInterval(() => broadcast({ type: 'ping', t: Date.now() }), 30e3);

server.listen(PORT, () => console.log(`Earth Monitor → http://localhost:${PORT}  (COOP/COEP on, /api cache, /ws)`));

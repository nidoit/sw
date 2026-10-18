// Two interchangeable orbit engines with one interface:
//  • SharedEngine — WASM in a Web Worker, positions shared zero-copy via SharedArrayBuffer
//    (needs cross-origin isolation: COOP/COEP headers, served by server/server.mjs).
//  • LocalEngine — WASM on the main thread, round-robin SGP4 with an adaptive time budget;
//    the renderer reads a Float32Array view straight over WASM linear memory (no copy).
// Both write 8 floats per object: x, y, z, t | vx, vy, vz, category (scene units, seconds).
import { julian } from './wasm.js';

export const STRIDE = 8;

export function createEngine(wasm, ref) {
  return self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'
    ? new SharedEngine(ref)
    : new LocalEngine(wasm, ref);
}

class SharedEngine {
  constructor(ref) {
    this.mode = 'Worker + SharedArrayBuffer';
    this.ref = ref;
    this.worker = new Worker(new URL('./orbit-worker.js', import.meta.url), { type: 'module' });
    this.seq = 0;
    this.pending = new Map();
    this.worker.onmessage = ({ data }) => {
      this.pending.get(data.id)?.(data);
      this.pending.delete(data.id);
    };
    this.header = new Int32Array(new SharedArrayBuffer(16));
    this.cap = 0;
    this.array = new Float32Array(0);
    this.version = 0;
  }
  call(type, payload = {}) {
    const id = ++this.seq;
    return new Promise((res) => {
      this.pending.set(id, res);
      this.worker.postMessage({ id, type, ...payload });
    });
  }
  async init() {
    await this.call('init', { ref: this.ref });
    await this.ensure(40000);
  }
  async ensure(n) {
    if (n <= this.cap) return;
    this.cap = Math.ceil(n * 1.25);
    this.sab = new SharedArrayBuffer(this.cap * STRIDE * 4);
    this.array = new Float32Array(this.sab);
    this.version++;
    await this.call('buffer', { sab: this.sab, cap: this.cap, header: this.header.buffer });
  }
  setClock(clock) {
    this.worker.postMessage({ type: 'clock', clock });
  }
  tick() {}
  get count() {
    return Atomics.load(this.header, 0);
  }
  get computeMs() {
    return Atomics.load(this.header, 2) / 100;
  }
  async after(r) {
    await this.ensure(r.count);
    return r;
  }
  clear(which) { return this.call('clear', { which }).then((r) => this.after(r)); }
  loadTLE(text, kind) { return this.call('tle', { text, kind }).then((r) => this.after(r)); }
  synth(debris, active, seed) { return this.call('synth', { debris, active, seed }).then((r) => this.after(r)); }
  counts() { return this.call('counts').then((r) => r.counts); }
  pick(vpCam, args) { return this.call('pick', { vpCam, args }).then((r) => r.index); }
  info(index) { return this.call('info', { index }); }
}

class LocalEngine {
  constructor(wasm, ref) {
    this.mode = 'Main thread (no cross-origin isolation)';
    this.w = wasm;
    this.ref = ref;
    this.budget = 6000;
    this.computeMs = 0;
    this.version = 0;
    this._buf = null;
    this._ptr = 0;
    this.clock = { simBase: Date.now(), absBase: performance.timeOrigin + performance.now(), speed: 1 };
  }
  async init() {}
  setClock(clock) {
    this.clock = clock;
  }
  simMs() {
    const c = this.clock;
    return c.simBase + (performance.timeOrigin + performance.now() - c.absBase) * c.speed;
  }
  /** Adaptive round-robin: spend ~5 ms/frame; the vertex shader extrapolates the rest. */
  tick(full = false) {
    const sm = this.simMs();
    const t0 = performance.now();
    const n = this.w.x.update(julian(sm), (sm - this.ref) / 1000, full ? 0 : this.budget);
    const dt = performance.now() - t0;
    this.computeMs = dt;
    if (!full && n) this.budget = Math.max(1500, Math.min(60000, Math.round(this.budget * (dt > 0 ? Math.min(1.3, 5 / dt) : 1.3))));
  }
  get count() {
    return this.w.x.obj_count();
  }
  /** Zero-copy view; re-created when WASM memory grows or the Vec moves. */
  get array() {
    const ptr = this.w.x.inst_ptr();
    const n = this.count;
    if (this._buf !== this.w.mem || this._ptr !== ptr || this._n !== n) {
      this._buf = this.w.mem;
      this._ptr = ptr;
      this._n = n;
      this._arr = this.w.f32(ptr, n * STRIDE);
      this.version++;
    }
    return this._arr;
  }
  get cap() {
    return this.count;
  }
  done() {
    this.tick(true);
    return { count: this.count };
  }
  async clear(which) { this.w.x.obj_clear(which); return this.done(); }
  async loadTLE(text, kind) { const added = this.w.x.tle_load(this.w.put(text), kind); return { added, ...this.done() }; }
  async synth(debris, active, seed) {
    const jd = julian(this.simMs());
    const added = (debris ? this.w.x.synth_debris(debris, seed, jd) : 0) + (active ? this.w.x.synth_active(seed, jd) : 0);
    return { added, ...this.done() };
  }
  async counts() { this.w.x.obj_counts(); return Array.from(this.w.scratch(6)); }
  async pick(vpCam, args) { this.w.scratch(19).set(vpCam); return this.w.x.pick(...args); }
  async info(index) {
    const ok = this.w.x.obj_info(index, julian(this.simMs()));
    return { ok, vals: Array.from(this.w.scratch(12)), name: this.w.str(this.w.x.name_ptr(index), this.w.x.name_len(index)) };
  }
}

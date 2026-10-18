// Orbit propagation worker: owns the object catalogue, runs full SGP4 passes in WASM and
// publishes the interleaved instance buffer into a SharedArrayBuffer read by the renderer.
import { loadWasm, julian } from './wasm.js';

let w, sab, view, header, ref = 0, clock = { simBase: Date.now(), absBase: 0, speed: 1 }, cap = 0;

const now = () => performance.timeOrigin + performance.now();
const simMs = () => clock.simBase + (now() - clock.absBase) * clock.speed;

function publish() {
  const n = w.x.obj_count();
  if (n > cap) return; // main thread will reallocate
  const src = w.f32(w.x.inst_ptr(), n * 8);
  view.set(src);
  Atomics.store(header, 0, n);
  Atomics.add(header, 1, 1);
}

function loop() {
  if (w && view) {
    const t0 = performance.now();
    const sm = simMs();
    w.x.update(julian(sm), (sm - ref) / 1000, 0);
    publish();
    Atomics.store(header, 2, Math.round((performance.now() - t0) * 100));
  }
  setTimeout(loop, 8);
}

onmessage = async ({ data: m }) => {
  const reply = (r) => postMessage({ id: m.id, ...r });
  switch (m.type) {
    case 'init':
      w = await loadWasm(new URL('../earth.wasm', import.meta.url));
      ref = m.ref;
      reply({});
      loop();
      break;
    case 'buffer':
      sab = m.sab;
      cap = m.cap;
      view = new Float32Array(sab, 0, cap * 8);
      header = new Int32Array(m.header);
      reply({});
      break;
    case 'clock':
      clock = m.clock;
      break;
    case 'clear':
      w.x.obj_clear(m.which);
      reply({ count: w.x.obj_count() });
      break;
    case 'tle':
      reply({ added: w.x.tle_load(w.put(m.text), m.kind), count: w.x.obj_count() });
      break;
    case 'synth': {
      const jd = julian(simMs());
      const d = m.debris ? w.x.synth_debris(m.debris, m.seed, jd) : 0;
      const a = m.active ? w.x.synth_active(m.seed, jd) : 0;
      reply({ added: d + a, count: w.x.obj_count() });
      break;
    }
    case 'counts':
      w.x.obj_counts();
      reply({ counts: Array.from(w.scratch(6)) });
      break;
    case 'pick': {
      w.scratch(19).set(m.vpCam);
      reply({ index: w.x.pick(...m.args) });
      break;
    }
    case 'info': {
      const ok = w.x.obj_info(m.index, julian(simMs()));
      const vals = Array.from(w.scratch(12));
      reply({ ok, vals, name: w.str(w.x.name_ptr(m.index), w.x.name_len(m.index)) });
      break;
    }
  }
};

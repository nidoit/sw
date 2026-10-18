// Thin bridge over earth.wasm (plain C-ABI exports, no bindgen).
// Text/pixels go in through io_alloc; results come back as typed-array views over linear memory.

const enc = new TextEncoder();
const dec = new TextDecoder();

export async function loadWasm(url = new URL('../earth.wasm', import.meta.url)) {
  const res = await fetch(url);
  const { instance } = WebAssembly.instantiateStreaming && res.headers.get('content-type')?.includes('wasm')
    ? await WebAssembly.instantiateStreaming(res, {})
    : await WebAssembly.instantiate(await res.arrayBuffer(), {});
  return new Wasm(instance.exports);
}

export class Wasm {
  constructor(x) {
    this.x = x;
  }
  get mem() {
    return this.x.memory.buffer;
  }
  f32(ptr, len) {
    return new Float32Array(this.mem, ptr, len);
  }
  u8(ptr, len) {
    return new Uint8Array(this.mem, ptr, len);
  }
  scratch(n = 16) {
    return this.f32(this.x.scratch_ptr(), n);
  }
  /** Copies a string or bytes into WASM memory; returns the byte length to pass to the parser. */
  put(data) {
    const bytes = typeof data === 'string' ? enc.encode(data) : data;
    const ptr = this.x.io_alloc(bytes.length);
    this.u8(ptr, bytes.length).set(bytes);
    return bytes.length;
  }
  str(ptr, len) {
    return len ? dec.decode(this.u8(ptr, len)) : '';
  }
}

export function julian(ms) {
  return ms / 86400000 + 2440587.5;
}

// Three.js scene layers. All geometry lives in the Earth-fixed frame produced by earth.wasm
// (+X = 0°N 0°E, +Y = north pole, −Z = 90°E); the Sun moves, the Earth mesh stays still.
import * as THREE from 'three';

export const R_CLOUD = 1.012;

const SURFACE_VS = /* glsl */ `
  varying vec2 vUv; varying vec3 vN; varying vec3 vW;
  void main() {
    vUv = uv;
    vN = normalize(mat3(modelMatrix) * normal);
    vec4 w = modelMatrix * vec4(position, 1.0);
    vW = w.xyz;
    gl_Position = projectionMatrix * viewMatrix * w;
  }`;

export function makeEarth(tex) {
  const mat = new THREE.ShaderMaterial({
    uniforms: {
      dayMap: { value: tex.day },
      nightMap: { value: tex.night },
      bumpMap: { value: tex.bump },
      specMap: { value: tex.spec },
      sunDir: { value: new THREE.Vector3(1, 0, 0) },
      uNight: { value: 1 },
      bumpScale: { value: 3.0 },
      texel: { value: new THREE.Vector2(1 / 2048, 1 / 1024) },
    },
    vertexShader: SURFACE_VS,
    fragmentShader: /* glsl */ `
      uniform sampler2D dayMap, nightMap, bumpMap, specMap;
      uniform vec3 sunDir; uniform float uNight, bumpScale; uniform vec2 texel;
      varying vec2 vUv; varying vec3 vN; varying vec3 vW;
      void main() {
        vec3 N = normalize(vN);
        vec3 up = vec3(0.0, 1.0, 0.0);
        vec3 T = length(cross(up, N)) < 1e-3 ? vec3(1.0, 0.0, 0.0) : normalize(cross(up, N));
        vec3 B = cross(N, T);
        float h = texture2D(bumpMap, vUv).r;
        float hu = texture2D(bumpMap, vUv + vec2(texel.x, 0.0)).r;
        float hv = texture2D(bumpMap, vUv + vec2(0.0, texel.y)).r;
        vec3 Np = normalize(N - bumpScale * ((hu - h) * T + (hv - h) * B));
        float d = dot(N, sunDir);
        float day = smoothstep(-0.12, 0.18, d);
        vec3 albedo = texture2D(dayMap, vUv).rgb;
        vec3 lit = albedo * (0.05 + 1.05 * max(dot(Np, sunDir), 0.0));
        vec3 V = normalize(cameraPosition - vW);
        vec3 Hh = normalize(V + sunDir);
        float spec = texture2D(specMap, vUv).r * pow(max(dot(N, Hh), 0.0), 45.0) * 0.55;
        lit += vec3(1.0, 0.94, 0.82) * spec * day;
        vec3 lights = pow(texture2D(nightMap, vUv).rgb, vec3(1.4)) * vec3(1.0, 0.85, 0.6) * 1.8 * uNight;
        vec3 col = lit * day + (albedo * 0.025 + lights) * (1.0 - day);
        float fres = pow(1.0 - max(dot(N, V), 0.0), 3.0);
        col += vec3(0.3, 0.55, 1.0) * fres * 0.55 * (0.2 + day);
        gl_FragColor = vec4(col, 1.0);
      }`,
  });
  const mesh = new THREE.Mesh(new THREE.SphereGeometry(1, 192, 96), mat);
  return mesh;
}

export function makeAtmosphere() {
  const mat = new THREE.ShaderMaterial({
    uniforms: { sunDir: { value: new THREE.Vector3(1, 0, 0) } },
    vertexShader: SURFACE_VS,
    fragmentShader: /* glsl */ `
      uniform vec3 sunDir; varying vec3 vN; varying vec3 vW;
      void main() {
        vec3 N = normalize(vN);
        vec3 V = normalize(cameraPosition - vW);
        float rim = pow(1.0 - abs(dot(N, V)), 2.5);
        float lit = smoothstep(-0.35, 0.4, dot(N, sunDir));
        gl_FragColor = vec4(vec3(0.35, 0.6, 1.0) * rim * (0.15 + 0.9 * lit), rim);
      }`,
    side: THREE.BackSide,
    blending: THREE.AdditiveBlending,
    transparent: true,
    depthWrite: false,
  });
  return new THREE.Mesh(new THREE.SphereGeometry(1.06, 96, 48), mat);
}

export function makeClouds(tex) {
  const mat = new THREE.ShaderMaterial({
    uniforms: {
      map: { value: tex },
      useAlpha: { value: 0 },
      sunDir: { value: new THREE.Vector3(1, 0, 0) },
    },
    vertexShader: SURFACE_VS,
    fragmentShader: /* glsl */ `
      uniform sampler2D map; uniform float useAlpha; uniform vec3 sunDir;
      varying vec2 vUv; varying vec3 vN;
      void main() {
        vec4 t = texture2D(map, vUv);
        float a = useAlpha > 0.5 ? t.a * max(t.r, 0.6) : t.r;
        float lit = 0.08 + 0.95 * smoothstep(-0.12, 0.3, dot(normalize(vN), sunDir));
        gl_FragColor = vec4(vec3(lit), a * 0.92);
      }`,
    transparent: true,
    depthWrite: false,
  });
  return new THREE.Mesh(new THREE.SphereGeometry(R_CLOUD, 128, 64), mat);
}

/** Generic surface overlay (AQI heat-map, SST anomaly, …) with optional ocean mask & pulse.
 *  `bounds` = [west, south, east, north] places a regional plate-carrée texture; default is the whole globe. */
export function makeOverlay(radius, tex, { waterMask = null, pulse = 0, opacity = 1, bounds = null } = {}) {
  const b = bounds || [-180, -90, 180, 90];
  const mat = new THREE.ShaderMaterial({
    uniforms: {
      map: { value: tex },
      water: { value: waterMask },
      useMask: { value: waterMask ? 1 : 0 },
      uTime: { value: 0 },
      uPulse: { value: pulse },
      uOpacity: { value: opacity },
      uBounds: { value: new THREE.Vector4((b[0] + 180) / 360, (b[1] + 90) / 180, (b[2] - b[0]) / 360, (b[3] - b[1]) / 180) },
    },
    vertexShader: SURFACE_VS,
    fragmentShader: /* glsl */ `
      uniform sampler2D map, water; uniform float useMask, uTime, uPulse, uOpacity; uniform vec4 uBounds;
      varying vec2 vUv;
      void main() {
        vec2 st = (vUv - uBounds.xy) / uBounds.zw;
        if (any(lessThan(st, vec2(0.0))) || any(greaterThan(st, vec2(1.0)))) discard;
        vec4 t = texture2D(map, st);
        float m = useMask > 0.5 ? smoothstep(0.3, 0.7, texture2D(water, vUv).r) : 1.0;
        float wave = 1.0 - uPulse * (0.5 + 0.5 * sin(uTime * 1.3 + vUv.x * 60.0 + sin(vUv.y * 40.0 + uTime) * 2.0));
        gl_FragColor = vec4(t.rgb, t.a * m * uOpacity * (0.55 + 0.45 * wave));
      }`,
    transparent: true,
    depthWrite: false,
  });
  return new THREE.Mesh(new THREE.SphereGeometry(radius, 128, 64), mat);
}

/** Interleaved stride-5 particle cloud: position(3), a, b — shader decides the look. */
export function makeParticles(kind, capacity) {
  const array = new Float32Array(Math.max(1, capacity) * 5);
  const ib = new THREE.InterleavedBuffer(array, 5).setUsage(THREE.DynamicDrawUsage);
  const g = new THREE.BufferGeometry();
  g.setAttribute('position', new THREE.InterleavedBufferAttribute(ib, 3, 0));
  g.setAttribute('a', new THREE.InterleavedBufferAttribute(ib, 1, 3));
  g.setAttribute('b', new THREE.InterleavedBufferAttribute(ib, 1, 4));
  g.setDrawRange(0, 0);
  const frag = {
    // a = intensity, b = altitude fraction → green (low) to crimson (high) curtains
    aurora: `
      vec3 c = mix(vec3(0.15, 1.0, 0.45), vec3(0.95, 0.2, 0.55), smoothstep(0.45, 1.0, vB));
      float flick = 0.65 + 0.35 * sin(uTime * 2.3 + vP.x * 40.0 + vP.z * 33.0);
      gl_FragColor = vec4(c * vA * flick * soft * 0.55, 1.0);`,
    // a = AQI, b = fade
    dust: `
      vec3 c = aqi(vA);
      gl_FragColor = vec4(c * soft * vB * 0.9, 1.0);`,
    // a = FRP (0..1), b = phase
    fire: `
      float fl = 0.6 + 0.4 * sin(uTime * 7.0 + vB) * sin(uTime * 3.1 + vB * 2.0);
      vec3 c = mix(vec3(1.0, 0.25, 0.02), vec3(1.0, 0.85, 0.3), vA);
      gl_FragColor = vec4(c * soft * fl * (0.5 + vA), 1.0);`,
  }[kind];
  const size = { aurora: 2.6, dust: 2.2, fire: 4.0 }[kind];
  const mat = new THREE.ShaderMaterial({
    uniforms: { uTime: { value: 0 }, uSize: { value: size }, uScale: { value: 1 } },
    vertexShader: /* glsl */ `
      attribute float a; attribute float b;
      uniform float uSize, uScale, uTime;
      varying float vA, vB; varying vec3 vP;
      void main() {
        vA = a; vB = b; vP = position;
        vec4 mv = modelViewMatrix * vec4(position, 1.0);
        gl_Position = projectionMatrix * mv;
        gl_PointSize = uSize * uScale * clamp(4.0 / -mv.z, 0.6, 3.0);
      }`,
    fragmentShader: /* glsl */ `
      uniform float uTime; varying float vA, vB; varying vec3 vP;
      vec3 aqi(float x) {
        if (x < 50.0) return mix(vec3(0.0, 0.85, 0.35), vec3(0.35, 0.9, 0.0), x / 50.0);
        if (x < 100.0) return mix(vec3(0.35, 0.9, 0.0), vec3(1.0, 0.92, 0.0), (x - 50.0) / 50.0);
        if (x < 150.0) return mix(vec3(1.0, 0.92, 0.0), vec3(1.0, 0.5, 0.0), (x - 100.0) / 50.0);
        if (x < 200.0) return mix(vec3(1.0, 0.5, 0.0), vec3(1.0, 0.0, 0.0), (x - 150.0) / 50.0);
        if (x < 300.0) return mix(vec3(1.0, 0.0, 0.0), vec3(0.56, 0.25, 0.6), (x - 200.0) / 100.0);
        return vec3(0.5, 0.0, 0.14);
      }
      void main() {
        float d = length(gl_PointCoord - 0.5) * 2.0;
        if (d > 1.0) discard;
        float soft = pow(1.0 - d, 1.8);
        ${frag}
      }`,
    blending: THREE.AdditiveBlending,
    transparent: true,
    depthWrite: false,
  });
  const pts = new THREE.Points(g, mat);
  pts.frustumCulled = false;
  pts.userData = { ib, capacity };
  return pts;
}

/** Copies stride-5 data into a particle layer, growing its buffer when needed. */
export function setParticles(pts, src, count) {
  let { ib, capacity } = pts.userData;
  if (count > capacity) {
    const fresh = new THREE.InterleavedBuffer(new Float32Array(count * 5), 5).setUsage(THREE.DynamicDrawUsage);
    const g = pts.geometry;
    g.setAttribute('position', new THREE.InterleavedBufferAttribute(fresh, 3, 0));
    g.setAttribute('a', new THREE.InterleavedBufferAttribute(fresh, 1, 3));
    g.setAttribute('b', new THREE.InterleavedBufferAttribute(fresh, 1, 4));
    pts.userData = { ib: (ib = fresh), capacity: (capacity = count) };
  }
  ib.array.set(src.subarray(0, count * 5));
  ib.needsUpdate = true;
  pts.geometry.setDrawRange(0, count);
}

/**
 * Orbital objects: THREE.InstancedMesh of camera-facing quads. Per-instance data is the
 * interleaved WASM buffer itself (x,y,z,t | vx,vy,vz,cat); the vertex shader extrapolates
 * position by velocity to the current time, so the CPU never touches individual instances.
 */
export function makeOrbitals() {
  const base = new THREE.PlaneGeometry(2, 2);
  const mat = new THREE.ShaderMaterial({
    uniforms: {
      uTime: { value: 0 },
      uSize: { value: 1.6 },
      uSel: { value: -1 },
      uMask: { value: new THREE.Vector4(1, 1, 1, 1) },
      uViewport: { value: new THREE.Vector2(1, 1) },
    },
    vertexShader: /* glsl */ `
      attribute vec4 iA; attribute vec4 iB;
      uniform float uTime, uSize, uSel; uniform vec4 uMask; uniform vec2 uViewport;
      varying vec3 vCol; varying vec2 vQ;
      void main() {
        float c = iB.w;
        float vis = c < 0.5 ? uMask.x : c < 1.5 ? uMask.y : c < 2.5 ? uMask.z : c < 4.5 ? uMask.w : 0.0;
        vec3 col = c < 0.5 ? vec3(1.0, 0.2, 0.16) : c < 1.5 ? vec3(1.0, 0.55, 0.08)
                 : c < 2.5 ? vec3(1.0, 0.93, 0.22) : c < 3.5 ? vec3(0.25, 0.85, 1.0) : vec3(0.3, 1.0, 0.45);
        float sel = abs(float(gl_InstanceID) - uSel) < 0.5 ? 1.0 : 0.0;
        float s = uSize * (c > 2.5 ? (c > 3.5 ? 3.2 : 1.35) : 1.0) * (1.0 + sel * 3.0);
        vec3 p = iA.xyz + iB.xyz * (uTime - iA.w);
        vec4 clip = projectionMatrix * modelViewMatrix * vec4(p, 1.0);
        clip.xy += position.xy * s / uViewport * 2.0 * clip.w;
        gl_Position = vis > 0.5 ? clip : vec4(2.0, 2.0, 2.0, 1.0);
        vCol = sel > 0.5 ? vec3(1.0) : col;
        vQ = position.xy;
      }`,
    fragmentShader: /* glsl */ `
      varying vec3 vCol; varying vec2 vQ;
      void main() {
        float d = length(vQ);
        if (d > 1.0) discard;
        float a = pow(1.0 - d, 1.5);
        gl_FragColor = vec4(vCol * a * 1.5, 1.0);
      }`,
    blending: THREE.AdditiveBlending,
    transparent: true,
    depthWrite: false,
  });
  const mesh = new THREE.InstancedMesh(base, mat, 1);
  mesh.count = 0;
  mesh.frustumCulled = false;
  mesh.userData.version = -1;
  return mesh;
}

/** Binds a (possibly shared) Float32Array of stride-8 instance data to the InstancedMesh. */
export function bindOrbitals(mesh, array) {
  const ib = new THREE.InstancedInterleavedBuffer(array, 8).setUsage(THREE.DynamicDrawUsage);
  const g = mesh.geometry;
  g.setAttribute('iA', new THREE.InterleavedBufferAttribute(ib, 4, 0));
  g.setAttribute('iB', new THREE.InterleavedBufferAttribute(ib, 4, 4));
  mesh.userData.ib = ib;
}

export function makeIce(waterMap) {
  const mat = new THREE.ShaderMaterial({
    uniforms: { water: { value: waterMap }, sunDir: { value: new THREE.Vector3(1, 0, 0) } },
    vertexShader: /* glsl */ `
      varying vec3 vP;
      void main() { vP = position; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
    fragmentShader: /* glsl */ `
      uniform sampler2D water; uniform vec3 sunDir; varying vec3 vP;
      void main() {
        vec3 n = normalize(vP);
        float lat = asin(clamp(n.y, -1.0, 1.0));
        float lon = atan(-n.z, n.x);
        vec2 uv = vec2((lon + 3.14159265) / 6.2831853, 0.5 + lat / 3.14159265);
        float sea = smoothstep(0.3, 0.7, texture2D(water, uv).r);
        float lit = 0.25 + 0.75 * smoothstep(-0.15, 0.25, dot(n, sunDir));
        gl_FragColor = vec4(vec3(0.93, 0.97, 1.0) * lit, 0.88 * sea);
      }`,
    transparent: true,
    depthWrite: false,
  });
  const group = new THREE.Group();
  group.userData.mat = mat;
  return group;
}

/** Rebuilds sea-ice meshes from the WASM vertex block [N cur, N base, S cur, S base]. */
export function setIce(group, verts, rings, seg) {
  group.children.slice().forEach((c) => { group.remove(c); c.geometry.dispose(); });
  const per = (rings + 1) * (seg + 1) * 3;
  const idx = [];
  for (let r = 0; r < rings; r++)
    for (let s = 0; s < seg; s++) {
      const a = r * (seg + 1) + s, b = a + seg + 1;
      idx.push(a, b, a + 1, b, b + 1, a + 1);
    }
  const lineMat = new THREE.LineDashedMaterial({ color: 0xffd36b, dashSize: 0.02, gapSize: 0.012, transparent: true, opacity: 0.95 });
  for (let k = 0; k < 4; k++) {
    const block = verts.slice(k * per, (k + 1) * per);
    if (k % 2 === 0) {
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(block, 3));
      g.setIndex(idx);
      group.add(new THREE.Mesh(g, group.userData.mat));
    } else {
      const edge = block.slice(rings * (seg + 1) * 3);
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(edge, 3));
      const line = new THREE.Line(g, lineMat);
      line.computeLineDistances();
      group.add(line);
    }
  }
}

export function makeNeo() {
  const group = new THREE.Group();
  group.userData.lineMat = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending, depthWrite: false });
  group.userData.dotMat = new THREE.PointsMaterial({ vertexColors: true, size: 7, sizeAttenuation: false, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending });
  return group;
}

export function setNeo(group, lines, now, hazardous, perObj) {
  group.children.slice().forEach((c) => { group.remove(c); c.geometry.dispose(); });
  const n = hazardous.length;
  const idx = [], col = new Float32Array(n * perObj * 3), dcol = new Float32Array(n * 3);
  for (let k = 0; k < n; k++) {
    const c = hazardous[k] ? [1, 0.12, 0.1] : [0.65, 0.12, 0.12];
    for (let i = 0; i < perObj; i++) {
      const fade = 0.35 + 0.65 * Math.sin((i / (perObj - 1)) * Math.PI);
      col.set(c.map((v) => v * fade), (k * perObj + i) * 3);
      if (i < perObj - 1) idx.push(k * perObj + i, k * perObj + i + 1);
    }
    dcol.set(hazardous[k] ? [1, 0.35, 0.25] : [1, 0.6, 0.5], k * 3);
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute('position', new THREE.BufferAttribute(lines, 3));
  g.setAttribute('color', new THREE.BufferAttribute(col, 3));
  g.setIndex(idx);
  group.add(new THREE.LineSegments(g, group.userData.lineMat));
  const d = new THREE.BufferGeometry();
  d.setAttribute('position', new THREE.BufferAttribute(now, 3));
  d.setAttribute('color', new THREE.BufferAttribute(dcol, 3));
  group.add(new THREE.Points(d, group.userData.dotMat));
  group.userData.now = now;
}

export function makePin() {
  const g = new THREE.Group();
  const head = new THREE.Mesh(new THREE.SphereGeometry(0.012, 20, 12), new THREE.MeshBasicMaterial({ color: 0xff3d6e }));
  head.position.y = 0.05;
  const stem = new THREE.Mesh(new THREE.CylinderGeometry(0.0022, 0.0006, 0.05, 8), new THREE.MeshBasicMaterial({ color: 0xffffff }));
  stem.position.y = 0.025;
  const ringMat = new THREE.MeshBasicMaterial({ color: 0xff3d6e, transparent: true, side: THREE.DoubleSide, depthWrite: false });
  const ring = new THREE.Mesh(new THREE.RingGeometry(0.8, 1, 48), ringMat);
  ring.rotation.x = -Math.PI / 2;
  ring.position.y = 0.001;
  const glow = new THREE.Mesh(new THREE.SphereGeometry(0.022, 16, 10), new THREE.MeshBasicMaterial({ color: 0xff3d6e, transparent: true, opacity: 0.35, blending: THREE.AdditiveBlending, depthWrite: false }));
  glow.position.y = 0.05;
  g.add(head, stem, ring, glow);
  g.userData = { ring, glow };
  return g;
}

export function placeOnSurface(obj, lat, lon) {
  const la = THREE.MathUtils.degToRad(lat), lo = THREE.MathUtils.degToRad(lon);
  const n = new THREE.Vector3(Math.cos(la) * Math.cos(lo), Math.sin(la), -Math.cos(la) * Math.sin(lo));
  obj.position.copy(n);
  obj.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), n);
  return n;
}

/** Population on the H3 grid: one hexagonal prism per cell (InstancedMesh, single draw call). */
export function makeHexColumns() {
  const geo = new THREE.CylinderGeometry(1, 1, 1, 6, 1);
  geo.translate(0, 0.5, 0); // base on the surface
  const mat = new THREE.ShaderMaterial({
    uniforms: { sunDir: { value: new THREE.Vector3(1, 0, 0) } },
    vertexShader: /* glsl */ `
      attribute float aVal;
      uniform vec3 sunDir;
      varying vec3 vCol; varying float vShade;
      vec3 ramp(float t) { // dark violet → magenta → orange → pale yellow
        vec3 a = vec3(0.23, 0.05, 0.42), b = vec3(0.86, 0.16, 0.47), c = vec3(1.0, 0.55, 0.12), d = vec3(1.0, 0.96, 0.62);
        return t < 0.4 ? mix(a, b, t / 0.4) : t < 0.75 ? mix(b, c, (t - 0.4) / 0.35) : mix(c, d, (t - 0.75) / 0.25);
      }
      void main() {
        vec4 w = modelMatrix * instanceMatrix * vec4(position, 1.0);
        vec3 n = normalize(mat3(modelMatrix) * mat3(instanceMatrix) * normal);
        vec3 up = normalize((modelMatrix * instanceMatrix * vec4(0.0, 0.0, 0.0, 1.0)).xyz);
        float day = smoothstep(-0.25, 0.3, dot(up, sunDir));
        vShade = (0.55 + 0.45 * max(dot(n, normalize(up + sunDir * 0.6)), 0.0)) * (0.55 + 0.45 * day);
        vCol = ramp(aVal) * (position.y > 0.99 ? 1.15 : 1.0);
        gl_Position = projectionMatrix * viewMatrix * w;
      }`,
    fragmentShader: /* glsl */ `
      varying vec3 vCol; varying float vShade;
      void main() { gl_FragColor = vec4(vCol * vShade, 1.0); }`,
  });
  const mesh = new THREE.InstancedMesh(geo, mat, 1);
  mesh.count = 0;
  mesh.frustumCulled = false;
  return mesh;
}

/** cells: Float32Array of [lat, lon, people, rotation, radiusKm] records (see code/data/build_population.py). */
export function setHexColumns(old, cells) {
  const n = cells.length / 5;
  const mesh = new THREE.InstancedMesh(old.geometry, old.material, n);
  mesh.frustumCulled = false;
  const val = new Float32Array(n);
  const max = Math.max(...Array.from({ length: Math.min(n, 64) }, (_, i) => cells[i * 5 + 2]));
  const m = new THREE.Matrix4(), up = new THREE.Vector3(), north = new THREE.Vector3(), east = new THREE.Vector3();
  const vx = new THREE.Vector3(), vz = new THREE.Vector3(), Y = new THREE.Vector3(0, 1, 0);
  for (let i = 0; i < n; i++) {
    const [lat, lon, pop, rot, rKm] = cells.subarray(i * 5, i * 5 + 5);
    const la = THREE.MathUtils.degToRad(lat), lo = THREE.MathUtils.degToRad(lon);
    up.set(Math.cos(la) * Math.cos(lo), Math.sin(la), -Math.cos(la) * Math.sin(lo));
    north.copy(Y).addScaledVector(up, -up.y).normalize();
    east.crossVectors(north, up);
    vz.copy(north).multiplyScalar(Math.cos(rot)).addScaledVector(east, Math.sin(rot)); // first H3 vertex
    vx.crossVectors(up, vz);
    const t = Math.pow(pop / max, 0.55);
    const r = (rKm / 6371) * 0.9, h = 0.0015 + 0.15 * t;
    m.makeBasis(vx.multiplyScalar(r), up.clone().multiplyScalar(h), vz.multiplyScalar(r)).setPosition(up.multiplyScalar(1.0005));
    mesh.setMatrixAt(i, m);
    val[i] = Math.pow(pop / max, 0.35);
  }
  mesh.geometry = old.geometry.clone();
  mesh.geometry.setAttribute('aVal', new THREE.InstancedBufferAttribute(val, 1));
  mesh.instanceMatrix.needsUpdate = true;
  mesh.visible = old.visible;
  mesh.userData = { cells, max };
  old.parent?.add(mesh);
  old.parent?.remove(old);
  return mesh;
}

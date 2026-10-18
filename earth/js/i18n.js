// UI language (Korean / English). Static markup uses data-i18n* attributes; code calls t(key, ...args).
const n = (v) => v.toLocaleString();

const TEXT = {
  ko: {
    title: '3D 통합 지구/우주 환경 실시간 모니터링 — Three.js + Rust WebAssembly',
    globe: '3D 지구',
    bootLoading: 'WASM 모듈과 텍스처를 불러오는 중…',
    bootTextures: '지구 텍스처 준비 중…',
    bootSynth: 'WASM: 궤도 파편 개체군 생성 중…',
    bootFail: (m) => `초기화 실패: ${m}. 로컬 파일(file://)이 아닌 HTTP 서버로 열어 주세요 (node code/server/server.mjs).`,
    panelBtn: '레이어 패널', panel: '레이어 제어', langBtn: 'English',
    speed: '시뮬레이션 배속', realtime: '실시간 ×1', perMin: '×60 (1분/초)', perHour: '×3600 (1시간/초)',
    now: '지금', nowTitle: '현재 시각으로',
    gLocation: '위치', gWeather: '기상 · 대기', gClimate: '기후 위기 · 환경 한계선', gSpace: '우주 환경',
    myLocation: '내 위치', clouds: '구름', air: '대기오염 (AQI)', aurora: '오로라',
    sst: '해수면 온도 이상', fires: '실시간 산불', ice: '해빙 면적', night: '야간 불빛',
    leo: 'LEO 파편', meo: 'MEO 파편', geo: 'GEO 파편', sats: '인공위성', neo: '지구 근접 천체',
    legend: '범례 · 상세', settings: '설정 · API 키',
    kOwm: 'OpenWeatherMap 키 (구름)', kWaqi: 'WAQI 토큰 (대기질)', kFirms: 'NASA FIRMS MAP_KEY (산불)', kNasa: 'NASA API 키 (NEO)',
    kTle: 'TLE URL (Space-Track 프록시 등)', kWs: 'WebSocket URL', kDebris: '합성 파편 수',
    optional: '선택', optWaqi: '선택 · 없으면 CAMS', defTle: '기본: CelesTrak', defWs: '기본: 백엔드 /ws',
    save: '저장 후 다시 불러오기',
    keyHint: '키는 이 브라우저(localStorage)에만 저장됩니다. 백엔드(code/server/server.mjs)를 쓰면 키를 서버 환경변수로 둘 수 있습니다.',
    // status chips
    locating: '위치 확인', defaultLoc: '기본값', synthPop: '합성 개체군', tleLoading: 'TLE 수신', staticTex: '정적 텍스처',
    owmTiles: 'OWM 타일', model: 'WASM 모델', kp3: '모델 (Kp 3)', synth: '합성', seasonal: '계절 모델', trend: '추세 모델',
    // details
    loading: '로딩 중…', noBackend: '없음 (API 직접 호출)', backendOn: '연결됨 (캐싱 프록시)',
    tleOk: (added, srcs, cached, debris) => `실측 TLE ${n(added)}개 (${srcs})${cached ? ' · 캐시' : ''} + 합성 파편 ${n(debris)}개`,
    tleFail: (e, debris) => `TLE 수신 실패(${e}) → WASM 합성 개체군(파편 ${n(debris)} + 위성군 Walker shell)`,
    auroraFail: (e) => `NOAA 연결 실패(${e}) → WASM 통계 오벌(Kp 3)`,
    auroraStats: (src, p, cnt) => `${src} · 최대 확률 ${p}% · 입자 ${n(cnt)}`,
    airSrc: (src, cnt) => `${src} · 관측점 ${n(cnt)}`,
    airFail: (e) => `수신 실패(${e}) → 합성 도시 핫스팟`,
    airStats: (src, avg, max) => `${src} · 평균 AQI ${avg}, 최대 ${max}`,
    cloudsNoKey: 'OpenWeatherMap 키 없음 → 정적 위성 구름 합성 텍스처 (설정에서 키 입력 시 실시간)',
    cloudsOk: (z, time) => `OpenWeatherMap clouds_new z${z} → WASM 메르카토르→정거원통 재투영 (${time})`,
    cloudsFail: (e) => `OWM 실패(${e}) → 정적 텍스처`,
    sstOk: 'NASA GIBS · GHRSST L4 MUR 해수면 온도 이상 (2일 지연)',
    sstFail: (e) => `GIBS 실패(${e}) → WASM 절차적 이상장 (북대서양·북동태평양 해양열파, ENSO +1.1)`,
    firesOk: (cnt) => `NASA FIRMS VIIRS S-NPP NRT · 24시간 화재 ${n(cnt)}점`,
    firesFail: (e, cnt) => `FIRMS 불가(${e}) → WASM 계절 화재대 모델 ${n(cnt)}점`,
    iceSrc: (live) => (live ? 'NSIDC Sea Ice Index v3' : 'NSIDC 불가 → 기후값+추세 모델'),
    iceBody: (src, arc, arcRef, ant, antRef) => `${src} · 단위 10⁶ km²<br>북극 ${arc} / 기준 ${arcRef}<br>남극 ${ant} / 기준 ${antRef}<br>`
      + '<small>흰 면 = 현재 해빙, 노란 점선 = 1981–2010 중앙값 경계 (면적 일치 근사 형상)</small>',
    neoOk: (cnt, haz) => `NASA NeoWs 7일 접근 ${cnt}개 (위험 ${haz}) · 궤적은 접근거리·속도 기반 모식도, 거리 로그 압축`,
    neoFail: (e) => `NeoWs 수신 실패: ${e} (DEMO_KEY는 시간당 30회 제한)`,
    dAir: '대기오염 (US AQI)', dSst: '해수면 온도 이상', dFires: '산불', dIce: '해빙', dAurora: '오로라', dClouds: '구름',
    dOrbit: '궤도 객체', dNeo: '지구 근접 천체', dBackend: '백엔드',
    orbitNote: 'LEO &lt;2,000 km · GEO 35,786±1,000 km · 그 외 MEO (평균 고도 기준). 클릭하면 상세 정보.',
    // tooltips
    close: '닫기', cats: ['LEO 파편', 'MEO 파편', 'GEO 파편', '활성 위성', '유인 우주정거장'],
    tCat: '분류', tAlt: '고도', tSpeed: '속도', tPos: '위치', tInc: '경사각', tPeriod: '주기', min: '분', tPgAp: '근/원지점',
    asteroid: '지구 근접 소행성', hazard: '잠재 위험', tClosest: '최근접', tDist: '거리', mkm: '백만 km',
    tRelV: '상대 속도', tDia: '추정 지름',
    hudObjects: '렌더 객체', mobile: '모바일',
  },
  en: {
    title: 'Real-time 3D monitor of Earth and near-space — Three.js + Rust WebAssembly',
    globe: '3D Earth',
    bootLoading: 'Loading the WASM module and textures…',
    bootTextures: 'Preparing Earth textures…',
    bootSynth: 'WASM: generating the orbital debris population…',
    bootFail: (m) => `Failed to start: ${m}. Open the page over HTTP, not as a local file:// (node code/server/server.mjs).`,
    panelBtn: 'Layer panel', panel: 'Layer controls', langBtn: '한국어',
    speed: 'Simulation speed', realtime: 'Real time ×1', perMin: '×60 (1 min/s)', perHour: '×3600 (1 h/s)',
    now: 'Now', nowTitle: 'Jump to the current time',
    gLocation: 'Location', gWeather: 'Weather · Atmosphere', gClimate: 'Climate crisis · Planetary boundaries', gSpace: 'Space environment',
    myLocation: 'My location', clouds: 'Clouds', air: 'Air pollution (AQI)', aurora: 'Aurora',
    sst: 'Sea surface temp. anomaly', fires: 'Active fires', ice: 'Sea ice extent', night: 'Night lights',
    leo: 'LEO debris', meo: 'MEO debris', geo: 'GEO debris', sats: 'Satellites', neo: 'Near-Earth objects',
    legend: 'Legend · Details', settings: 'Settings · API keys',
    kOwm: 'OpenWeatherMap key (clouds)', kWaqi: 'WAQI token (air quality)', kFirms: 'NASA FIRMS MAP_KEY (fires)', kNasa: 'NASA API key (NEO)',
    kTle: 'TLE URL (Space-Track proxy etc.)', kWs: 'WebSocket URL', kDebris: 'Synthetic debris count',
    optional: 'optional', optWaqi: 'optional · CAMS if empty', defTle: 'default: CelesTrak', defWs: 'default: backend /ws',
    save: 'Save and reload data',
    keyHint: 'Keys are stored only in this browser (localStorage). With the backend (code/server/server.mjs) you can keep them in server environment variables instead.',
    locating: 'locating', defaultLoc: 'default', synthPop: 'synthetic', tleLoading: 'fetching TLE', staticTex: 'static texture',
    owmTiles: 'OWM tiles', model: 'WASM model', kp3: 'model (Kp 3)', synth: 'synthetic', seasonal: 'seasonal model', trend: 'trend model',
    loading: 'Loading…', noBackend: 'none (direct API calls)', backendOn: 'connected (caching proxy)',
    tleOk: (added, srcs, cached, debris) => `${n(added)} real TLEs (${srcs})${cached ? ' · cached' : ''} + ${n(debris)} synthetic debris`,
    tleFail: (e, debris) => `TLE fetch failed (${e}) → WASM synthetic population (${n(debris)} debris + Walker satellite shells)`,
    auroraFail: (e) => `NOAA unreachable (${e}) → WASM statistical oval (Kp 3)`,
    auroraStats: (src, p, cnt) => `${src} · max probability ${p}% · ${n(cnt)} particles`,
    airSrc: (src, cnt) => `${src} · ${n(cnt)} stations`,
    airFail: (e) => `Fetch failed (${e}) → synthetic city hotspots`,
    airStats: (src, avg, max) => `${src} · mean AQI ${avg}, max ${max}`,
    cloudsNoKey: 'No OpenWeatherMap key → static composite cloud texture (enter a key in Settings for live clouds)',
    cloudsOk: (z, time) => `OpenWeatherMap clouds_new z${z} → WASM Mercator→equirectangular reprojection (${time})`,
    cloudsFail: (e) => `OWM failed (${e}) → static texture`,
    sstOk: 'NASA GIBS · GHRSST L4 MUR sea surface temperature anomaly (2-day lag)',
    sstFail: (e) => `GIBS failed (${e}) → WASM procedural anomaly field (N. Atlantic / NE Pacific marine heatwaves, ENSO +1.1)`,
    firesOk: (cnt) => `NASA FIRMS VIIRS S-NPP NRT · ${n(cnt)} fires in 24 h`,
    firesFail: (e, cnt) => `FIRMS unavailable (${e}) → WASM seasonal fire-belt model, ${n(cnt)} points`,
    iceSrc: (live) => (live ? 'NSIDC Sea Ice Index v3' : 'NSIDC unavailable → climatology + trend model'),
    iceBody: (src, arc, arcRef, ant, antRef) => `${src} · unit 10⁶ km²<br>Arctic ${arc} / baseline ${arcRef}<br>Antarctic ${ant} / baseline ${antRef}<br>`
      + '<small>White = current sea ice, yellow dashes = 1981–2010 median edge (approximate shape matching the extent)</small>',
    neoOk: (cnt, haz) => `NASA NeoWs: ${cnt} approaches in 7 days (${haz} hazardous) · tracks are schematic from miss distance and speed, log-scaled distance`,
    neoFail: (e) => `NeoWs fetch failed: ${e} (DEMO_KEY allows 30 requests/hour)`,
    dAir: 'Air pollution (US AQI)', dSst: 'Sea surface temp. anomaly', dFires: 'Fires', dIce: 'Sea ice', dAurora: 'Aurora', dClouds: 'Clouds',
    dOrbit: 'Orbital objects', dNeo: 'Near-Earth objects', dBackend: 'Backend',
    orbitNote: 'LEO &lt;2,000 km · GEO 35,786±1,000 km · otherwise MEO (by mean altitude). Click an object for details.',
    close: 'Close', cats: ['LEO debris', 'MEO debris', 'GEO debris', 'Active satellite', 'Crewed station'],
    tCat: 'Type', tAlt: 'Altitude', tSpeed: 'Speed', tPos: 'Position', tInc: 'Inclination', tPeriod: 'Period', min: 'min', tPgAp: 'Perigee/apogee',
    asteroid: 'Near-Earth asteroid', hazard: 'potentially hazardous', tClosest: 'Closest approach', tDist: 'Distance', mkm: 'million km',
    tRelV: 'Relative speed', tDia: 'Est. diameter',
    hudObjects: 'rendered', mobile: 'mobile',
  },
};

const store = {
  get: (k) => { try { return localStorage.getItem(k); } catch { return null; } },
  set: (k, v) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } },
};

let lang = store.get('lang');
if (!TEXT[lang]) lang = (navigator.language || '').toLowerCase().startsWith('ko') ? 'ko' : 'en';
const listeners = [];

export const getLang = () => lang;

export function t(key, ...args) {
  const v = TEXT[lang][key] ?? TEXT.en[key] ?? key;
  return typeof v === 'function' ? v(...args) : v;
}

export function applyStatic(root = document) {
  document.documentElement.lang = lang;
  document.title = 'Earth Monitor';
  document.querySelector('meta[name=description]')?.setAttribute('content', t('title'));
  for (const el of root.querySelectorAll('[data-i18n]')) el.textContent = t(el.dataset.i18n);
  for (const [attr, data] of [['title', 'i18nTitle'], ['placeholder', 'i18nPh'], ['aria-label', 'i18nAria']]) {
    for (const el of root.querySelectorAll(`[data-${data.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase())}]`)) el.setAttribute(attr, t(el.dataset[data]));
  }
}

export function setLang(l) {
  if (!TEXT[l] || l === lang) return;
  lang = l;
  store.set('lang', l);
  applyStatic();
  listeners.forEach((fn) => fn(l));
}

export const onLangChange = (fn) => listeners.push(fn);

# 🌍 Earth Monitor: 3D 통합 지구·우주 환경 실시간 모니터링 (v2.0)

브라우저에서 동작하는 인터랙티브 3D 지구본입니다. 렌더링과 UI는 **Three.js**가 맡고,
파싱·궤도 역학·보간·파티클 물리는 **Rust → WebAssembly(`earth.wasm`, 약 95 KB)**가 맡습니다.
외부 API가 막혀도 모든 레이어가 WASM 내장 모델로 바뀌어 화면이 비지 않습니다.
패널의 상태 표시는 🟢 실측, 🟠 대체 모델, 🔴 실패이고, 회전하는 아이콘은 로딩 중이라는 뜻입니다.

## 실행

```sh
node earth/server/server.mjs          # http://localhost:8080  (의존성 없음, Node 18 이상)
```

서버가 COOP/COEP 헤더를 붙이므로 `crossOriginIsolated`가 켜지고, 궤도 연산이 **Web Worker + SharedArrayBuffer** 모드로 돌아갑니다.
다른 정적 서버(`python3 -m http.server`, GitHub Pages 등)로 열어도 동작합니다. 이때는 메인 스레드 WASM 모드로 자동 전환됩니다.
`file://`로 열면 WASM을 불러올 수 없습니다.

선택 환경변수입니다. 백엔드에 두면 키가 브라우저에 노출되지 않습니다.

| 변수 | 용도 |
|---|---|
| `SPACETRACK_USER`, `SPACETRACK_PASS` | Space-Track.org GP(3LE). 없으면 CelesTrak |
| `WAQI_TOKEN` | WAQI 관측소 AQI. 없으면 Open-Meteo(Copernicus CAMS) 10° 격자 |
| `FIRMS_MAP_KEY` | NASA FIRMS VIIRS 24시간 화재 |
| `NASA_API_KEY` | NASA NeoWs (기본값 `DEMO_KEY`, 시간당 30회) |
| `OWM_KEY` | OpenWeatherMap `clouds_new` 타일 |
| `PORT` | 기본 8080 |

백엔드 없이 쓸 때는 패널의 **설정 · API 키**에 같은 키를 넣으면 됩니다(localStorage에 저장).

## 기능과 구현

| 요구사항 | 데이터 | 구현 | 대체 (오프라인) |
|---|---|---|---|
| FR-1.1 지구 | NASA Blue Marble / Black Marble, 지형(bump), 해양(specular) | 커스텀 셰이더. WASM이 계산한 태양 위치로 낮/밤, 범프, 해양 반사, 대기 림 처리 | — |
| FR-1.2 위치 | Geolocation → IP-API(백엔드) / GeoJS → 예테보리 | 3D 핀 + 펄스 링 + 라벨(현지 AQI) | 예테보리 |
| FR-1.3 카메라 | — | OrbitControls (드래그, 핀치, 줌) | — |
| FR-2.1 구름 | OWM 타일 (z2, 16장) | **WASM 메르카토르 → 정거원통 재투영**, 투명 구체가 서서히 회전 | 정적 구름 텍스처 |
| FR-2.2 대기오염 | WAQI 또는 CAMS(Open-Meteo) | **WASM JSON 스캔 → 5° 버킷 IDW 보간 히트맵 + 바람장으로 이동하는 먼지 입자** | 대도시 핫스팟 합성 |
| FR-2.3 오로라 | NOAA SWPC OVATION | **WASM 파싱 → 고도 100–300 km 커튼 입자**, 고도에 따라 녹색 → 적색, 깜빡임 셰이더 | Kp 통계 오벌 |
| FR-3.1 해수면 온도 이상 | NASA GIBS `GHRSST_L4_MUR_Sea_Surface_Temperature_Anomalies` | 해양 마스크 + 물결 펄스 애니메이션 | WASM 절차적 이상장 |
| FR-3.2 산불 | NASA FIRMS CSV | **WASM CSV 파싱 → FRP(화재 복사 강도) 기반 발광 입자** | 계절별 화재대 모델 |
| FR-3.3 해빙 | NSIDC Sea Ice Index v3 일별 면적 | **WASM 극관 메시**(면적을 맞춤). 흰 면 = 현재, 노란 점선 = 1981–2010 중앙값 | 기후값 + 추세 모델 |
| FR-3.4 야간 불빛 | Black Marble | 밤 영역에서만 발광 | — |
| FR-4.1 우주 쓰레기 | Space-Track / CelesTrak 파편군 | **WASM TLE 파싱 + SGP4** → LEO 빨강, MEO 주황, GEO 노랑 | 합성 파편 30,000개 |
| FR-4.2 활성 위성 | CelesTrak `active` | 파랑(ISS는 녹색). **클릭 → WASM 화면 좌표 픽킹(지구 가림 판정)** → 이름, 고도, 속도, 궤도요소 툴팁 | Walker 방식 위성군 합성 |
| FR-4.3 근접 천체(NEO) | NASA NeoWs feed | 빨간 궤적선 + 현재 위치 점, 클릭하면 툴팁 | 없음 (실패로 표시) |
| FR-5 UI | — | 레이어 토글 13개, 상태/스피너, 범례, 배속(×1–×3600), 반응형(모바일은 하단 시트) | — |

### 한계

- **SGP4**: Vallado 개정판의 근지구(near-earth) 분기를 전부 구현했고, Vallado 00005 기준값과 일치하는지 테스트합니다. 주기 225분 이상(GEO, GNSS 등)은 단순 항력 분기를 쓰고 **SDP4의 달·태양 섭동 항은 넣지 않았습니다**. 시각화에는 충분하지만 근접 충돌 분석에는 맞지 않습니다.
- **NEO**: NeoWs feed에는 궤도요소가 없습니다. 그래서 궤적은 접근 거리와 속도로 만든 **모식도**이고, 거리는 로그로 압축해 표시합니다.
- **해빙**: NSIDC 면적 값은 실측이지만 경계 **모양**은 그 면적에 맞춘 근사 형상입니다. 실제 윤곽 GeoJSON은 쓰지 않았습니다.
- **합성 개체군**: 실측 TLE만으로는 3만 개에 못 미칩니다. 그래서 NFR-1(입자 3만 개 이상)을 검증하려고 합성 파편을 함께 띄웁니다. 툴팁에 `합성` 태그로 구분되며, 설정에서 0–60,000개로 조절합니다.
- **SharedArrayBuffer**: 워커의 WASM이 SGP4를 돌린 뒤 결과를 SAB에 한 번 기록합니다. 메인 스레드는 그 SAB를 `InstancedMesh` 속성으로 삼아 **복사 없이** GPU에 올립니다. WASM 선형 메모리 자체를 스레드 간에 공유하려면 nightly Rust `build-std`가 필요해서 그 방식은 쓰지 않았습니다. SAB를 쓸 수 없으면 WASM 메모리 위의 `Float32Array` 뷰를 그대로 씁니다.

## 성능 설계 (NFR)

- 궤도 객체 전체를 **`THREE.InstancedMesh` 드로우콜 1회**로 그립니다. 인스턴스마다 8개 float(`x y z t | vx vy vz 분류`)를 갖고, 정점 셰이더가 `p + v·(t_now − t)`로 위치를 외삽합니다. 그래서 WASM이 일부만 갱신해도 매 프레임 부드럽게 움직입니다.
- 워커 모드: 3.5만 개 전체 SGP4(약 25–30 ms)를 메인 스레드 밖에서 반복합니다. 메인 스레드 모드: 프레임당 약 5 ms 예산으로 라운드로빈 갱신하며 예산은 자동 조절됩니다.
- **LOD**: 모바일은 합성 파편 9,000개와 줄어든 입자 수로 시작합니다. FPS가 45 아래로 떨어지면 렌더 인스턴스 수를 20%씩 줄이고, 회복되면 다시 늘립니다.
- **Rate limit**: 백엔드가 소스별 TTL로 메모리 + 디스크 캐시를 둡니다(오로라 10분, AQI·산불·구름 1시간, TLE·NEO 6시간, 해빙·SST 24시간). 캐시가 갱신되면 `/ws`로 `refresh` 알림을 보냅니다. 백엔드가 없으면 브라우저 localStorage TTL 캐시를 씁니다.

## 구조

```
earth/
├─ index.html            UI 골격, importmap
├─ css/style.css         반응형 패널 (데스크톱 좌측 / 모바일 하단 시트)
├─ js/
│  ├─ main.js            부팅, 레이어 로딩·대체, UI, 렌더 루프, LOD, 픽킹 툴팁
│  ├─ layers.js          Three.js 메시·셰이더 (지구, 구름, 오버레이, 입자, InstancedMesh, 해빙, NEO, 핀)
│  ├─ data.js            API 소스, TTL 캐시, WebSocket
│  ├─ orbit-engine.js    Worker+SAB 엔진 / 메인 스레드 엔진 (같은 인터페이스)
│  ├─ orbit-worker.js    궤도 워커
│  └─ wasm.js            earth.wasm 브리지
├─ earth.wasm            빌드 산출물 (커밋됨)
├─ wasm/                 Rust 소스 (sgp4, objects, aurora, air, climate, geo, scan)
├─ server/server.mjs     정적 서버 + COOP/COEP + /api 캐시 프록시 + /ws
├─ textures/             NASA Blue/Black Marble 기반 텍스처 (three-globe 예제, MIT). 구름은 알파 채널에서 추출
└─ vendor/               three.js r160 (MIT) + OrbitControls
```

8K 텍스처(예: [Solar System Scope](https://www.solarsystemscope.com/textures/), CC BY 4.0)를 쓰려면
`textures/8k_earth_daymap.jpg`, `8k_earth_nightmap.jpg`, `8k_earth_clouds.jpg` 이름으로 넣으세요. 있으면 자동으로 우선 사용합니다.

## WASM 빌드와 테스트

```sh
earth/wasm/build.sh                     # → earth/earth.wasm
cd earth/wasm && cargo test --release   # SGP4 기준값, TLE/JSON/CSV 파서, 개체군 분류
```

# Maya Chronos — Rust + WebAssembly + WebGL2

마야력(Tzolk'in · Haab' · Long Count)과 실시간 시계를 3D 기어로 보여주는 페이지입니다.
3D 렌더링, 텍스처 생성, 마야력 계산, 애니메이션을 모두 Rust로 구현했습니다.
three.js, GSAP, Tailwind는 쓰지 않습니다. JS는 `index.html`의 로더 몇 줄뿐입니다.

## 빌드 & 실행

```bash
# 1회 설치
rustup target add wasm32-unknown-unknown
cargo install wasm-pack          # 또는: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# 빌드 → ./pkg/ 생성
./build.sh                       # = wasm-pack build --target web --release --out-dir pkg

# 로컬 서버로 열기 (file:// 로는 wasm 모듈이 안 열립니다)
python3 -m http.server 8080
# → http://localhost:8080
```

wasm-pack 없이 빌드하려면 다음 명령을 씁니다.

```bash
cargo build --release --target wasm32-unknown-unknown
cargo install wasm-bindgen-cli   # Cargo.lock의 wasm-bindgen 버전과 맞출 것
wasm-bindgen target/wasm32-unknown-unknown/release/maya_chronos.wasm --target web --out-dir pkg
```

테스트(마야력 계산, URL 해시, 이징 함수)는 브라우저 없이 돌아갑니다.

```bash
cargo test
```

## 링 구성 (안 → 밖)

| 링 | 칸 수 | 내용 |
|---|---|---|
| 1 | 13 | Tzolk'in 숫자 1–13 (빨강 점·막대) |
| 2 | 20 | Tzolk'in 날 이름 상형문자 (Imix … Ajaw) |
| 3 | 20 | Haab' 일 0–19 |
| 4 | 19 | Haab' 달 상형문자 (Pop … Wayeb') |
| 5 | 24 | 시 |
| 6 | 60 | 분 |
| 7 | 60 | 초 |

금색 게이트(+X 방향) 안에 오늘 날짜와 지금 시각이 정렬됩니다. 예: 2026-10-18 = **13.0.14.0.9 · 9 Muluk · 2 Sak'**

## 원래 HTML에서 고친 것

1. **텍스처가 안 보이던 문제.** 기어 몸체의 bevel(0.1) 윗면이 텍스처 면(y=0.02)을 덮고 있었습니다. 지금은 몸체 옆면과 텍스처 윗면을 따로 만들어 같은 높이(y=0)에서 맞닿게 했습니다.
2. **상형문자 404.** Commons URL의 `/7/7b/` 같은 경로는 파일명의 MD5 해시입니다. 원래 URL은 이 해시가 전부 틀렸습니다. 이제 `md5` 크레이트로 런타임에 계산합니다(`src/glyphs.rs`). D20 Ajaw 파일도 `.svg`가 아니라 `.png`입니다.
3. **실제 마야력이 아니던 문제.** 1970-01-01 기준 나머지 계산 대신 GMT 상관계수 584283으로 Long Count를 구합니다(`src/calendar.rs`). 2012-12-21 = 13.0.0.0.0 · 4 Ajaw 3 K'ank'in을 테스트로 확인합니다.
4. 이미지가 로드되기 전이거나 실패하면 날 이름 글자(예: `Muluk`)로 대신 그립니다. 이미지가 도착하면 텍스처를 다시 그립니다. 브라우저 콘솔에 `Tzolkin glyphs: 20/20 loaded`가 찍힙니다.

## 파일

```
src/
  lib.rs       진입점 (#[wasm_bindgen(start)])
  app.rs       장면 구성, 프레임 루프, 카메라, DOM 연결
  calendar.rs  마야력 계산 (순수 Rust, 테스트 포함)
  glyphs.rs    Commons 이미지 URL(MD5) + 로딩/재시도
  draw2d.rs    점·막대 숫자, 기어 텍스처, 라벨, UI 시계 (Canvas 2D)
  gl.rs        WebGL2: 셰이더, 메시 업로드, 텍스처
  mesh.rs      링/박스/피라미드/평면 지오메트리
  math.rs      Vec3, Mat4
  anim.rs      트윈 + GSAP식 이징
index.html     UI 오버레이 + wasm 로더
```

상형문자 이미지를 직접 가진 파일로 바꾸려면 `src/glyphs.rs`의 `tzolkin_candidates` / `haab_candidates`가 돌려주는 URL을 바꾸면 됩니다. 상대경로(예: `glyphs/tz01.png`)도 됩니다.

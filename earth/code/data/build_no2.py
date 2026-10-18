#!/usr/bin/env python3
"""EcoDataCube (OpenGeoHub STAC) Sentinel-5P tropospheric NO₂, Europe → ../../data/no2-europe.png

STAC item: https://s3.ecodatacube.eu/arco/stac/no2_s5p.l3.trop.tmwm.p50/catalog.json
(median of the monthly medians, May 2018 – Nov 2022, 2 km, EPSG:3035 Cloud-Optimized GeoTIFF).
Reprojects LAEA Europe → plate carrée over BOUNDS and colours it with the collection's own SLD ramp.
js/main.js uses the same BOUNDS to place the texture on the globe.

  pip install numpy tifffile imagecodecs pillow
  python3 build_no2.py
"""
import math
import tempfile
import urllib.request
from pathlib import Path

import numpy as np
import tifffile
from PIL import Image

COG = 'https://s3.ecodatacube.eu/arco/no2_s5p.l3.trop.tmwm.p50_p50_2km_a_20180501_20221130_eu_epsg.3035_v20221219.tif'
BOUNDS = (-32.0, 26.0, 46.0, 72.0)  # west, south, east, north (degrees)
STEP = 0.04
OUT = Path(__file__).resolve().parents[2] / 'data' / 'no2-europe.png'
RAMP = [(74, '30123b'), (227.5, '28bceb'), (381, 'a4fc3c'), (534.5, 'fb7e21'), (688, '7a0403')]  # from the collection's .sld


def laea_3035(lat, lon):
    """EPSG:3035 forward (Lambert azimuthal equal-area on GRS80, Snyder's ellipsoidal formulas)."""
    a, f = 6378137.0, 1 / 298.257222101
    e2 = f * (2 - f)
    e = math.sqrt(e2)
    lat0, lon0, fe, fn = math.radians(52), math.radians(10), 4321000.0, 3210000.0

    def q(phi):
        s = np.sin(phi)
        return (1 - e2) * (s / (1 - e2 * s * s) - np.log((1 - e * s) / (1 + e * s)) / (2 * e))

    qp, q0 = q(math.pi / 2), q(lat0)
    rq = a * math.sqrt(qp / 2)
    b0 = math.asin(q0 / qp)
    d = a * (math.cos(lat0) / math.sqrt(1 - e2 * math.sin(lat0) ** 2)) / (rq * math.cos(b0))
    beta = np.arcsin(q(lat) / qp)
    dl = lon - lon0
    b = rq * np.sqrt(2 / (1 + math.sin(b0) * np.sin(beta) + math.cos(b0) * np.cos(beta) * np.cos(dl)))
    x = fe + b * d * np.cos(beta) * np.sin(dl)
    y = fn + (b / d) * (math.cos(b0) * np.sin(beta) - math.sin(b0) * np.cos(beta) * np.cos(dl))
    return x, y


def main():
    with tempfile.NamedTemporaryFile(suffix='.tif') as tmp:
        urllib.request.urlretrieve(COG, tmp.name)
        with tifffile.TiffFile(tmp.name) as tif:
            page, geo = tif.pages[0], tif.geotiff_metadata
            src = page.asarray().astype(np.float32)
            nodata = page.nodata
            sx, sy = geo['ModelPixelScale'][:2]
            x0, y0 = geo['ModelTiepoint'][3:5]
    src[src == nodata] = np.nan

    w, s, e, n = BOUNDS
    W, H = round((e - w) / STEP), round((n - s) / STEP)
    lon = np.radians(w + (np.arange(W) + 0.5) * STEP)[None, :]
    lat = np.radians(n - (np.arange(H) + 0.5) * STEP)[:, None]
    x, y = laea_3035(lat, lon)
    c = np.floor((x - x0) / sx).astype(int)
    r = np.floor((y0 - y) / sy).astype(int)
    ok = (c >= 0) & (c < src.shape[1]) & (r >= 0) & (r < src.shape[0])
    v = np.full((H, W), np.nan, np.float32)
    v[ok] = src[r[ok], c[ok]]

    qs = np.array([q for q, _ in RAMP])
    cols = np.array([[int(h[i:i + 2], 16) for i in (0, 2, 4)] for _, h in RAMP], np.float32)
    vv = np.nan_to_num(v, nan=qs[0])
    rgb = np.stack([np.interp(vv, qs, cols[:, k]) for k in range(3)], -1)
    alpha = np.clip((vv - 40) / (250 - 40), 0, 1) * 235  # clean air fades out, polluted areas stay opaque
    alpha[np.isnan(v)] = 0
    img = np.dstack([rgb, alpha]).round().astype(np.uint8)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(img, 'RGBA').quantize(256, method=Image.Quantize.FASTOCTREE).save(OUT, optimize=True)
    print(f'{W}x{H} → {OUT} ({OUT.stat().st_size // 1024} KB)')


if __name__ == '__main__':
    main()

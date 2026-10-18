#!/usr/bin/env python3
"""WorldPop 2025 global 1 km mosaic → H3 hexagons (resolution 4 by default, ~1,800 km² each) → ../../data/population.bin

Same idea as the Kontur Population dataset (population on the H3 grid), but built from the
~290 MB WorldPop GeoTIFF instead of Kontur's 2.4 GB GeoPackage so the result stays small.

  pip install numpy tifffile imagecodecs h3
  curl -o pop.tif https://data.worldpop.org/GIS/Population/Global_2015_2030/R2025A/2025/0_Mosaicked/v1/1km_ua/constrained/global_pop_2025_CN_1km_R2025A_UA_v1.tif
  python3 build_population.py pop.tif [h3 resolution, default 4]

Output: little-endian float32 records [lat°, lon°, people, rotation rad, radius km], largest first.
rotation = bearing of the cell's first vertex from its centre, so the 3D prism lines up with the real H3 cell.
"""
import math
import sys
from pathlib import Path

import h3
import numpy as np
import tifffile

RES = int(sys.argv[2]) if len(sys.argv) > 2 else 4
MIN_PEOPLE = 500
BLOCK = 10  # 10 × 30" = 5' blocks before H3 binning
OUT = Path(__file__).resolve().parents[2] / 'data' / 'population.bin'


def main(path):
    with tifffile.TiffFile(path) as tif:
        geo = tif.geotiff_metadata
        dx, dy = geo['ModelPixelScale'][:2]
        lon0, lat0 = geo['ModelTiepoint'][3:5]
        a = tif.pages[0].asarray()
    a[~np.isfinite(a) | (a < 0)] = 0
    h, w = a.shape
    h2, w2 = h // BLOCK, w // BLOCK
    blocks = a[: h2 * BLOCK, : w2 * BLOCK].reshape(h2, BLOCK, w2, BLOCK).sum(axis=(1, 3), dtype=np.float64)
    del a
    print(f'total population {blocks.sum() / 1e9:.3f} billion')

    rows, cols = np.nonzero(blocks)
    lats = lat0 - (rows + 0.5) * dy * BLOCK
    lons = lon0 + (cols + 0.5) * dx * BLOCK
    sums = {}
    for la, lo, v in zip(lats, lons, blocks[rows, cols]):
        c = h3.latlng_to_cell(float(la), float(lo), RES)
        sums[c] = sums.get(c, 0.0) + float(v)

    recs = []
    for c, v in sums.items():
        if v < MIN_PEOPLE:
            continue
        la, lo = h3.cell_to_latlng(c)
        vla, vlo = h3.cell_to_boundary(c)[0]
        # bearing of the first vertex (east of north) and centre→vertex distance
        p1, p2, dl = math.radians(la), math.radians(vla), math.radians(vlo - lo)
        rot = math.atan2(math.sin(dl) * math.cos(p2), math.cos(p1) * math.sin(p2) - math.sin(p1) * math.cos(p2) * math.cos(dl))
        r = h3.great_circle_distance((la, lo), (vla, vlo), unit='km')
        recs.append((la, lo, v, rot, r))
    recs.sort(key=lambda x: -x[2])
    OUT.parent.mkdir(parents=True, exist_ok=True)
    np.asarray(recs, dtype='<f4').tofile(OUT)
    print(f'{len(recs)} cells, max {recs[0][2]:,.0f} people → {OUT} ({OUT.stat().st_size // 1024} KB)')


if __name__ == '__main__':
    main(sys.argv[1])

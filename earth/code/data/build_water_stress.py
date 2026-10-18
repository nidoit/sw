#!/usr/bin/env python3
"""WRI Aqueduct baseline water stress (sub-basin polygons) → ../../data/water-stress.png

Pulls the polygons as GeoJSON from Resource Watch's public Carto table (dataset wat.050, Aqueduct 3.0)
and rasterises them to a 2048×1024 equirectangular RGBA texture with the Aqueduct colour scale.

  pip install pillow
  python3 build_water_stress.py
"""
import json
import urllib.parse
import urllib.request
from pathlib import Path

from PIL import Image, ImageDraw

W, H = 2048, 1024
OUT = Path(__file__).resolve().parents[2] / 'data' / 'water-stress.png'
SQL = ('select bws_cat, ST_SimplifyPreserveTopology(the_geom, 0.02) as the_geom '
       'from wat_050_aqueduct_baseline_water_stress where bws_cat <> -9999')
URL = 'https://wri-rw.carto.com/api/v2/sql?' + urllib.parse.urlencode({'q': SQL, 'format': 'geojson'})
COLORS = {  # Aqueduct legend: arid & low use, low, low-medium, medium-high, high, extremely high (RGBA)
    -1: (128, 128, 128, 150), 0: (255, 255, 153, 70), 1: (255, 230, 0, 200), 2: (255, 153, 0, 230), 3: (255, 25, 0, 240), 4: (153, 0, 0, 255),
}


def px(lon, lat):
    return ((lon + 180) / 360 * W, (90 - lat) / 180 * H)


def main():
    with urllib.request.urlopen(URL, timeout=300) as r:
        fc = json.load(r)
    img = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    for cat in sorted(COLORS):  # draw higher stress last so it wins on shared edges
        for f in fc['features']:
            g = f['geometry']
            if not g or f['properties']['bws_cat'] != cat:
                continue
            polys = g['coordinates'] if g['type'] == 'MultiPolygon' else [g['coordinates']]
            for poly in polys:
                draw.polygon([px(*p[:2]) for p in poly[0]], fill=COLORS[cat])
                for hole in poly[1:]:
                    draw.polygon([px(*p[:2]) for p in hole], fill=(0, 0, 0, 0))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    img.save(OUT, optimize=True)
    print(f"{len(fc['features'])} basins → {OUT} ({OUT.stat().st_size // 1024} KB)")


if __name__ == '__main__':
    main()

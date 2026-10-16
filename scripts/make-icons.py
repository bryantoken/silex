"""Gera os ícones do Silex a partir da geometria em src/lib/logo.ts.

Cada tamanho é desenhado em 8x e reduzido (nítido em 16/24/32 px na barra de
tarefas), em vez de reduzir um único PNG grande. Saída:
  assets/logo.svg, assets/icon.png (1024), static/favicon.png
  src-tauri/icons/icon.ico (16..256) — rode antes `pnpm tauri icon assets/icon.png`
  para os demais formatos e depois este script de novo com --ico.

Uso: python scripts/make-icons.py [--ico]
"""
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
TILE = (32, 32, 32)  # #202020, o cinza do tema escuro
SRC = (ROOT / "src/lib/logo.ts").read_text(encoding="utf-8")


def array(name):
    body = re.search(rf"const {name} = \[(.*?)\] as const;", SRC, re.S).group(1)
    return [tuple(map(int, p)) for p in re.findall(r"\[(\d+),\s*(\d+)\]", body)]


OUTER, TABLE = array("OUTER"), array("TABLE")
SHADE = [float(x) for x in re.search(r"const SHADE = \[(.*?)\]", SRC).group(1).split(",")]
FACETS = [([OUTER[i], OUTER[(i + 1) % 6], TABLE[(i + 1) % 6], TABLE[i]], SHADE[i]) for i in range(6)] + [(TABLE, 1.0)]


def mix(a):
    return tuple(round(255 * a + c * (1 - a)) for c in TILE)


def render(size, scale=0.82):
    s = 8
    n = size * s
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, n - 1, n - 1], radius=round(n * 232 / 1024), fill=TILE + (255,))
    k = n / 1024
    for pts, a in FACETS:
        # Cores sólidas (branco misturado ao cinza): as facetas não se sobrepõem
        d.polygon([((512 + (x - 512) * scale) * k, (512 + (y - 512) * scale) * k) for x, y in pts], fill=mix(a) + (255,))
    return img.resize((size, size), Image.LANCZOS)


def svg():
    polys = "\n".join(
        f'    <polygon points="{" ".join(f"{x},{y}" for x, y in pts)}" fill-opacity="{a}" />' for pts, a in FACETS
    )
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <!-- Silex: pedra de sílex lapidada. Gerado por scripts/make-icons.py a partir de src/lib/logo.ts -->
  <rect width="1024" height="1024" rx="232" fill="#202020" />
  <g fill="#ffffff" transform="translate(512 512) scale(0.82) translate(-512 -512)">
{polys}
  </g>
</svg>
"""


if "--ico" in sys.argv:
    # Tamanhos pequenos com a pedra maior, para continuar legível na barra de tarefas
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [render(z, 0.92 if z <= 32 else 0.88 if z <= 64 else 0.82) for z in sizes]
    frames[-1].save(ROOT / "src-tauri/icons/icon.ico", sizes=[(z, z) for z in sizes], append_images=frames[:-1])
    for z, name in [(32, "32x32.png"), (64, "64x64.png"), (128, "128x128.png"), (256, "128x128@2x.png")]:
        render(z, 0.92 if z <= 32 else 0.88 if z <= 64 else 0.82).save(ROOT / "src-tauri/icons" / name)
    print("icon.ico e PNGs pequenos regenerados")
else:
    (ROOT / "assets/logo.svg").write_text(svg(), encoding="utf-8")
    render(1024).save(ROOT / "assets/icon.png")
    render(64, 0.88).save(ROOT / "static/favicon.png")
    print("assets/logo.svg, assets/icon.png e static/favicon.png gerados")

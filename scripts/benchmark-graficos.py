"""Resume o CSV de scripts/benchmark-memoria.ps1 e gera os gráficos.

Definições (as mesmas do texto do estudo):
- Regime: segundos 61 a 90 de cada rodada (app aberto e parado).
- Memória privada em regime: mediana das amostras da janela, por rodada.
- CPU em regime: tempo de CPU gasto na janela / 30 s (100% = um núcleo inteiro).
- Custo de abertura: tempo de CPU acumulado até o segundo 60.
- Cada app: mediana das rodadas, com mínimo e máximo.

Uso: python scripts/benchmark-graficos.py benchmark.csv pasta-de-saida
"""
import csv
import json
import statistics as st
import sys
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

src, out = Path(sys.argv[1]), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
rows = list(csv.DictReader(src.open(encoding="utf-8-sig")))
runs = defaultdict(list)
for r in rows:
    runs[(r["App"], int(r["Rodada"]))].append({k: float(r[k].replace(",", ".")) for k in ("Segundo", "Processos", "PrivadaMB", "WorkingMB", "CpuSeg")})

A, B = 60, 90
per_run = defaultdict(list)
for (app, rodada), s in sorted(runs.items()):
    s.sort(key=lambda x: x["Segundo"])
    win = [x for x in s if A < x["Segundo"] <= B]
    at = {int(x["Segundo"]): x for x in s}
    per_run[app].append(
        {
            "rodada": rodada,
            "privada_mb": st.median(x["PrivadaMB"] for x in win),
            "working_mb": st.median(x["WorkingMB"] for x in win),
            "pico_privada_mb": max(x["PrivadaMB"] for x in s),
            "processos": st.median(x["Processos"] for x in win),
            "cpu_regime_pct": (at[B]["CpuSeg"] - at[A]["CpuSeg"]) / (B - A) * 100,
            "cpu_abertura_s": at[A]["CpuSeg"],
        }
    )

summary = {}
for app, rs in per_run.items():
    summary[app] = {k: {"mediana": st.median(r[k] for r in rs), "min": min(r[k] for r in rs), "max": max(r[k] for r in rs)} for k in rs[0] if k != "rodada"}
    summary[app]["rodadas"] = rs
print(json.dumps(summary, ensure_ascii=False, indent=2))
(out / "resumo.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")

COLORS = {"Silex": "#111111", "Obsidian": "#7c6cf0"}
apps = sorted(summary, key=lambda a: a != "Silex")
plt.rcParams.update({"font.family": "Segoe UI", "font.size": 10, "axes.spines.top": False, "axes.spines.right": False})

# 1. Memória privada ao longo do tempo (cada rodada fina, mediana grossa)
fig, ax = plt.subplots(figsize=(8, 4.2), dpi=160)
for app in apps:
    series = [sorted(runs[(app, r["rodada"])], key=lambda x: x["Segundo"]) for r in per_run[app]]
    for s in series:
        ax.plot([x["Segundo"] for x in s], [x["PrivadaMB"] for x in s], color=COLORS.get(app), alpha=0.25, lw=1)
    n = min(len(s) for s in series)
    ax.plot(range(1, n + 1), [st.median(s[i]["PrivadaMB"] for s in series) for i in range(n)], color=COLORS.get(app), lw=2.4, label=app)
ax.axvspan(A, B, color="#000", alpha=0.05)
ax.text((A + B) / 2, ax.get_ylim()[1] * 0.97, "janela de regime", ha="center", va="top", fontsize=8, color="#666")
ax.set_xlabel("segundos desde a abertura")
ax.set_ylabel("memória privada (MB), todos os processos")
ax.set_title("Memória ao abrir o mesmo cofre (3 rodadas por app)", loc="left", fontweight="bold")
ax.legend(frameon=False)
fig.tight_layout()
fig.savefig(out / "1-memoria-no-tempo.png")

# 2. Regime: memória privada e working set (mediana, barras de mínimo–máximo)
fig, axes = plt.subplots(1, 3, figsize=(10, 3.8), dpi=160)
for ax, key, label in [
    (axes[0], "privada_mb", "Memória privada (MB)"),
    (axes[1], "working_mb", "Working set (MB)"),
    (axes[2], "cpu_regime_pct", "CPU parado (% de 1 núcleo)"),
]:
    med = [summary[a][key]["mediana"] for a in apps]
    err = [[m - summary[a][key]["min"] for a, m in zip(apps, med)], [summary[a][key]["max"] - m for a, m in zip(apps, med)]]
    bars = ax.bar(apps, med, yerr=err, capsize=4, color=[COLORS.get(a) for a in apps], width=0.55)
    for b, m, a in zip(bars, med, apps):
        top = summary[a][key]["max"]  # rótulo acima da barra de erro
        ax.text(b.get_x() + b.get_width() / 2, top, f" {m:.2f}" if key == "cpu_regime_pct" else f"{m:.0f}", ha="center", va="bottom", fontsize=9, fontweight="bold")
    ax.set_title(label, fontsize=10)
    ax.margins(y=0.18)
fig.suptitle("App aberto e parado (segundos 61–90) · mediana de 3 rodadas, barras = mín–máx", x=0.01, ha="left", fontweight="bold")
fig.tight_layout()
fig.savefig(out / "2-regime.png")
print("gráficos em", out)

"""Dados fictícios e coerentes para as capturas de tela do README (nada da máquina real)."""
import math
import time

TARIFA = 0.92
COMPONENTES = [  # rótulo, tipo, origem, watts agora
    ("NVIDIA GeForce RTX 4060", "medido", "NVML (driver NVIDIA)", 68.4),
    ("CPU núcleos", "medido", "RAPL do kernel", 41.2),
    ("CPU resto do chip", "medido", "RAPL do kernel", 18.7),
    ("Placa-mãe", "estimado", "valor fixo", 15.0),
    ("RAM", "estimado", "valor fixo", 8.0),
    ("Fans e USB", "estimado", "valor fixo", 8.0),
    ("SSD Exemplo 1TB", "estimado", "atividade do disco", 2.6),
    ("HDD Exemplo 4TB", "estimado", "atividade do disco", 4.1),
]
EFICIENCIA = 0.88


def _ids(i, rotulo):
    return {"NVIDIA GeForce RTX 4060": "gpu:id-3f9a1c2e7b6d4a10", "CPU núcleos": "cpu_nucleos",
            "CPU resto do chip": "cpu_soc"}.get(rotulo, f"comp{i}")


def status():
    dc = sum(w for *_, w in COMPONENTES)
    perda = dc / EFICIENCIA - dc
    parede = dc + perda
    agora = [{"id": _ids(i, r), "rotulo": r, "tipo": t, "origem": o, "w": w, "pct": round(w / parede * 100, 1),
              "atividade": 0.12 if "disco" in o else None} for i, (r, t, o, w) in enumerate(COMPONENTES)]
    agora.append({"id": "fonte_perda", "rotulo": "Perda na fonte", "tipo": "calculado", "origem": "eficiência da fonte",
                  "w": round(perda, 1), "pct": round(perda / parede * 100, 1), "atividade": None})
    agora.sort(key=lambda c: (c["tipo"] == "calculado", -c["w"]))

    def periodo(horas):
        comps = []
        for c in agora:
            kwh = c["w"] * horas / 1000 * 0.7
            comps.append({"id": c["id"], "rotulo": c["rotulo"], "tipo": c["tipo"], "origem": c["origem"],
                          "kwh": round(kwh, 3), "custo": round(kwh * TARIFA, 2), "pct": c["pct"]})
        kwh = sum(c["kwh"] for c in comps)
        return comps, round(kwh, 3), round(kwh * TARIFA, 2)

    hoje, kh, ch = periodo(9)
    ciclo, kc, cc = periodo(9 * 10)
    return {
        "versao_api": 1,
        "agora": {"parede_w": round(parede, 1), "custo_hora": round(parede / 1000 * TARIFA, 3),
                  "atualizado_ha_s": 0.8, "componentes": agora},
        "hoje": {"kwh": kh, "custo": ch, "componentes": hoje},
        "ciclo": {"inicio": "2026-10-01", "fim": "2026-11-01", "kwh": kc, "custo": cc,
                  "previsao_kwh": round(kc * 3.1, 3), "previsao_custo": round(cc * 3.1, 2), "componentes": ciclo},
        "media": {"kwh_dia": round(kc / 10, 3), "custo_dia": round(cc / 10, 2), "horas_ligado_dia": 9.2,
                  "janela_dias": 10.0},
        "estimativa_24x7": {"kwh": round(parede * 24 * 31 / 1000, 3), "custo": round(parede * 24 * 31 / 1000 * TARIFA, 2)},
        "config": {"preco_kwh": TARIFA, "bandeira_kwh": 0.0, "dia_leitura": 1, "eficiencia_fonte": EFICIENCIA,
                   "placa_mae_w": 15.0, "ram_w": 8.0, "fans_usb_w": 8.0, "hdd_ocioso_w": 4.0, "hdd_ativo_w": 6.0,
                   "hdd_standby_w": 0.8, "ssd_ocioso_w": 1.5, "ssd_ativo_w": 5.5, "cpu_ocioso_w": 10.0, "cpu_max_w": 65.0},
        "config_schema": {k: {"min": 0, "max": 500, "rotulo": r} for k, r in [
            ("preco_kwh", "Tarifa (R$/kWh, com impostos)"), ("bandeira_kwh", "Adicional bandeira (R$/kWh)"),
            ("dia_leitura", "Dia de início do ciclo da conta"), ("eficiencia_fonte", "Eficiência da fonte (0–1)"),
            ("placa_mae_w", "Placa-mãe/chipset (W)"), ("ram_w", "RAM (W)"), ("fans_usb_w", "Fans, bomba, USB, LEDs (W)"),
            ("hdd_ocioso_w", "HDD ocioso (W)"), ("hdd_ativo_w", "HDD ativo (W)"), ("hdd_standby_w", "HDD em standby (W)"),
            ("ssd_ocioso_w", "SSD ocioso (W)"), ("ssd_ativo_w", "SSD ativo (W)"), ("cpu_ocioso_w", "CPU ociosa (W)"),
            ("cpu_max_w", "CPU em carga total (W)")]},
        "cpu_estimada": False, "tarifa_efetiva": TARIFA, "tem_hdd": True, "avisos": [],
    }


def historico(faixa):
    agora = int(time.time()) // 60 * 60
    if faixa == "30d":
        dias = []
        for i in range(30, 0, -1):
            t = time.localtime(agora - i * 86400)
            kwh = 1.4 + 0.6 * math.sin(i / 3) + (0.4 if t.tm_wday >= 5 else 0)
            dias.append({"dia": time.strftime("%Y-%m-%d", t), "kwh": round(kwh, 3), "custo": round(kwh * TARIFA, 2),
                         "horas_ligado": round(7 + 2 * math.sin(i), 3)})
        return {"dias": dias}
    span = 86400 if faixa == "24h" else 7 * 86400
    passo = 300 if faixa == "24h" else 1800
    pontos = []
    for t in range(agora - span, agora, passo):
        hora = time.localtime(t).tm_hour + time.localtime(t).tm_min / 60
        if 1 <= hora < 8:  # desligado de madrugada
            continue
        w = 95 + 60 * max(0, math.sin((hora - 8) / 16 * math.pi)) + 25 * math.sin(t / 2700)
        pontos.append({"t": t, "w": round(w, 1), "kwh": round(w * passo / 3_600_000, 3)})
    return {"passo": passo, "inicio": agora - span, "fim": agora, "pontos": pontos}

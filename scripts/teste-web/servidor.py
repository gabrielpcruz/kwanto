"""Servidor falso para o teste do navegador: entrega as páginas reais de web/ e uma API com
rótulos hostis (como se a limpeza do servidor tivesse falhado), para provar que o dashboard e o
popup não executam nada vindo dos dados."""
import http.server
import json
import sys
from pathlib import Path

WEB = Path(sys.argv[1])
PORTA = int(sys.argv[2])
# "demo": dados fictícios coerentes (capturas do README) em vez dos rótulos hostis.
DEMO = len(sys.argv) > 3 and sys.argv[3] == "demo"
if DEMO:
    sys.path.insert(0, str(Path(__file__).parent))
    import demo
XSS = "<img src=x onerror=\"document.body.dataset.xss='1'\">"
COMP = {"id": "gpu:" + XSS, "rotulo": "GPU " + XSS, "tipo": "medido\" onmouseover=\"x", "origem": XSS,
        "w": 50.0, "pct": 50.0, "kwh": 1.0, "custo": 1.0, "atividade": None}
STATUS = {
    "versao_api": 1,
    "agora": {"parede_w": 100.0, "custo_hora": 0.1, "atualizado_ha_s": 1.0, "componentes": [COMP]},
    "hoje": {"kwh": 1.0, "custo": 1.0, "componentes": [COMP]},
    "ciclo": {"inicio": "2026-10-01", "fim": "2026-11-01", "kwh": 1.0, "custo": 1.0, "previsao_kwh": 2.0,
              "previsao_custo": 2.0, "componentes": [COMP]},
    "media": {"kwh_dia": 1.0, "custo_dia": 1.0, "horas_ligado_dia": 1.0, "janela_dias": 1.0},
    "estimativa_24x7": {"kwh": 1.0, "custo": 1.0},
    "config": {"preco_kwh": 0.85}, "config_schema": {"preco_kwh": {"min": 0, "max": 20, "rotulo": "Tarifa " + XSS}},
    "cpu_estimada": True, "tarifa_efetiva": 0.85, "tem_hdd": False, "avisos": ["Aviso " + XSS],
}
EXTENSOES = [{"titulo": "Ext " + XSS, "url": "javascript:document.body.dataset.xss='1'"}]


class Manipulador(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def enviar(self, corpo, tipo):
        self.send_response(200)
        self.send_header("Content-Type", tipo)
        self.end_headers()
        self.wfile.write(corpo)

    def do_GET(self):
        caminho = self.path.split("?")[0]
        if caminho == "/api/status":
            return self.enviar(json.dumps(demo.status() if DEMO else STATUS).encode(), "application/json")
        if caminho == "/api/extensoes":
            return self.enviar(json.dumps([] if DEMO else EXTENSOES).encode(), "application/json")
        if caminho == "/api/historico":
            if DEMO:
                faixa = self.path.split("faixa=")[-1] if "faixa=" in self.path else "24h"
                return self.enviar(json.dumps(demo.historico(faixa)).encode(), "application/json")
            return self.enviar(b'{"pontos":[],"dias":[],"inicio":0,"fim":0}', "application/json")
        if caminho == "/_marca":
            return self.enviar((Path(__file__).parent / "marca.html").read_bytes(), "text/html")
        arquivo = {"/": "dashboard.html", "/mini": "mini.html"}.get(caminho, caminho.lstrip("/"))
        alvo = (WEB / arquivo).resolve()
        if WEB.resolve() not in alvo.parents or not alvo.is_file():
            self.send_response(404)
            self.end_headers()
            return
        tipo = {".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml",
                ".woff2": "font/woff2"}.get(alvo.suffix, "application/octet-stream")
        self.enviar(alvo.read_bytes(), tipo)


http.server.ThreadingHTTPServer(("127.0.0.1", PORTA), Manipulador).serve_forever()

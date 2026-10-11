const { desenhar, simboloHTML, fmt, brl, nomeCurto } = Unifilar;
const $ = s => document.querySelector(s);
// Dentro da bandeja, a página fala com ela por window.ipc; aberta no navegador (testes), não há ponte.
const ponte = window.ipc ? { postMessage: m => window.ipc.postMessage(m) } : null;
const enviar = msg => ponte ? ponte.postMessage(JSON.stringify(msg)) : null;
$("#leg").innerHTML = ["medido", "estimado", "calculado"].map(t => `<span>${simboloHTML(t)}${t}</span>`).join("");

// Popup: os 4 maiores, o resto somado em "Outros", e a perda da fonte por último.
function itens(s) {
  const comps = s.agora.componentes.filter(c => c.w > 0);
  const normais = comps.filter(c => c.tipo !== "calculado"), calc = comps.filter(c => c.tipo === "calculado");
  const top = normais.slice(0, 4), resto = normais.slice(4);
  const lista = top.map(c => ({ nome: nomeCurto(c.rotulo), w: c.w, tipo: c.tipo }));
  if (resto.length) lista.push({ nome: `Outros (${resto.length})`, w: resto.reduce((t, c) => t + c.w, 0),
    tipo: resto.every(c => c.tipo === "medido") ? "medido" : "estimado" });
  return [...lista, ...calc.map(c => ({ nome: nomeCurto(c.rotulo), w: c.w, tipo: c.tipo }))];
}

function avisarAltura() { enviar({ acao: "altura", valor: $("#cartao").offsetHeight }); }

async function atualizar() {
  try {
    const s = await (await fetch("/api/status")).json();
    $("#conteudo").hidden = false; $("#parado").hidden = true; $("#tres").hidden = false;
    const a = s.agora;
    $("#w").innerHTML = `${fmt(a.parede_w)}<small>W</small>`;
    $("#custo").textContent = `${brl(a.custo_hora)}/h`;
    const ok = a.atualizado_ha_s != null && a.atualizado_ha_s < 10;
    $("#st-vivo").textContent = ok ? "ao vivo " : "sem leitura ";
    $("#st-vivo").classList.toggle("off", !ok);
    if (a.atualizado_ha_s != null) $("#st-hora").textContent = new Date(Date.now() - a.atualizado_ha_s * 1000).toLocaleTimeString("pt-BR");
    desenhar($("#fluxo-mini"), itens(s), { topo: 8, alt: 96, gap: 4, fs: 11.5, simb: 15, largura: 330,
      xT: 0, xF: 52, xC: 146, xW: 330, corrente: true, cabecalho: false });
    $("#hoje").textContent = brl(s.hoje.custo);
    $("#ciclo").textContent = brl(s.ciclo.custo);
    $("#prev").textContent = brl(s.ciclo.previsao_custo);
  } catch {
    $("#conteudo").hidden = true; $("#parado").hidden = false; $("#tres").hidden = true;
    $("#st-vivo").textContent = "parado"; $("#st-vivo").classList.add("off"); $("#st-hora").textContent = "";
  }
  avisarAltura();
}

let timer = null;
window.energiaVisivel = visivel => {
  clearInterval(timer); timer = null;
  if (visivel) { atualizar(); timer = setInterval(atualizar, 2000); }
};
$("#abrir").addEventListener("click", () => ponte ? enviar({ acao: "abrir" }) : window.open("/", "_blank"));
$("#sair").addEventListener("click", () => enviar({ acao: "sair" }));
document.addEventListener("keydown", e => { if (e.key === "Escape") enviar({ acao: "fechar" }); });
if (!ponte) window.energiaVisivel(true);  // aberto direto no navegador (para testes)
else atualizar();

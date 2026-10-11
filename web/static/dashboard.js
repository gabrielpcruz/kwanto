const { desenhar, simboloHTML, fmt, brl, nomeCurto } = Unifilar;
const $ = s => document.querySelector(s);
/** Todo texto vindo da API passa por aqui antes de virar HTML. */
const esc = t => String(t).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const dataBR = iso => `${iso.slice(8, 10)}/${iso.slice(5, 7)}`;
const cor = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
let ultimo = null, abaHist = "24h", abaComp = "hoje", hist = null;

$("#legenda").innerHTML = [
  ["medido", "Medido", "por sensor do hardware, como o RAPL do kernel para a CPU e o NVML do driver NVIDIA para a GPU"],
  ["estimado", "Estimado", "sem sensor: valor fixo ajustável, ou pela atividade real no caso dos discos"],
  ["calculado", "Calculado", "a partir dos demais: perda da fonte pela eficiência configurada"],
].map(([t, n, d]) => `<span>${simboloHTML(t)}<b>${n}</b> ${d}</span>`).join("");

function itensDiagrama(s) {
  const comps = s.agora.componentes.filter(c => c.w > 0);
  const normais = comps.filter(c => c.tipo !== "calculado"), calc = comps.filter(c => c.tipo === "calculado");
  return [...normais, ...calc].map(c => ({ nome: nomeCurto(c.rotulo), w: c.w, tipo: c.tipo, origem: c.origem,
    custoH: c.w / 1000 * s.tarifa_efetiva }));
}

function render(s) {
  const a = s.agora;
  $("#w").innerHTML = `${fmt(a.parede_w)}<small>W</small>`;
  $("#custo-h").textContent = brl(a.custo_hora, 3);
  $("#mes").textContent = brl(s.estimativa_24x7.custo, 0);

  const ok = a.atualizado_ha_s != null && a.atualizado_ha_s < 10;
  $("#st-vivo").textContent = ok ? "ao vivo" : "sem leitura recente";
  $("#st-vivo").classList.toggle("off", !ok);
  if (a.atualizado_ha_s != null) $("#st-hora").textContent = new Date(Date.now() - a.atualizado_ha_s * 1000).toLocaleTimeString("pt-BR");
  const med = a.componentes.filter(c => c.tipo === "medido").length, est = a.componentes.filter(c => c.tipo === "estimado").length;
  $("#st-sensores").textContent = `${med} medidos, ${est} estimados`;

  desenhar($("#fluxo"), itensDiagrama(s), { topo: 48, alt: 230, gap: 6, fs: 13, simb: 18, largura: 990,
    xT: 44, xF: 200, xC: 410, xW: 700, colCusto: 790, colOrigem: 808, corrente: true, cabecalho: true,
    eficiencia: `${fmt(s.config.eficiencia_fonte * 100)}% eficiente` });

  const avisos = [...s.avisos];
  if (s.versao_api !== VERSAO_API) avisos.push("O serviço do Kwanto foi atualizado e esta página ficou desatualizada. Recarregue a página para ver tudo certo.");
  if (s.media.janela_dias < 2) avisos.push(`A previsão usa só ${fmt(s.media.janela_dias * 24, 1)} h de dados. Ela fica confiável depois de alguns dias de uso normal.`);
  $("#avisos").innerHTML = avisos.map(t => `<div class="nota-aviso"><svg viewBox="0 0 18 18" aria-hidden="true"><path d="M9 2 17 16H1Z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M9 7v4M9 13v.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg><span>${esc(t)}</span></div>`).join("");

  const c = s.ciclo, m = s.media;
  $("#c-hoje").textContent = brl(s.hoje.custo);
  $("#c-hoje-kwh").textContent = `${fmt(s.hoje.kwh, 2)} kWh`;
  $("#c-ciclo-r").textContent = `Conta, desde ${dataBR(c.inicio)}`;
  $("#c-ciclo").textContent = brl(c.custo);
  $("#c-ciclo-kwh").textContent = `${fmt(c.kwh, 1)} kWh`;
  $("#c-prev-r").textContent = `Previsão até ${dataBR(c.fim)}`;
  $("#c-prev").textContent = brl(c.previsao_custo);
  const ini = new Date(c.inicio + "T00:00").getTime(), fim = new Date(c.fim + "T00:00").getTime();
  $("#c-prog").style.width = `${Math.min((Date.now() - ini) / (fim - ini) * 100, 100)}%`;
  $("#c-media").textContent = `${fmt(m.kwh_dia, 2)} kWh`;
  $("#c-horas").textContent = `${fmt(m.horas_ligado_dia, 1)} h ligado por dia`;
  $("#c-media-r").textContent = brl(m.custo_dia);
  $("#c-tarifa-r").textContent = s.config.bandeira_kwh > 0 ? "Tarifa com adicional de bandeira" : "Tarifa, com impostos";
  $("#c-tarifa").textContent = `${brl(s.tarifa_efetiva, 3)}/kWh`;
  renderTabela();
}

function renderTabela() {
  if (!ultimo) return;
  const lista = ultimo[abaComp].componentes;
  const max = Math.max(...lista.map(c => c.pct), 1);
  $("#tabela").innerHTML = lista.map(c => `<tr class="${esc(c.tipo)}">
      <td class="nome">${simboloHTML(c.tipo)}${esc(nomeCurto(c.rotulo))}</td><td class="origem">${esc(c.origem || "")}</td>
      <td>${c.tipo === "estimado" ? "≈ " : ""}${fmt(c.kwh, 3)}</td><td>${fmt(c.custo, 2)}</td>
      <td>${fmt(c.pct, 1)}%<span class="parcela"><b style="width:${c.pct / max * 100}%"></b></span></td></tr>`).join("")
    || `<tr><td colspan="5" style="color:var(--apagado)">Os dados aparecem aqui depois do primeiro minuto de coleta.</td></tr>`;
}

// ------------------------------------------------------------ histórico
function desenharHist() {
  const svg = $("#graf"), W = 600, H = 190, fio = cor("--fio"), grade = cor("--grade-2"), tinta = cor("--tinta");
  if (!hist) return;
  let g = "", pontos = [], max = 1, rotulos = [];
  if (abaHist === "30d") {
    const dias = hist.dias;
    max = Math.max(...dias.map(d => d.kwh), .1) * 1.15;
    const bw = W / Math.max(dias.length, 1);
    dias.forEach((d, i) => {
      const h = d.kwh / max * H;
      g += `<rect x="${(i * bw + bw * .2).toFixed(1)}" y="${(H - h).toFixed(1)}" width="${(bw * .6).toFixed(1)}" height="${h.toFixed(1)}" fill="${fio}" fill-opacity=".8"/>`;
      pontos.push({ x: (i + .5) * bw, txt: `${dataBR(d.dia)}  ${fmt(d.kwh, 2)} kWh  ${brl(d.custo)}` });
    });
    const passo = Math.max(1, Math.ceil(dias.length / 6));
    rotulos = dias.filter((_, i) => i % passo === 0).map(d => dataBR(d.dia));
    $("#graf-nota").textContent = dias.length ? `Energia por dia, na tomada. Total do período: ${brl(dias.reduce((s, d) => s + d.custo, 0))}.` : "Sem dados ainda.";
  } else {
    const span = hist.fim - hist.inicio;
    max = Math.max(...hist.pontos.map(p => p.w), 50) * 1.15;
    let d = "", prev = null;
    hist.pontos.forEach(p => {
      const x = (p.t - hist.inicio) / span * W, y = H - p.w / max * H;
      d += `${prev === null || p.t - prev > hist.passo * 1.5 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
      prev = p.t;
      const dt = new Date(p.t * 1000);
      const quando = abaHist === "24h" ? dt.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })
        : dt.toLocaleString("pt-BR", { weekday: "short", hour: "2-digit", minute: "2-digit" });
      pontos.push({ x, txt: `${quando}  ${fmt(p.w)} W` });
    });
    g += `<path d="${d}" fill="none" stroke="${fio}" stroke-width="2" stroke-linejoin="round" vector-effect="non-scaling-stroke"/>`;
    // ponto isolado (pouco histórico) aparece como marcador
    if (hist.pontos.length === 1) g += `<circle cx="${pontos[0].x}" cy="${H - hist.pontos[0].w / max * H}" r="3" fill="${fio}"/>`;
    for (let i = 0; i <= 4; i++) {
      const t = new Date((hist.inicio + span * i / 4) * 1000);
      rotulos.push(i === 4 ? "agora" : abaHist === "24h" ? t.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" }) : t.toLocaleDateString("pt-BR", { weekday: "short" }));
    }
    $("#graf-nota").textContent = hist.pontos.length ? "Potência média na tomada. Os trechos vazios são períodos com o PC desligado ou suspenso." : "Sem dados ainda.";
  }
  let grid = "";
  for (let i = 1; i <= 4; i++) grid += `<line x1="0" y1="${H - i * H / 4}" x2="${W}" y2="${H - i * H / 4}" stroke="${grade}" stroke-width="1" vector-effect="non-scaling-stroke"/>`;
  grid += `<line x1="0" y1="${H}" x2="${W}" y2="${H}" stroke="${tinta}" stroke-opacity=".6" stroke-width="1.2" vector-effect="non-scaling-stroke"/>`;
  svg.innerHTML = grid + g;
  svg._pontos = pontos;
  $("#eixo").innerHTML = rotulos.map(r => `<span>${r}</span>`).join("");
  const un = abaHist === "30d" ? "kWh" : "W";
  $("#graf-box").querySelectorAll(".escala").forEach(e => e.remove());
  for (const i of [2, 4]) {
    const e = document.createElement("span");
    e.className = "escala"; e.style.top = `${(1 - i / 4) * H + 6}px`;
    e.textContent = `${fmt(max * i / 4, un === "W" ? 0 : 1)} ${un}`;
    $("#graf-box").appendChild(e);
  }
}

$("#graf").addEventListener("mousemove", e => {
  const svg = $("#graf"), pts = svg._pontos || [];
  if (!pts.length) return;
  const r = svg.getBoundingClientRect(), x = (e.clientX - r.left) / r.width * 600;
  const p = pts.reduce((a, b) => Math.abs(b.x - x) < Math.abs(a.x - x) ? b : a);
  const mira = $("#mira");
  mira.hidden = false; mira.textContent = p.txt;
  mira.style.left = `${Math.max(0, Math.min(p.x / 600 * r.width - mira.offsetWidth / 2, r.width - mira.offsetWidth))}px`;
  let l = svg.querySelector(".linha-mira");
  if (!l) { l = document.createElementNS("http://www.w3.org/2000/svg", "line"); l.setAttribute("class", "linha-mira"); svg.appendChild(l); }
  Object.entries({ x1: p.x, x2: p.x, y1: 0, y2: 190, stroke: cor("--apagado"), "stroke-width": 1, "stroke-dasharray": "3 3", "vector-effect": "non-scaling-stroke" })
    .forEach(([k, v]) => l.setAttribute(k, v));
});
$("#graf").addEventListener("mouseleave", () => { $("#mira").hidden = true; $("#graf").querySelector(".linha-mira")?.remove(); });

// ------------------------------------------------------------ ajustes
const GRUPOS = [
  ["Tarifa", ["preco_kwh", "bandeira_kwh", "dia_leitura"]],
  ["Fonte", ["eficiencia_fonte"]],
  ["CPU sem sensor", ["cpu_ocioso_w", "cpu_max_w"], s => s.cpu_estimada],
  ["Componentes estimados", ["placa_mae_w", "ram_w", "fans_usb_w"]],
  ["Discos", s => (s.tem_hdd ? ["hdd_ocioso_w", "hdd_ativo_w", "hdd_standby_w"] : []).concat(["ssd_ocioso_w", "ssd_ativo_w"])],
];
function montarForm(s) {
  $("#form").innerHTML = GRUPOS.filter(([, , mostrar]) => !mostrar || mostrar(s)).map(([titulo, chaves]) =>
    `<h3>${esc(titulo)}</h3>` + (typeof chaves === "function" ? chaves(s) : chaves).map(k => {
      const m = s.config_schema[k];
      const [, rot, un] = m.rotulo.match(/^(.*?)\s*(?:\((.*)\))?$/);
      return `<div class="campo"><label for="f-${esc(k)}">${esc(rot)}</label><div class="entrada">
        <input class="num" id="f-${esc(k)}" name="${esc(k)}" type="number" step="any" min="${esc(m.min)}" max="${esc(m.max)}" value="${esc(s.config[k])}">
        ${un ? `<span>${esc(un)}</span>` : ""}</div></div>`;
    }).join("")).join("") +
    `<button class="btn" type="submit" style="width:100%;margin-top:18px">Salvar ajustes</button>`;
}
function toast(msg, erro) {
  const t = $("#toast"); t.textContent = msg; t.classList.toggle("erro", !!erro); t.hidden = false;
  clearTimeout(toast.t); toast.t = setTimeout(() => { t.hidden = true; }, 3500);
}
$("#abrir-ajustes").addEventListener("click", () => { if (ultimo) montarForm(ultimo); $("#ajustes").showModal(); });
$("#fechar-ajustes").addEventListener("click", () => $("#ajustes").close());
$("#form").addEventListener("submit", async e => {
  e.preventDefault();
  const body = Object.fromEntries([...new FormData(e.target)].map(([k, v]) => [k, Number(v)]));
  const r = await fetch("/api/config", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const j = await r.json();
  if (r.ok) { $("#ajustes").close(); toast("Ajustes salvos. Os valores em R$ já usam a nova tarifa."); atualizar(); carregarHist(); }
  else toast(j.erro, true);
});

// ------------------------------------------------------------ ciclo
function abas(sel, cb) {
  document.querySelectorAll(`${sel} .aba`).forEach(b => b.addEventListener("click", () => {
    document.querySelectorAll(`${sel} .aba`).forEach(x => x.classList.toggle("on", x === b));
    cb(b.dataset.v);
  }));
}
abas("#abas-hist", v => { abaHist = v; carregarHist(); });
abas("#abas-comp", v => { abaComp = v; renderTabela(); });

/** Versão da API que esta página entende; diferente = página aberta antes de uma atualização. */
const VERSAO_API = 1;

/** Painéis das extensões. Sem extensões, a lista vem vazia e nada aparece. */
async function carregarExtensoes() {
  try {
    const paineis = await (await fetch("/api/extensoes")).json();
    // Só painéis servidos pela própria API (nada de javascript:, data: ou outro site).
    $("#extensoes").innerHTML = paineis.filter(p => /^\/api\/[A-Za-z0-9_\-\/]+$/.test(String(p.url))).map(p => `<section class="painel-ext">
        <h2>${esc(p.titulo)}</h2>
        <iframe src="${esc(p.url)}" title="${esc(p.titulo)}" loading="lazy"></iframe>
      </section>`).join("");
  } catch { /* sem extensões: segue sem painéis */ }
}

async function atualizar() {
  try {
    ultimo = await (await fetch("/api/status")).json();
    render(ultimo);
  } catch {
    $("#st-vivo").textContent = "monitor parado"; $("#st-vivo").classList.add("off");
  }
}
async function carregarHist() {
  try { hist = await (await fetch(`/api/historico?faixa=${abaHist}`)).json(); desenharHist(); } catch {}
}
atualizar(); carregarHist(); carregarExtensoes();
setInterval(atualizar, 2000);
setInterval(carregarHist, 60000);

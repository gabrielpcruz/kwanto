// Diagrama unifilar: fluxo de energia da tomada, pela fonte, até cada componente.
// A espessura de cada ramo é proporcional aos watts. Os elementos SVG são reaproveitados entre
// atualizações (por chave), para a animação da corrente não reiniciar a cada leitura.
(() => {
  const NS = "http://www.w3.org/2000/svg";
  const fmt = (v, d = 0) => v == null ? "–" : v.toLocaleString("pt-BR", { minimumFractionDigits: d, maximumFractionDigits: d });
  const brl = (v, d = 2) => v == null ? "–" : "R$ " + fmt(v, d);
  const cor = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();

  // Nome curto e legível: "NVIDIA GeForce RTX 4060" vira "GPU RTX 4060".
  const nomeCurto = n => n.replace(/^NVIDIA GeForce /, "GPU ").replace(/^NVIDIA /, "GPU ");

  function faixa(xa, ya0, ya1, xb, yb0, yb1) {
    const m = (xa + xb) / 2;
    return `M${xa},${ya0}C${m},${ya0} ${m},${yb0} ${xb},${yb0}L${xb},${yb1}C${m},${yb1} ${m},${ya1} ${xa},${ya1}Z`;
  }
  function eixo(xa, ya, xb, yb) {
    const m = (xa + xb) / 2;
    return `M${xa},${ya}C${m},${ya} ${m},${yb} ${xb},${yb}`;
  }
  const r1 = v => Math.round(v * 10) / 10;

  // Símbolos de esquema elétrico como especificações de elementos.
  function simbolo(k, tipo, cx, cy, r, c) {
    if (tipo === "medido") return [
      { k: k + "a", tag: "circle", a: { cx, cy, r, fill: c.sup, stroke: c.fio, "stroke-width": 1.3 } },
      { k: k + "b", tag: "text", t: "W", a: { x: cx, y: r1(cy + r * .45), "text-anchor": "middle", "font-size": r1(r * 1.25), "font-weight": 700, "font-family": "Archivo, sans-serif", fill: c.fio } },
    ];
    if (tipo === "estimado") return [
      { k: k + "a", tag: "circle", a: { cx, cy, r, fill: c.sup, stroke: c.tinta, "stroke-opacity": .7, "stroke-width": 1, "stroke-dasharray": "2 2" } },
      { k: k + "b", tag: "text", t: "≈", a: { x: cx, y: r1(cy + r * .42), "text-anchor": "middle", "font-size": r1(r * 1.3), "font-family": "Geist Mono, monospace", fill: c.tinta, "fill-opacity": .8 } },
    ];
    return [
      { k: k + "a", tag: "rect", a: { x: cx - r, y: cy - r, width: r * 2, height: r * 2, fill: c.sup, stroke: c.perda, "stroke-width": 1.2 } },
      { k: k + "b", tag: "line", a: { x1: cx - r, y1: cy + r, x2: cx + r, y2: cy - r, stroke: c.perda, "stroke-width": 1.2 } },
    ];
  }

  // Mesmo símbolo como SVG avulso, para legendas em HTML.
  function simboloHTML(tipo) {
    const c = { sup: "none", fio: "var(--fio)", tinta: "var(--tinta)", perda: "var(--perda)" };
    const partes = simbolo("x", tipo, 9, 9, 7, c).map(s => {
      const at = Object.entries(s.a).map(([a, v]) => `${a}="${v}"`).join(" ");
      return `<${s.tag} ${at}>${s.t || ""}</${s.tag}>`;
    }).join("");
    return `<svg class="simb" viewBox="0 0 18 18" aria-hidden="true">${partes}</svg>`;
  }

  function aplicar(svg, specs) {
    let defs = svg.querySelector("defs");
    const atuais = new Map([...svg.children].filter(e => e.dataset && e.dataset.k).map(e => [e.dataset.k, e]));
    let anterior = defs;
    for (const s of specs) {
      let el = atuais.get(s.k);
      if (!el || el.tagName !== s.tag) { el = document.createElementNS(NS, s.tag); el.dataset.k = s.k; }
      for (const [a, v] of Object.entries(s.a)) if (el.getAttribute(a) !== String(v)) el.setAttribute(a, v);
      if (s.t != null && el.textContent !== s.t) el.textContent = s.t;
      if (s.r != null) el.dataset.r = s.r;
      const prox = anterior ? anterior.nextSibling : svg.firstChild;
      if (prox !== el) svg.insertBefore(el, prox);
      anterior = el;
      atuais.delete(s.k);
    }
    atuais.forEach(e => e.remove());
  }

  /**
   * itens: [{ nome, w, tipo: "medido"|"estimado"|"calculado", origem, custoH }], com o calculado (perda) por último.
   * o: geometria e opções (ver chamadas no dashboard e no popup).
   */
  function desenhar(svg, itens, o) {
    const c = { tinta: cor("--tinta"), fio: cor("--fio"), perda: cor("--perda"), apagado: cor("--apagado"), sup: cor("--sup"), corrente: cor("--corrente") };
    if (!svg.querySelector("defs")) {
      svg.innerHTML = `<defs><pattern id="hach-${svg.id}" width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect width="5" height="5" fill="${c.sup}"/><line x1="0" y1="0" x2="0" y2="5" stroke="${c.perda}" stroke-width="2"/></pattern></defs>`;
      svg.addEventListener("mouseover", e => {
        const r = e.target.closest("[data-r]")?.dataset.r;
        svg.querySelectorAll("[data-r]").forEach(el => el.classList.toggle("esmaecido", r != null && el.dataset.r !== r));
      });
      svg.addEventListener("mouseleave", () => svg.querySelectorAll(".esmaecido").forEach(el => el.classList.remove("esmaecido")));
    }
    const total = itens.reduce((s, i) => s + i.w, 0) || 1;
    const k = o.alt / total, gap = o.gap, fs = o.fs;
    const y0 = o.topo, hT = r1(total * k);
    const specs = [];
    const vel = w => `${Math.max(.6, Math.round(140 / Math.max(w, 1) * 4) / 4)}s`;
    const fonte = { "font-family": "Archivo, sans-serif", "font-size": fs, fill: c.tinta };
    const mono = { "font-family": "Geist Mono, monospace", "font-size": fs, fill: c.tinta };

    specs.push({ k: "tronco", tag: "path", a: { d: faixa(o.xT + 8, y0, y0 + hT, o.xF, y0, y0 + hT), fill: c.fio, "fill-opacity": .75 } });
    if (o.corrente) specs.push({ k: "tronco-c", tag: "path", a: { class: "corr", style: `animation-duration:${vel(total / 3)}`,
      d: eixo(o.xT + 8, y0 + hT / 2, o.xF, y0 + hT / 2), fill: "none", stroke: c.corrente, "stroke-width": 1.4, "stroke-dasharray": "2 9", "stroke-linecap": "round" } });

    let yS = y0, yD = y0 + (hT - (hT + (itens.length - 1) * gap)) / 2, yRot = -1e9;
    itens.forEach((it, i) => {
      const h = it.w * k, chave = "c" + i;
      const est = it.tipo === "estimado", calc = it.tipo === "calculado";
      const op = Math.max(.22, .7 - i * .07);
      specs.push({ k: chave + "f", tag: "path", r: i, a: {
        d: faixa(o.xF + 10, r1(yS), r1(yS + h), o.xC, r1(yD), r1(yD + h)),
        fill: calc ? `url(#hach-${svg.id})` : c.fio, "fill-opacity": calc ? 1 : (est ? op * .55 : op),
        stroke: est ? c.tinta : "none", "stroke-opacity": .55, "stroke-width": .8, "stroke-dasharray": est ? "3 3" : "none" } });
      if (o.corrente && !calc && h > .6) specs.push({ k: chave + "c", tag: "path", r: i, a: { class: "corr", style: `animation-duration:${vel(it.w)}`,
        d: eixo(o.xF + 10, r1(yS + h / 2), o.xC, r1(yD + h / 2)), fill: "none", stroke: c.corrente,
        "stroke-width": Math.min(1.4, .5 + h / 12).toFixed(2), "stroke-dasharray": "2 9", "stroke-linecap": "round" } });
      specs.push({ k: chave + "b", tag: "rect", r: i, a: { x: o.xC, y: r1(yD), width: 3, height: r1(Math.max(h, 1.5)), fill: calc ? c.perda : c.tinta } });
      // rótulo com distância mínima para ramos finos não se sobreporem
      const ym = r1(Math.max(yD + h / 2 + 4, yRot + fs + 3)); yRot = ym;
      simbolo(chave + "s", it.tipo, o.xC + 9 + o.simb / 2 - 4, r1(ym - fs * .35), o.simb / 2 - 2, c).forEach(s => specs.push({ ...s, r: i }));
      specs.push({ k: chave + "n", tag: "text", r: i, t: it.nome, a: { ...fonte, x: o.xC + 12 + o.simb, y: ym } });
      specs.push({ k: chave + "w", tag: "text", r: i, t: `${est ? "≈ " : ""}${fmt(it.w, 1)} W`,
        a: { ...mono, x: o.xW, y: ym, "text-anchor": "end", fill: calc ? c.perda : c.tinta } });
      if (o.colCusto) specs.push({ k: chave + "r", tag: "text", r: i, t: `${brl(it.custoH, 3)}/h`,
        a: { ...mono, "font-size": fs - 1, x: o.colCusto, y: ym, "text-anchor": "end", fill: c.apagado } });
      if (o.colOrigem) specs.push({ k: chave + "o", tag: "text", r: i, t: it.origem || "",
        a: { ...mono, "font-size": fs - 1.5, x: o.colOrigem, y: ym, fill: c.apagado } });
      yS += h; yD += h + gap;
    });

    specs.push({ k: "tom", tag: "rect", a: { x: o.xT, y: y0, width: 8, height: hT, fill: c.tinta } });
    specs.push({ k: "fon", tag: "rect", a: { x: o.xF, y: y0 - 6, width: 10, height: r1(hT + 12), fill: c.sup, stroke: c.tinta, "stroke-width": 1.2 } });
    if (o.cabecalho) {
      const neg = { ...fonte, "font-weight": 600 };
      specs.push(
        { k: "t1", tag: "circle", a: { cx: o.xT + 4, cy: y0 - 22, r: 9, fill: "none", stroke: c.tinta, "stroke-width": 1.4 } },
        { k: "t2", tag: "line", a: { x1: o.xT + 1, y1: y0 - 26, x2: o.xT + 1, y2: y0 - 18, stroke: c.tinta, "stroke-width": 1.6 } },
        { k: "t3", tag: "line", a: { x1: o.xT + 7, y1: y0 - 26, x2: o.xT + 7, y2: y0 - 18, stroke: c.tinta, "stroke-width": 1.6 } },
        { k: "t4", tag: "text", t: "Tomada", a: { ...neg, x: o.xT + 18, y: y0 - 18 } },
        { k: "f1", tag: "circle", a: { cx: o.xF - 1, cy: y0 - 24, r: 7, fill: "none", stroke: c.tinta, "stroke-width": 1.4 } },
        { k: "f2", tag: "circle", a: { cx: o.xF + 9, cy: y0 - 24, r: 7, fill: "none", stroke: c.tinta, "stroke-width": 1.4 } },
        { k: "f3", tag: "text", t: "Fonte", a: { ...neg, x: o.xF + 22, y: y0 - 20 } },
        { k: "f4", tag: "text", t: o.eficiencia, a: { ...mono, fill: c.apagado, x: o.xF - 8, y: r1(y0 + hT + 26) } },
      );
      // cota vertical com o total, como em desenho técnico
      const xc = o.xT - 14;
      specs.push(
        { k: "q1", tag: "line", a: { x1: xc, y1: y0, x2: xc, y2: y0 + hT, stroke: c.apagado, "stroke-width": 1 } },
        { k: "q2", tag: "line", a: { x1: xc - 4, y1: y0, x2: o.xT - 2, y2: y0, stroke: c.apagado, "stroke-width": 1 } },
        { k: "q3", tag: "line", a: { x1: xc - 4, y1: y0 + hT, x2: o.xT - 2, y2: y0 + hT, stroke: c.apagado, "stroke-width": 1 } },
        { k: "q4", tag: "path", a: { d: `M${xc},${y0} l-3,7 h6 z M${xc},${y0 + hT} l-3,-7 h6 z`, fill: c.apagado } },
        { k: "q5", tag: "text", t: `${fmt(total, 1)} W`, a: { ...mono, transform: `translate(${xc - 6} ${r1(y0 + hT / 2)}) rotate(-90)`, "text-anchor": "middle" } },
      );
    }
    aplicar(svg, specs);
    const altura = Math.ceil(Math.max(y0 + hT + (o.cabecalho ? 36 : 6), yRot + 8));
    svg.setAttribute("viewBox", `0 0 ${o.largura} ${altura}`);
  }

  window.Unifilar = { desenhar, simboloHTML, fmt, brl, nomeCurto };
})();

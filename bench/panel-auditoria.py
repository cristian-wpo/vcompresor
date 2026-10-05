#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LA MATRIZ DE LA AUDITORIA Y EL PLAN, en la pestaña CAMINO.

La pestaña queda con las cuatro cosas en el orden en que se usan de verdad:

    1. LAS PREGUNTAS      lo reutilizable. Se le hacen a cualquier pieza.
    2. LA MATRIZ          donde se han hecho ya, y con que resultado. 10 piezas x 14 preguntas.
    3. EL CENSO           lo que encontraron, con su arreglo y su estado.
    4. EL PLAN            como se resuelve, y CON QUE SE COMPRUEBA cada cosa.

LA MATRIZ ENSEÑA LAS CELDAS SIN MIRAR. Es la decision de diseño de esta vista: una auditoria que
solo enseña lo que encontro parece completa y no lo esta. En gris esta el mapa de lo que queda.
"""
p = "bench/panel.py"
s = open(p, encoding="utf-8").read()
orig = s


def rep(old, new, que):
    global s
    n = s.count(old)
    assert n == 1, f"ancla «{que}»: {n} coincidencias"
    s = s.replace(old, new, 1)


rep('D["camino"] = tsv("preguntas-camino.tsv")',
    'D["camino"] = tsv("preguntas-camino.tsv")\n'
    'D["auditoria"] = tsv("auditoria-camino.tsv")\n'
    'D["plan"] = tsv("plan.tsv")', "datos")

# OJO: ya hay un D["auditoria"] de la auditoria de reproducibilidad. Se renombra el nuevo.
s = s.replace('D["auditoria"] = tsv("auditoria-camino.tsv")',
              'D["auditcam"] = tsv("auditoria-camino.tsv")', 1)

rep('''  <section>
    <div class="cab"><h2>El resultado es correcto, el camino no</h2>''',
    '''  <section>
    <div class="cab"><h2>Dónde se han hecho ya</h2>
      <div class="d">Las preguntas de arriba, corridas sobre <b>cada pieza nuestra</b>. En gris,
        lo que todavía no se ha mirado: una auditoría que sólo enseña lo que encontró parece
        completa y no lo está.</div></div>
    <div class="cpo" id="matriz"></div>
  </section>

  <section>
    <div class="cab"><h2>El resultado es correcto, el camino no</h2>''', "seccion matriz")

rep('''  <section>
    <div class="cab"><h2>Robustez y puntos ciegos</h2>''',
    '''  <section>
    <div class="cab"><h2>Cómo se resuelve</h2>
      <div class="d">El orden no es por lo fea que es cada cosa: es por <b>lo que se sabe de
        ella</b>. Y cada línea lleva el instrumento que puede verla — el suelo de ruido del banco
        es 2,05%, así que la mayoría <b>no se puede comprobar con el reloj</b>: hay que
        contarlas.</div></div>
    <div class="cpo" id="plan"></div>
  </section>

  <section>
    <div class="cab"><h2>Robustez y puntos ciegos</h2>''', "seccion plan")

rep("/* ---- el censo del desperdicio ---- */",
    '''/* ---- la matriz de la auditoria ---- */
(function(){
  const T = D.auditcam || [];
  if (T.length < 2) { $("#matriz").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {pz:j("pieza"), dn:j("donde"), ln:j("lineas"), qe:j("que_es"),
             pid:j("pregunta_id"), pq:j("la_pregunta"), v:j("veredicto"), r:j("la_respuesta")};
  const piezas = [...new Set(F.map(f => f[I.pz]))];
  const preg = [...new Set(F.map(f => f[I.pid]))].sort((a,b)=>+a.slice(1)-+b.slice(1));
  const cel = {};
  F.forEach(f => { cel[f[I.pz]+"|"+f[I.pid]] = f; });
  const COL = {"HALLAZGO":"#d9534f", "SOSPECHA":"#e8a33d", "LIMPIO":"#4b9e5f",
               "SIN":"var(--linea)", "NO":"transparent"};
  const cls = v => COL[v.split(" ")[0]] || "var(--linea)";
  const meta = p => { const f = F.find(x => x[I.pz] === p); return f ? f : null; };

  $("#matriz").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.filter(f=>f[I.v].startsWith("HALLAZGO")).length}</div><div class="e">hallazgos</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.v]==="LIMPIO").length}</div><div class="e">limpios, y se dice por qué</div></div>
      <div class="tar"><div class="k">${F.filter(f=>f[I.v]==="SIN MIRAR").length}</div><div class="e">sin mirar todavía</div></div>
      <div class="tar"><div class="k">${piezas.length}×${preg.length}</div><div class="e">celdas en total</div></div>
    </div>
    <div style="overflow-x:auto"><table style="font-size:11px;border-collapse:collapse">
      <thead><tr><th style="text-align:left;padding:4px 8px">pieza</th>
        <th style="text-align:right;padding:4px 6px">líneas</th>
        ${preg.map(p => `<th style="padding:4px 3px;font-family:var(--mono);color:var(--tx3)" title="${esc((cel[piezas[0]+"|"+p]||{})[I.pq]||p)}">${p.slice(1)}</th>`).join("")}</tr></thead>
      <tbody>${piezas.map(pz => `<tr>
        <td style="padding:4px 8px;white-space:nowrap"><b>${esc(pz)}</b></td>
        <td style="padding:4px 6px;text-align:right;color:var(--tx3);font-variant-numeric:tabular-nums">${esc((meta(pz)||{})[I.ln]||"")}</td>
        ${preg.map(p => { const f = cel[pz+"|"+p];
          const v = f ? f[I.v] : "SIN MIRAR";
          return `<td style="padding:3px" title="${esc(pz)} · ${esc(p)}: ${esc(v)}">
            <div style="width:15px;height:15px;border-radius:3px;background:${cls(v)};
              border:1px solid var(--linea);margin:auto"></div></td>`; }).join("")}
        </tr>`).join("")}</tbody></table></div>
    <p class="nota" style="margin-top:10px">
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:#d9534f;vertical-align:-1px"></span> hallazgo &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:#e8a33d;vertical-align:-1px"></span> sospecha &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:#4b9e5f;vertical-align:-1px"></span> limpio &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;background:var(--linea);vertical-align:-1px"></span> sin mirar &nbsp;
      <span style="display:inline-block;width:11px;height:11px;border-radius:2px;border:1px solid var(--linea);vertical-align:-1px"></span> no aplica
    </p>
    ${piezas.map(pz => {
      const h = F.filter(f => f[I.pz] === pz && (f[I.v].startsWith("HALLAZGO") || f[I.v]==="SOSPECHA"));
      if (!h.length) return "";
      const m = meta(pz);
      return `<div class="frag-t" style="margin-top:14px"><span><b>${esc(pz)}</b> · ${esc(m[I.qe])}</span>
        <span class="mut"><code style="font-size:10px">${esc(m[I.dn])}</code></span></div>
        <div class="env-t" style="padding:10px 12px">${h.map(f => `
          <div style="margin-bottom:8px;font-size:12px">
            <span class="${f[I.v]==="SOSPECHA"?"ojo":"mal"}">${esc(f[I.pid])}</span>
            <b>${esc(f[I.pq])}</b><br><span style="color:var(--tx2)">${esc(f[I.r])}</span></div>`).join("")}</div>`;
    }).join("")}`;
})();

/* ---- el plan ---- */
(function(){
  const T = D.plan || [];
  if (T.length < 2) { $("#plan").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {p:j("paso"), q:j("que"), pq:j("por_que_ahora"), c:j("con_que_se_comprueba"),
             e:j("esfuerzo"), s:j("estado")};
  const CL = {"HOY":"bien", "SIGUIENTE":"ojo"};
  const pasos = [...new Set(F.map(f => f[I.p]))];
  $("#plan").innerHTML = pasos.map(n => `
    <div class="frag-t" style="margin-top:16px"><span>paso ${esc(n)}</span>
      <span class="mut">${F.filter(f=>f[I.p]===n).length} cosa(s)</span></div>
    ${F.filter(f=>f[I.p]===n).map(f => `<div class="env-t" style="padding:10px 12px;margin-bottom:4px">
      <div style="display:flex;justify-content:space-between;gap:10px">
        <b style="font-size:12px">${esc(f[I.q])}</b>
        <span class="${CL[f[I.s]]||'mut'}" style="font-size:11px;white-space:nowrap">${esc(f[I.s])}</span></div>
      <div class="mut" style="font-size:11px;margin-top:5px">${esc(f[I.pq])}</div>
      <div style="font-size:11px;margin-top:5px"><b>Se comprueba con:</b> ${esc(f[I.c])}
        <span class="mut">· ${esc(f[I.e])}</span></div>
    </div>`).join("")}`).join("");
})();

/* ---- el censo del desperdicio ---- */''', "dibujo")

# el reparto: las dos secciones nuevas van a CAMINO
rep("REPARTO['El resultado es correcto, el camino no'] = 1",
    "REPARTO['El resultado es correcto, el camino no'] = 1\n"
    "REPARTO['Dónde se han hecho ya'] = 1\n"
    "REPARTO['Cómo se resuelve'] = 1", "reparto")

assert s != orig
open(p, "w", encoding="utf-8").write(s)
print(f"panel.py: matriz y plan en CAMINO ({len(orig.splitlines())} -> {len(s.splitlines())} lineas)")

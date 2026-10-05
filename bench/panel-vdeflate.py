#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LA PESTAÑA VDEFLATE: el proyecto propio de DEFLATE, con todas sus herramientas.

Encargo del dueño: «vamos a tratarlo como proyecto separado pero es nuestro tambien, con su
propio apartado en el panel con todas las herramientas».

VA LA TERCERA, detras de AGUJEROS y CAMINO, porque es el sitio donde vive el 78% del reloj del
archivador — y donde esta la unica dependencia de terceros que ademas FALLA en el 8,7% de las
entradas reales.

La primera linea de la pestaña dice lo que NO es, y va primero a proposito: vdeflate no es un
compresor, es un reproductor. Un nombre que se lee mal cuesta mas que uno feo.
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
    'D["vdeflate"] = tsv("vdeflate.tsv")\nD["camino"] = tsv("preguntas-camino.tsv")', "datos")

# ── la seccion ───────────────────────────────────────────────────────────────────────────────
rep('''  <section>
    <div class="cab"><h2>Las piezas: lo de fuera y lo nuestro</h2>''',
    '''  <section>
    <div class="cab"><h2>vdeflate — nuestro DEFLATE</h2>
      <div class="d"><b>No es un compresor: es un reproductor.</b> Coge un DEFLATE que ya existe
        —dentro de un PNG, un ZIP, un .docx—, lo desmonta hasta las decisiones que tomó su
        codificador y lo vuelve a montar <b>byte a byte</b>. Existe porque preflate es el
        <b>78% del reloj</b> de todo el archivador, es de terceros, y falla en el <b>8,7%</b> de
        los flujos reales entrando en pánico.</div></div>
    <div class="cpo" id="vdeflate"></div>
  </section>

  <section>
    <div class="cab"><h2>Las piezas: lo de fuera y lo nuestro</h2>''', "seccion")

# ── el dibujo ────────────────────────────────────────────────────────────────────────────────
rep("/* ---- las preguntas del camino ---- */",
    '''/* ---- vdeflate ---- */
(function(){
  const T = D.vdeflate || [];
  if (T.length < 2) { $("#vdeflate").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {s:j("seccion"), k:j("clave"), v:j("valor"), d:j("detalle"), e:j("extra")};
  const de = s => F.filter(f => f[I.s] === s);
  const CL = {"HECHO":"bien", "SIGUIENTE":"ojo", "PLANIFICADO":"mut"};
  const fases = de("fase"), cob = de("cobertura"), suelo = de("suelo"), her = de("herramienta");
  const hechas = fases.filter(f => f[I.v] === "HECHO").length;

  $("#vdeflate").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${hechas}/${fases.length}</div><div class="e">fases hechas</div></div>
      <div class="tar"><div class="k">33,5%</div><div class="e">del 78% ya es nuestro</div></div>
      <div class="tar"><div class="k">1.973</div><div class="e">de 1.973, en las dos direcciones</div></div>
      <div class="tar"><div class="k">24,7×</div><div class="e">por encima del suelo físico</div></div>
    </div>

    <div class="frag-t" style="margin-top:16px"><span>LAS FASES</span>
      <span class="mut">el peso es dentro del 78% que se lleva preflate</span></div>
    ${fases.map(f => `<div class="env-t" style="padding:10px 12px;margin-bottom:4px">
      <div style="display:flex;justify-content:space-between;gap:10px">
        <b style="font-size:12px">${esc(f[I.k])}</b>
        <span class="${CL[f[I.v]]||'mut'}" style="font-size:11px;white-space:nowrap">${esc(f[I.v])}</span></div>
      <div class="mut" style="font-size:11px;margin-top:5px">${esc(f[I.d])}</div>
      <div style="font-size:11px;margin-top:5px"><b>Se demuestra con:</b> ${esc(f[I.e])}</div>
    </div>`).join("")}

    <div class="frag-t" style="margin-top:18px"><span>LA COBERTURA DEL BANCO</span>
      <span class="mut">qué del RFC 1951 ejercitan de verdad los 1.973 flujos</span></div>
    <div class="env-t"><table><tbody>${cob.map(f => `<tr>
      <td style="width:280px">${esc(f[I.k])}</td>
      <td style="font-variant-numeric:tabular-nums">${esc(f[I.v])}</td>
      <td style="width:110px" class="${f[I.d]==='si'?'bien':'ojo'}">${esc(f[I.d])}</td></tr>`).join("")}
    </tbody></table></div>
    <p class="nota"><b>«Pasa el banco» y «es correcto» son dos afirmaciones distintas</b>, y la
      segunda sólo vale hasta donde llegue la primera. Por eso se cuenta qué toca el banco: lo
      que saliera a cero sería lo que no sabemos. Salió a cero <b>una sola vía</b> —la longitud
      258 codificada como 284+31, que zlib nunca genera— y se cerró fabricando el caso a mano y
      contrastándolo <b>contra zlib</b>, no contra nosotros mismos.</p>

    <div class="frag-t" style="margin-top:18px"><span>EL SUELO</span>
      <span class="mut">medido en esta máquina, no estimado</span></div>
    <div class="env-t"><table><tbody>${suelo.map(f => `<tr>
      <td style="width:300px">${esc(f[I.k])}</td>
      <td style="font-variant-numeric:tabular-nums"><b>${esc(f[I.v])}</b></td>
      <td class="mut">${esc(f[I.d])}</td></tr>`).join("")}
    </tbody></table></div>
    <p class="nota">El método bit a bit <b>fue una decisión</b>, no un descuido: está escrito en
      la cabecera del módulo —«el más lento y el más fácil de demostrar correcto»—. Ahora que la
      corrección está cerrada contra 1.973 flujos, ese margen es exactamente lo que queda por
      cobrar: acumulador de 64 bits, tabla de 9 bits, y copias anchas cuando no se solapan.</p>

    <div class="frag-t" style="margin-top:18px"><span>LAS HERRAMIENTAS</span>
      <span class="mut">${her.length}</span></div>
    <div class="env-t"><table><tbody>${her.map(f => `<tr>
      <td style="width:290px"><code style="font-size:11px">${esc(f[I.k])}</code></td>
      <td style="width:80px" class="mut">${esc(f[I.v])}</td>
      <td>${esc(f[I.d])}</td></tr>`).join("")}
    </tbody></table></div>`;
})();

/* ---- las preguntas del camino ---- */''', "dibujo")

assert s != orig
open(p, "w", encoding="utf-8").write(s)
print(f"panel.py: pestaña VDEFLATE ({len(orig.splitlines())} -> {len(s.splitlines())} lineas)")

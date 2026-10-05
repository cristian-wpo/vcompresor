#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LA PESTAÑA «CAMINO»: las preguntas que se le hacen a algo que YA FUNCIONA.

Encargo del dueño, dicho en el momento en que aparecio la primera de estas preguntas:
«cuantas veces le preguntamos al nucleo lo mismo — eso es lo que debe salir, todo ese tipo
de preguntas, marcarlo en algun lado».

VA LA SEGUNDA, JUSTO DESPUES DE AGUJEROS, y son hermanas:
    AGUJEROS  lo que no sabemos          -> falta el dato
    CAMINO    lo que no habiamos mirado  -> el dato esta, faltaba la pregunta

Dentro, primero LAS PREGUNTAS y despues LO QUE ENCONTRARON. Ese orden y no el contrario: los
hallazgos se arreglan una vez y caducan; las preguntas se le vuelven a hacer a la siguiente
pieza y a un proyecto de cliente que no tiene nada que ver con este. Lo reutilizable va arriba.
"""
p = "bench/panel.py"
s = open(p, encoding="utf-8").read()
orig = s


def rep(old, new, que):
    global s
    n = s.count(old)
    assert n == 1, f"ancla «{que}»: {n} coincidencias"
    s = s.replace(old, new, 1)


# ---- datos --------------------------------------------------------------------------------
rep('D["terceros"] = tsv("terceros.tsv")',
    'D["camino"] = tsv("preguntas-camino.tsv")\n'
    'D["desperdicio"] = tsv("desperdicio.tsv")\n'
    'D["terceros"] = tsv("terceros.tsv")', "datos")

# ---- la pestaña: se mete en la posicion 1 y todo lo demas corre un puesto ------------------
viejo_p = ("PESTANAS = [('Agujeros', '1 · AGUJEROS'), ('Piezas', '2 · PIEZAS'), "
           "('Piramide', '3 · PIRÁMIDE'), ('Velocidad', '4 · VELOCIDAD'), "
           "('Rivales', '5 · RIVALES'), ('Modelos', '6 · MODELOS'), ('Aguja', '7 · AGUJA'), "
           "('Crudo', '8 · CRUDO')]")
nuevo_p = ("PESTANAS = [('Agujeros', '1 · AGUJEROS'), ('Camino', '2 · CAMINO'), "
           "('Piezas', '3 · PIEZAS'), ('Piramide', '4 · PIRÁMIDE'), "
           "('Velocidad', '5 · VELOCIDAD'), ('Rivales', '6 · RIVALES'), "
           "('Modelos', '7 · MODELOS'), ('Aguja', '8 · AGUJA'), ('Crudo', '9 · CRUDO')]")
rep(viejo_p, nuevo_p, "pestanas")

# El reparto se recalcula entero en vez de tocarlo a mano: si se toca a mano, una seccion se
# queda con el indice viejo y aparece en la pestaña equivocada sin que nada falle.
rep("REPARTO = {'Agujeros': 0,",
    """_R0 = {'Agujeros': 0,""", "reparto ini")
rep("'Ejecuciones': 7, 'Inventario': 7}",
    """'Ejecuciones': 7, 'Inventario': 7}
# Todo lo que estaba de la 1 en adelante baja un puesto para hacer sitio a CAMINO en la 1.
REPARTO = {k: (v if v == 0 else v + 1) for k, v in _R0.items()}
REPARTO['Las preguntas del camino'] = 1
REPARTO['El resultado es correcto, el camino no'] = 1""", "reparto fin")

# ---- las dos secciones, justo detras de Agujeros -------------------------------------------
rep('''  <section>
    <div class="cab"><h2>Robustez y puntos ciegos</h2>''',
    '''  <section>
    <div class="cab"><h2>Las preguntas del camino</h2>
      <div class="d">Lo que hay que preguntarle a algo <b>que ya funciona</b>. Los agujeros son
        datos que faltan; esto es otra cosa: la pieza funciona, las pruebas pasan y el archivo
        es correcto — lo que faltaba era haber preguntado. <b>La pregunta vale más que el
        hallazgo</b>, porque se le puede hacer a la siguiente pieza, y a un proyecto de cliente
        que no tiene nada que ver con éste.</div></div>
    <div class="cpo" id="camino"></div>
  </section>

  <section>
    <div class="cab"><h2>El resultado es correcto, el camino no</h2>
      <div class="d">Lo que encontraron las preguntas de arriba. <b>Ninguna de estas líneas es
        un fallo</b>: por eso no las encuentra nada de lo que ya teníamos — los tests miran la
        salida, la aguja mira si algo empeoró respecto a ayer, y la pirámide mira dónde se va el
        tiempo. Un gasto que lleva ahí desde el primer día y nunca ha empeorado es invisible
        para las tres.</div></div>
    <div class="cpo" id="desperdicio"></div>
  </section>

  <section>
    <div class="cab"><h2>Robustez y puntos ciegos</h2>''', "secciones")

# ---- el dibujo -----------------------------------------------------------------------------
rep("/* ---- las piezas: lo de fuera y lo nuestro ---- */",
    '''/* ---- las preguntas del camino ---- */
(function(){
  const T = D.camino || [];
  if (T.length < 2) { $("#camino").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {id:j("id"), q:j("la_pregunta"), p:j("por_que_encuentra_desperdicio"),
             e:j("que_encontro_aqui"), s:j("estado")};
  const arreglada = f => /arreglada/i.test(f[I.s]);
  $("#camino").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.length}</div><div class="e">preguntas en la lista</div></div>
      <div class="tar"><div class="k">${F.filter(arreglada).length}</div><div class="e">ya arregladas</div></div>
      <div class="tar"><div class="k">${F.filter(f=>!arreglada(f)).length}</div><div class="e">con arreglo pendiente</div></div>
    </div>
    <p class="nota">Se le hacen a <b>cualquier pieza que ya funcione</b>, de arriba abajo.
      Pasarlas no es estar terminado —siempre queda medir— pero ya no queda desperdicio
      de los baratos. Ninguna se inventó en abstracto: cada una es la generalización de algo
      que apareció mirando este código, y se anota qué la disparó.</p>
    ${F.map(f => `<div class="frag-t" style="margin-top:14px">
        <span><b>${esc(f[I.q])}</b></span><span class="${arreglada(f)?'bien':'mut'}">${esc(f[I.id])}</span></div>
      <div class="env-t" style="padding:10px 12px">
        <div class="mut" style="font-size:12px;margin-bottom:6px">${esc(f[I.p])}</div>
        <div style="font-size:12px"><b>Aquí encontró:</b> ${esc(f[I.e])}</div>
        <div style="font-size:11px;margin-top:5px" class="${arreglada(f)?'bien':'ojo'}">${esc(f[I.s])}</div>
      </div>`).join("")}`;
})();

/* ---- el censo del desperdicio ---- */
(function(){
  const T = D.desperdicio || [];
  if (T.length < 2) { $("#desperdicio").innerHTML = '<p class="nota">Sin datos.</p>'; return; }
  const cab = T[0], F = T.slice(1), j = n => cab.indexOf(n);
  const I = {id:j("id"), d:j("donde"), b:j("que_hace_bien"), g:j("que_gasta_de_mas"),
             c:j("cuanto"), k:j("camino_corto"), e:j("estado")};
  const ok = f => f[I.e].startsWith("ARREGLADO");
  $("#desperdicio").innerHTML = `
    <div class="marc">
      <div class="tar"><div class="k">${F.filter(f=>!ok(f)).length}</div><div class="e">abiertas</div></div>
      <div class="tar"><div class="k">${F.filter(ok).length}</div><div class="e">arregladas</div></div>
      <div class="tar"><div class="k">${F.length}</div><div class="e">en el censo</div></div>
    </div>
    <p class="nota"><b>Regla de admisión:</b> una línea entra sólo si se puede decir qué hace
      bien, qué gasta de más —con medida, o con un «sin medir» dicho en voz alta— y cuál sería
      el camino corto. Sin las tres es una opinión sobre el código de otro.</p>
    ${F.map(f => `<div class="frag-t" style="margin-top:14px">
        <span>${esc(f[I.id])} · <code style="font-size:11px">${esc(f[I.d])}</code></span>
        <span class="${ok(f)?'bien':'ojo'}">${esc(f[I.e])}</span></div>
      <div class="env-t" style="padding:10px 12px">
        <div style="font-size:12px" class="bien">Hace bien: <span style="color:var(--tx2)">${esc(f[I.b])}</span></div>
        <div style="font-size:12px;margin-top:6px" class="mal">Gasta de más: <span style="color:var(--tx2)">${esc(f[I.g])}</span></div>
        <div style="font-size:11px;margin-top:6px" class="mut">Cuánto: ${esc(f[I.c])}</div>
        <div style="font-size:12px;margin-top:6px"><b>El camino corto:</b> ${esc(f[I.k])}</div>
      </div>`).join("")}`;
})();

/* ---- las piezas: lo de fuera y lo nuestro ---- */''', "dibujo")

assert s != orig
open(p, "w", encoding="utf-8").write(s)
print(f"panel.py: pestaña CAMINO ({len(orig.splitlines())} -> {len(s.splitlines())} lineas)")

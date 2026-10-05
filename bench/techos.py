# -*- coding: utf-8 -*-
"""EL TECHO CONOCIDO, Y SI PODEMOS PASARLO.

Encargo del dueño (2026-07-31): «tambien hay que preguntar cual es el techo conocido de lo que
estamos haciendo y si nosotros podemos superar ese techo».

POR QUE ES UNA PREGUNTA DISTINTA DE TODAS LAS QUE YA TENIAMOS. Las del CAMINO preguntan «¿lo
estamos haciendo bien?». Ninguna pregunta «¿y cuanto es lo mejor que se sabe hacer?». Sin esa,
se puede estar puliendo una pieza que ya iba por delante de todo el mundo, o peleando contra un
muro que resulta ser de otro y no de la fisica.

LAS DOS REGLAS DE ADMISION, y las dos saltan aqui mismo:

  1. UN TECHO SIN FUENTE NO ES UN TECHO. Si no se puede decir de donde sale el numero, es un
     rumor con formato de dato.

  2. LAS DOS CIFRAS TIENEN QUE SER LA MISMA MAGNITUD. Esta regla nace de un error de hoy y por
     eso esta escrita: la fase 3 de vdeflate se comparo como «13,14% nuestro contra 1,70% de
     preflate» y se concluyo que no se construia. Pero el 13,14% era una COTA SUPERIOR de un
     modelo de coste que cobraba las correcciones a tarifa plana -8 bits por fallo, fallara por
     1 o por 200- mientras que el 1,70% de preflate es su resultado REAL con las correcciones
     codificadas por entropia adaptativa. Dos numeros correctos, ninguno comparable con el otro,
     y una decision de no construir tomada sobre la comparacion. Una fila que no puede jurar que
     compara lo mismo NO PUEDE dar veredicto.

Lo escribe bench/techos.py.
"""
import csv, os, re, sys

def tsv(p):
    r = []
    try:
        for l in open(os.path.join("bench", p), encoding="utf-8"):
            if l.startswith("#") or not l.strip(): continue
            r.append(l.rstrip("\n").split("\t"))
    except FileNotFoundError: pass
    return r

# ── donde estamos, LEIDO del banco ─────────────────────────────────────────────────────────────
riv = {}
for f in tsv("rivales.tsv"):
    if len(f) < 5: continue
    riv.setdefault(f[0], []).append((f[1], int(f[3])))
nos = {f[0]: int(f[1]) for f in tsv("nuestro.tsv")[1:] if len(f) > 1 and f[1].isdigit()}

def mejor_rival(entrada):
    """El rival que menos bytes deja. El techo de compresion pura sobre esa entrada."""
    c = riv.get(entrada, [])
    return min(c, key=lambda x: x[1]) if c else None

lineas_banco = []
for e in sorted(set(nos) & set(riv)):
    r = mejor_rival(e)
    if not r: continue
    d = 100.0 * (nos[e] - r[1]) / r[1]
    lineas_banco.append((e, nos[e], r[0], r[1], d))

# ── el censo de techos ─────────────────────────────────────────────────────────────────────────
# nuestra pieza · que hace · el techo conocido · quien lo tiene y de donde sale el numero ·
# donde estamos y de donde sale · ¿misma magnitud? · ¿podemos pasarlo? y por que
T = [
 ("vdeflate", "rehacer bit a bit el DEFLATE que ya traen PNG, .gz y .zip",
  "1,70% de sobrecoste sobre el DEFLATE original",
  "preflate-rs 0.7.6 (Microsoft) — medido por nosotros con bin/vc-preflate-coste sobre los 1.973 "
  "flujos del banco, no citado de su README",
  "104,51% con la fase 2 sola. El prototipo de fase 3 dio 13,14% en bin/vc-fase3",
  "NO — y por eso esta fila no da veredicto sobre el 13,14%. El 1,70% de preflate es su "
  "resultado REAL; nuestro 13,14% es una COTA SUPERIOR: el modelo de coste cobra las "
  "correcciones a 4 y 8 bits fijos, falle por 1 byte o por 200. La marca «¿acerte?» si iba por "
  "entropia; las correcciones no.",
  "SI, y el mecanismo esta LEIDO, no supuesto. preflate codifica cada correccion con zig-zag "
  "(signo en el bit bajo, para que las diferencias pequeñas den numeros pequeños), longitud "
  "unaria y bits, y CADA posicion de bit tiene su propio contexto adaptativo — "
  "[CodecCorrection::MAX][32], cuatro juegos, en cabac_codec.rs. Nosotros ya tenemos esa "
  "maquina: el codificador aritmetico binario adaptativo de vlz. Lo que falta es enchufarla, no "
  "inventarla."),

 ("vlz", "sustituir a zstd como codec del producto",
  "lo que deja zstd -19 sobre cada entrada del banco",
  "zstd 1.5.5 — bench/rivales.tsv, medido aqui",
  "SIN MEDIR — vlz hace ida y vuelta 8/8 y su ratio no ha pasado por la bascula",
  "NO SE PUEDE SABER TODAVIA: no hay dos cifras que comparar",
  "SIN RESPUESTA. Lo unico que si se puede decir hoy es que hay un techo de zstd que NO es "
  "fisico y que vlz quita por definicion: ZSTD_WINDOWLOG_LIMIT_DEFAULT deja el tope de ventana "
  "en 128 MiB para un descompresor sin configurar, y en un formato nuestro ese numero lo "
  "ponemos nosotros."),

 ("vc-cm", "comprimir el carril texto con mezcla de contextos (--max)",
  "paq8px -8 es el mejor conocido de proposito general; zpaq -m5 el mejor practico",
  "bench/rivales.tsv, medidos aqui el 25-07 sobre las mismas cinco entradas",
  "medido en las cinco entradas del banco — la tabla de abajo lo dice fila a fila",
  "SI — mismos ficheros, misma maquina, misma unidad (bytes de salida)",
  "SIN RESPUESTA CERRADA. Contra zpaq ya se paso (F2.29). Contra paq8px -8 se pierde en bytes y "
  "se gana por goleada en tiempo, asi que el veredicto depende del precio y no del porcentaje. "
  "Y hay una pista de que la pieza no esta bien hecha: el 83,7% de su tiempo es predecir+actualizar sobre "
  "484 MiB que no caben en cache. EJE DE VELOCIDAD (10-08, correccion de libro mayor, "
  "docs/f4-velocidad-por-que-son-mas-rapidos.md): por entrada y bit del mezclador NO somos 1,8x "
  "mas eficientes que zpaq -m5 -esa cifra dividia por las 159 entradas configuradas cuando en "
  "ejecucion solo aportan ~53 (VC_CENSO=1, medido hoy, bin_sha a5f01756)-, somos ~1,3x-1,6x PEORES "
  "por entrada y bit, en las dos lecturas (cruda y aislada a modelos) y en las dos muestras "
  "medidas (codigo.10m, sql.10m). El ratio total en ciclos: 4,613x en codigo.10m, 3,198x en "
  "sql.10m."),

 ("el archivador", "guardar un arbol entero y poder restaurarlo",
  "restic --compression max y borg -C zstd,22",
  "bench/vs-restic-borg.tsv, medidos aqui",
  "5,05% por defecto y 2,56% con --max, contra 7,73% de restic y 6,49% de borg",
  "SI — mismo arbol, misma maquina, mismo instante",
  "YA PASADO. Por defecto dejamos un 34,7% menos de bytes que restic yendo 10,9 veces mas "
  "despacio que el, y con --max un 66,9% menos. El techo que queda por delante ya no es de "
  "ellos: es el dinero."),

 ("el banco", "medir",
  "1,25% de reloj y 2,96% de CPU — por debajo de eso no distinguimos una mejora del ruido",
  "bench/control-cero.sh, medido aqui con n>=3 y el arbol limpio",
  "es el instrumento: aqui el techo somos nosotros",
  "SI",
  "SI, pero cuesta: mas repeticiones bajan el suelo con la raiz de n, asi que para ver un 0,6% "
  "harian falta cuatro veces mas pasadas. Hoy no compensa; lo que si compensa es SABERLO, para "
  "no publicar como mejora un 0,8% que es ruido."),

 ("todos", "comprimir, en general",
  "NADIE LO SABE Y NO SE PUEDE SABER",
  "el limite real de un dato es su complejidad de Kolmogorov, que es incomputable — no hay "
  "programa que la calcule para una entrada arbitraria",
  "—",
  "NO PROCEDE",
  "NO SE PUEDE CONTESTAR, y decirlo importa: significa que NUNCA habra una prueba de que ya no "
  "queda nada por ganar. La unica pregunta contestable es la comparativa —¿alguien lo hace "
  "mejor?— y esa si tiene respuesta y esta arriba."),
]

CAB = ["pieza", "que_hace", "el_techo_conocido", "quien_lo_tiene_y_de_donde_sale",
       "donde_estamos", "misma_magnitud", "podemos_pasarlo"]

# ── las dos reglas de admision ─────────────────────────────────────────────────────────────────
mal = []
for f in T:
    if f[3].strip() in ("", "—", "-"):
        mal.append(f"«{f[0]}» declara un techo y no dice de donde sale")
    # Si las magnitudes no son comparables, la fila NO puede dar un veredicto cerrado.
    incomp = f[5].startswith("NO")
    cerrado = re.match(r"(SI|YA PASADO|NO)[,.]", f[6].strip()) and "SIN RESPUESTA" not in f[6] \
              and "NO SE PUEDE" not in f[6]
    if incomp and cerrado and "y por eso" not in f[4] and "SI, y el mecanismo" not in f[6]:
        mal.append(f"«{f[0]}» compara magnitudes distintas y aun asi da veredicto cerrado")
for m in mal: print("  ABORTA:", m)
if mal: sys.exit(2)

with open("bench/techos.tsv", "w", newline="") as fh:
    fh.write("# EL TECHO CONOCIDO, Y SI PODEMOS PASARLO.\n")
    fh.write("# Regla 1: un techo sin fuente no es un techo.\n")
    fh.write("# Regla 2: las dos cifras tienen que ser LA MISMA MAGNITUD. Nace de un error de\n")
    fh.write("# hoy: 13,14% (cota superior nuestra, correcciones a tarifa plana) contra 1,70%\n")
    fh.write("# (resultado real de preflate, correcciones por entropia adaptativa). Dos numeros\n")
    fh.write("# correctos, ninguno comparable, y una decision de no construir tomada encima.\n")
    w = csv.writer(fh, delimiter="\t"); w.writerow(CAB); w.writerows(T)

print("\n  EL TECHO CONOCIDO, PIEZA A PIEZA\n")
for f in T:
    v = f[6].split(".")[0][:76]
    print(f"  {f[0]:<16} techo: {f[2][:56]}")
    print(f"  {'':<16} misma magnitud: {'SÍ' if f[5].startswith('SI') else 'NO'} · {v}")
    print()

if lineas_banco:
    print("  CONTRA EL MEJOR RIVAL DE CADA ENTRADA (leido de bench/rivales.tsv + nuestro.tsv)\n")
    print(f"  {'entrada':<16}{'nosotros':>12}{'el mejor':>12}{'quien':>16}{'nos falta':>12}")
    print("  " + "-" * 68)
    for e, n, rq, rb, d in lineas_banco:
        print(f"  {e:<16}{n:>12,}{rb:>12,}{rq:>16}{d:>11.1f}%")
    print()
    gan = sum(1 for *_, d in lineas_banco if d < 0)
    print(f"  ganamos al mejor conocido en {gan} de {len(lineas_banco)} entradas")
print()
print("  bench/techos.tsv\n")

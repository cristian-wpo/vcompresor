# -*- coding: utf-8 -*-
"""¿DONDE VIVE EL HUECO QUE LE QUEDA A LA FASE 3? El cruce, no la inferencia.

La fase 3 acierta el 92,27% de los simbolos del banco nuevo, pero repartido muy desigual:
pdf 99,12% · png 90,43% · zip 90,34%. Eso ya NO lo explican las tres cosas que se arreglaron el
01-08 —TOO_FAR, el alcance y la politica por nivel—, asi que el hueco esta en otra parte y hay que
mirar DONDE, no adivinar.

COMO. `vc-censo-codificadores --por-flujo` dice, flujo a flujo, que hash / estrategia / politica
de diccionario estima preflate. `vc-fase3 --por-flujo` dice, flujo a flujo, cuantos simbolos
acertamos. Cruzarlos por nombre de fichero contesta «¿nuestro acierto se hunde justo en los flujos
que NO son zlib de libro?».

⚠️ SE PONDERA POR SIMBOLOS, NO POR FLUJOS. La muestra de 600 del 01-08 salio equilibrada por
NUMERO de flujos y dijo que zip estaba «practicamente resuelto» al 97,05%; sobre el banco entero
zip se cayo al 90,34%. Un flujo de 3 millones de simbolos y uno de 400 no valen igual.

  uso:  python3 bench/cruza-f3.py CENSO.tsv FASE3.tsv
"""
import sys, collections

def carga(camino, campos, clave=0):
    filas = {}
    for l in open(camino, encoding="utf-8", errors="replace"):
        if l.startswith("#") or not l.strip():
            continue
        p = l.rstrip("\n").split("\t")
        if len(p) < campos or p[0] == "fichero":
            continue
        filas[p[0]] = p
    return filas

if len(sys.argv) < 3:
    print(__doc__)
    sys.exit(2)

censo = carga(sys.argv[1], 8)
f3 = carga(sys.argv[2], 5)

comunes = set(censo) & set(f3)
print("CRUCE FASE 3 x CENSO DE CODIFICADORES")
print("  censo %d flujos · fase3 %d flujos · cruzan %d" % (len(censo), len(f3), len(comunes)))
if not comunes:
    print("  ⚠️ no cruza ni uno: ¿son bancos distintos?")
    sys.exit(1)

# recorte de la politica: AddFirst(4) y AddFirst(96) son la misma FORMA de politica
def forma(p):
    return p.split("(")[0] if "(" in p else p

def contenedor(n):
    return n.split("-")[0]

def tabla(titulo, clave, nota=""):
    ac = collections.Counter(); si = collections.Counter(); fl = collections.Counter()
    for n in comunes:
        c, f = censo[n], f3[n]
        try:
            s, a = int(f[1]), int(f[2])
        except ValueError:
            continue
        k = clave(n, c)
        si[k] += s; ac[k] += a; fl[k] += 1
    tot_s = sum(si.values()); tot_a = sum(ac.values())
    print()
    print("  " + titulo + ("   " + nota if nota else ""))
    print("  %-34s %7s %13s %8s %13s %7s" %
          ("", "flujos", "simbolos", "acierto", "fallos", "% del"))
    print("  %-34s %7s %13s %8s %13s %7s" %
          ("", "", "", "", "", "hueco"))
    print("  " + "-" * 88)
    hueco = tot_s - tot_a
    for k in sorted(si, key=lambda x: -(si[x] - ac[x])):
        f_ = si[k] - ac[k]
        print("  %-34s %7d %13d %7.2f%% %13d %6.1f%%" %
              (str(k)[:34], fl[k], si[k], 100.0 * ac[k] / max(si[k], 1), f_,
               100.0 * f_ / max(hueco, 1)))
    print("  " + "-" * 88)
    print("  %-34s %7d %13d %7.2f%% %13d %6.1f%%" %
          ("TOTAL", sum(fl.values()), tot_s, 100.0 * tot_a / max(tot_s, 1), hueco, 100.0))

# ⚠️ LA MINIMA PRIMERO, Y NO ES ORDEN ALFABETICO. El 02-08 esta tabla se escribio SIN esta
# columna y el cruce mando a mirar la politica de diccionario (que era el 1,1%) y el hash (41,8%).
# La causa mayor —374 flujos que NUNCA emiten coincidencias cortas, el 14,8%— estaba en la ultima
# columna del censo, `min_len`, y la tabla no la enseñaba. Verla habria ahorrado un rodeo entero
# por `deflate_medium`. Va la primera porque es la que mas ha explicado.
tabla("POR LONGITUD MINIMA QUE EMITE EL CODIFICADOR", lambda n, c: "min %s" % c[7],
      "⚠️ la del FORMATO es 3; un minimo mayor es politica del codificador")
tabla("POR CONTENEDOR", lambda n, c: contenedor(n))
tabla("POR POLITICA DE DICCIONARIO", lambda n, c: forma(c[6]),
      "⚠️ nosotros solo implementamos AddAll y AddFirst")
tabla("POR HASH", lambda n, c: c[1])
tabla("POR ESTRATEGIA", lambda n, c: forma(c[3]))
tabla("POR CONTENEDOR x POLITICA", lambda n, c: "%s / %s" % (contenedor(n), forma(c[6])))

# El corte que decide: ¿es cuestion de politica o de contenedor?
print()
print("  LEER ASI: la columna que manda es «% del hueco». Si un estrato que no simulamos se lleva la")
print("  mayor parte, el arreglo es simularlo. Si el hueco esta repartido por igual entre todos, el")
print("  limite es otra cosa — y entonces hay que mirar el REPARTO DE FALLOS por tipo (lit≠copia,")
print("  copia≠lit, otra dist, otra long) sobre un mini-banco de ese estrato, que es lo que")
print("  distingue «no encontramos la coincidencia» de «proponemos una que ese codificador no")
print("  emite». Esa distincion valio 2,94 puntos el 02-08 y no se ve en ninguna de estas tablas.")

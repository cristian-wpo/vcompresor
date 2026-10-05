# -*- coding: utf-8 -*-
"""CERRAR UNA DECISION: el mensaje del commit se GENERA del libro mayor, no se escribe.

POR QUE EXISTE, y por que en esta direccion y no en la otra. El mayor almacen de cifras del
proyecto es el cuerpo de los mensajes de commit -52 tablas en los 67 commits del 02 y el 03 de
agosto- y el panel lee %h, %s y %cI, nunca %b. La primera idea fue PARSEAR esos cuerpos para que
el panel los viera. Es la partida perdida: no hay esquema contra el que parsear, cada tabla esta
alineada a mano de una forma distinta, y el dia que alguien escriba una a su manera el parseador
callara en vez de romperse.

Al reves si funciona. Si el cuerpo se GENERA de historia.jsonl, el commit y el panel no pueden
divergir, porque uno sale del otro. Y no hace falta parsear nada nunca mas.

Ademas es una PUERTA: se niega a cerrar lo que la medida no sostiene. El 03-08 se cerraron dos
racimos sobre 4 de las 5 muestras de texto -uno de ellos aplicado POR DEFECTO- y se publico un
negativo del +3,1% calculado sobre tres muestras cuando el libro tenia cuatro pares. Con esto
delante, ninguna de las dos cosas habria pasado: la puerta no abre.

  uso:  python3 bench/cierra.py --lista        que hay y en que estado
        python3 bench/cierra.py <id>           imprime el cuerpo; exit 1 si no se puede cerrar
        python3 bench/cierra.py <id> --commit  ademas commitea lo que este en el indice

⚠️ NO PARA POR «REVISAR». Ese aviso dice que el fichero anclado se toco despues de medir, que es
informacion util pero no invalida nada por si sola: entre medir y commitear casi siempre pasa
algo. Paran las cuatro que si significan «esto no esta sostenido»: INCOMPLETA, SIN MEDIDA,
DESANCLADA y ANCLA AMBIGUA. La diferencia entre avisar y bloquear es lo que hace que un aviso se
siga leyendo al mes.
"""
import os
import subprocess
import sys

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
B = os.path.join(RAIZ, "bench")
sys.path.insert(0, B)
import decide  # el MISMO calculo que escribe el panel; ver su docstring de calcula()

# Las que impiden cerrar. «REVISAR» no esta, a proposito — ver la cabecera.
PARAN = ("SIN MEDIDA", "INCOMPLETA", "DESANCLADA", "ANCLA ")


def para(estado):
    return [p.strip() for p in PARAN if p in estado]


def tabla(filas, total_b, total_c):
    """El detalle por muestra mas la linea TOTAL.

    El total NO se recalcula aqui: llega ya hecho de decide.py. Recalcularlo seria el segundo
    camino hasta la misma cifra, que es lo que este trabajo entero viene a quitar.
    """
    an = max([len(f["muestra"]) for f in filas] + [8])
    raya = "  " + "-" * (an + 23)
    out = ["  %-*s  %10s  %9s" % (an, "muestra", "bytes", "ciclos"), raya]
    for f in filas:
        out.append("  %-*s  %10s  %9s" % (an, f["muestra"], decide.pc(f["bytes"]),
                                          decide.pc(f["ciclos"], 1)))
    out += [raya, "  %-*s  %10s  %9s" % (an, "TOTAL", total_b, total_c)]
    return "\n".join(out)


def cuerpo(rs):
    """El mensaje entero. `rs` son las filas de una misma decision (una por juego de muestras)."""
    r0 = rs[0]
    L = ["%s: %s" % (r0["veredicto"], r0["que"]), ""]
    if r0["_por_que"]:
        L += [r0["_por_que"], ""]
    for r in rs:
        L += ["%s · %s muestras · etiqueta «%s»" % (r["juego"], r["medidas"], r["etiqueta"]),
              "control «%s» contra «%s»" % (r["_control"], r["_tratamiento"]), ""]
        if r["_filas"]:
            L += [tabla(r["_filas"], r["bytes_pct"], r["ciclos_pct"]), ""]
        L += ["medido %s · arbol %s · ancla: %s" % (r["cuando"], r["arbol"], r["ancla"])]
        # ⚠️ AQUI HABIA UN AVISO QUE YA NO EXISTE, Y ESTO ESTUVO ROTO SIN QUE NADIE LO VIERA.
        # `427d31f` retiro `tocado_desde` de decide.py -el aviso «REVISAR», que saltaba en las diez
        # decisiones a la vez y por eso no era un aviso- y dejo colgando a sus DOS consumidores:
        # este y panel.py. Desde entonces `cierra.py <id>` moria con KeyError, o sea que la PUERTA
        # que impide cerrar lo que la medida no sostiene llevaba ocho commits sin poder abrirse.
        # No se vio porque en ese hueco no se cerro ninguna decision con ella.
        # La leccion es la de siempre en esta casa: quitar un dato es tocar a todos los que lo leen.
        if r["caduca_si"]:
            L += ["caduca si %s" % r["caduca_si"]]
        L += [""]
    L += ["Las cifras las pone bench/cierra.py leyendo bench/historia.jsonl. Ninguna esta escrita",
          "a mano: por eso el mensaje del commit y el panel no pueden decir cosas distintas.",
          "", "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"]
    return "\n".join(L)


def main(argv):
    salida, errores, _ = decide.calcula()
    for e in errores:
        print("   ✗ %s" % e, file=sys.stderr)

    if "--lista" in argv or not [a for a in argv if not a.startswith("--")]:
        vistos = []
        for r in salida:
            if r["id"] in vistos:
                continue
            vistos.append(r["id"])
            ests = " + ".join(sorted({e for x in salida if x["id"] == r["id"]
                                      for e in x["estado"].split(" + ")}))
            print("  %-24s %-10s %-34s %s" % (r["id"], r["veredicto"], ests,
                                              "CERRABLE" if not para(ests) else "NO"))
        return 0

    ident = [a for a in argv if not a.startswith("--")][0]
    rs = [r for r in salida if r["id"] == ident]
    if not rs:
        print("no hay ninguna decision con id '%s' en bench/decisiones.tsv" % ident,
              file=sys.stderr)
        return 2

    frenos = sorted({p for r in rs for p in para(r["estado"])})
    if frenos:
        print("NO SE PUEDE CERRAR '%s': %s" % (ident, ", ".join(frenos)), file=sys.stderr)
        for r in rs:
            if r["faltan"] != "-":
                print("   %s: falta medir %s" % (r["juego"], r["faltan"]), file=sys.stderr)
            if "DESANCLADA" in r["estado"]:
                print("   el fuente no dice lo que la decision afirma: %s no casa en %s"
                      % (r["ancla"], r["_ancla_fichero"]), file=sys.stderr)
            if "SIN MEDIDA" in r["estado"]:
                print("   la etiqueta «%s» no tiene ni una fila en el libro mayor"
                      % r["etiqueta"], file=sys.stderr)
        print("\nNo es burocracia: una decision que la medida no sostiene es una corazonada con\n"
              "una tabla al lado. Mide lo que falta, o baja el veredicto a PENDIENTE.",
              file=sys.stderr)
        return 1

    txt = cuerpo(rs)
    print(txt)
    if "--commit" in argv:
        p = subprocess.run(["git", "commit", "-F", "-"], cwd=RAIZ,
                           input=txt, text=True)
        return p.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

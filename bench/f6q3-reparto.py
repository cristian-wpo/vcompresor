#!/usr/bin/env python3
"""F6.Q3 — el reparto del coste de partir en frames, sacado del libro mayor.

La pregunta del encargo no es «cuanto ocupa cada condicion» sino COMO SE REPARTE el hueco entre
las dos causas: tablas frias (i) y coincidencias largas perdidas (ii). Eso son cuatro restas, y
escribirlas a mano en el documento es justo lo que hace que un numero no se pueda re-derivar
seis meses despues. Aqui se calculan desde bench/historia.jsonl, que esta versionado.

    bench/f6q3-reparto.py [solid]

Se compara en TOTALES DE LOS DOS FRAMES, que es lo unico que no mezcla geometrias de tabla:
el `tam` con el que se construye el predictor fija tmask/imask, y en (d) el bloque es del doble.

    (a) = F0 + F1_frio     (b) = F0 + F1_cebado     (c) = F0 + F1_cebado_ventana     (d) = F01
    hueco = (a) - (d)      recupera X = (a) - X
"""
import json, os, sys

SOLID = sys.argv[1] if len(sys.argv) > 1 else "32"
REG = os.path.join(os.path.dirname(__file__), "historia.jsonl")

# La ultima fila de cada condicion gana: una tanda se puede repetir, y lo que vale es lo ultimo
# medido. Se exige rc=0 — una fila que fallo tiene bytes_out con toda la pinta de un dato.
CLAVES = {
    f"F6.Q3 s{SOLID} F0 solo":            "F0",
    f"F6.Q3 s{SOLID} (a) frio":           "a",
    f"F6.Q3 s{SOLID} (b) tablas primadas":"b",
    f"F6.Q3 s{SOLID} (c) primadas+historia": "c",
    f"F6.Q3 s{SOLID} (d) techo un bloque":"d",
}
fila = {}
with open(REG, encoding="utf-8") as fh:
    for linea in fh:
        f = json.loads(linea)
        k = CLAVES.get(f.get("etiqueta", "").strip())
        if k and f.get("rc") == 0 and f.get("bytes_out"):
            fila[k] = f

falta = [k for k in ("F0", "a", "b", "c", "d") if k not in fila]
if falta:
    print(f"!! faltan condiciones en el libro mayor: {', '.join(falta)}")
    print("   (corre bench/f6q3-condiciones.sh; sin las cinco no hay reparto que publicar)")

# EL SELLO. Dos condiciones medidas con binarios distintos no se pueden restar la una de la otra,
# y la resta saldria igual de limpia. Se comprueba antes de dar un solo numero.
sellos = {f["bin_sha"] for f in fila.values()}
if len(sellos) > 1:
    print(f"!! filas de binarios DISTINTOS {sellos} — no son comparables")
    sys.exit(3)
if falta:
    sys.exit(2)

F0 = fila["F0"]["bytes_out"]
tot = {k: (F0 + fila[k]["bytes_out"]) for k in ("a", "b", "c")}
tot["d"] = fila["d"]["bytes_out"]
hueco = tot["a"] - tot["d"]

print(f"F6.Q3 · solid={SOLID} · binario {sellos.pop()}")
print(f"  frame 0 = {fila['F0']['bytes_in']:,} B   frame 1 = {fila['a']['bytes_in']:,} B")
print(f"  F0 comprimido = {F0:,} B\n")
print(f"  {'condicion':<26} {'F1':>12} {'total 2 frames':>16} {'vs (d)':>12} {'recupera':>12}")
NOMBRE = {"a": "(a) frio  [hoy]", "b": "(b) tablas primadas",
          "c": "(c) primadas+historia", "d": "(d) techo un bloque"}
for k in ("a", "b", "c", "d"):
    f1 = fila[k]["bytes_out"] if k != "d" else 0
    sobre = tot[k] - tot["d"]
    rec = tot["a"] - tot[k]
    print(f"  {NOMBRE[k]:<26} {(f'{f1:,}' if f1 else '—'):>12} {tot[k]:>16,}"
          f" {f'+{sobre:,}' if sobre else '0':>12}"
          f" {(f'{rec:,}' if rec else '—'):>12}"
          + (f"   {100*sobre/tot['d']:+.3f}%" if sobre else "   (referencia)"))

print(f"\n  hueco de partir = (a) - (d) = {hueco:,} B  ({100*hueco/tot['d']:+.3f}% sobre el techo)")
if hueco:
    for k in ("b", "c"):
        rec = tot["a"] - tot[k]
        print(f"  recupera {k} = {rec:,} B = {100*rec/hueco:.1f}% del hueco"
              f"   · queda {100*(tot[k]-tot['d'])/tot['d']:+.3f}% sobre el techo")
# Los totales (a)(b)(c) llevan DOS cabeceras de flujo de 10 B y (d) lleva UNA. Se dice en vez de
# esconderlo: son 10 B sobre ~4 MB y no mueven ninguna conclusion, pero forman parte del hueco.
print(f"\n  nota: 10 B del hueco son estructurales (dos cabeceras de flujo contra una)"
      f" = {100*10/hueco:.2f}% del hueco.")

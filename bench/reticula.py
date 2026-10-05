#!/usr/bin/env python3
"""P16 · ¿Se puede medir el «no cabe el nodo con cuentas» SIN construir nada? SI.

La retícula de estados de vc-cm es una tabla ESTATICA: `LIM`, `valido`, `acota` y `descuento`
la definen entera. Enumerarla no toca el compresor, no gasta un ciclo de banco y contesta la
pregunta que yo habia contestado a ojo.

Transcrito literalmente de crates/vc-cm/src/lib.rs:47-86.
"""
LIM = [41, 40, 18, 12, 9, 7, 6, 5]

def valido(a, b):
    mn, mx = (a, b) if a <= b else (b, a)
    return mn <= 7 and mx <= LIM[mn]

def acota(a, b):
    if a <= b:
        a = min(a, 7)
        return (a, min(b, LIM[a]))
    else:
        b = min(b, 7)
        return (min(a, LIM[b]), b)

def descuento(x):
    return 2 + (x - 2) // 2 if x > 2 else x

pares = [(a, b) for a in range(64) for b in range(64) if valido(a, b)]
idx = {p: i for i, p in enumerate(pares)}
print(f"=== LA RETICULA ===")
print(f"estados validos: {len(pares)} de 256 posibles en un byte "
      f"(de {64*64} pares (a,b) concebibles)")

print(f"\n=== EL TECHO DE EVIDENCIA, POR GRADO DE DESEQUILIBRIO ===")
print(f"{'minoria':>8s} {'mayoria max':>12s} {'a+b max':>9s}  {'que contexto es':s}")
for mn in range(8):
    cand = [(a, b) for (a, b) in pares if min(a, b) == mn]
    if not cand:
        print(f"{mn:8d} {'-':>12s} {'-':>9s}  IMPOSIBLE: LIM[{mn}]={LIM[mn]} < {mn}")
        continue
    mx = max(max(a, b) for (a, b) in cand)
    tot = max(a + b for (a, b) in cand)
    print(f"{mn:8d} {mx:12d} {tot:9d}  " +
          ("racha casi pura" if mn <= 1 else "equilibrado" if mn >= 4 else "sesgado"))

print(f"\n🔑 Un contexto que ha visto los DOS bits unas cuantas veces topa en "
      f"{max(a+b for (a,b) in pares if min(a,b) >= 4)} observaciones.")
print(f"   Solo una racha casi pura llega a {max(a+b for (a,b) in pares)}.")

# ── LO QUE DECIDE: ¿que pasa al SUMAR dos nodos? (la operacion de nucleacion) ──
print(f"\n=== SUMAR DOS NODOS: cuanta evidencia sobrevive a la re-cuantizacion ===")
print("Nucleacion fusiona sumando cuentas. Aqui la suma tiene que volver a la reticula.")
tot_ent = tot_sal = 0
peor = None
clamp = 0
for (a1, b1) in pares:
    for (a2, b2) in pares:
        s = (a1 + a2, b1 + b2)
        q = acota(*s)
        ent, sal = s[0] + s[1], q[0] + q[1]
        tot_ent += ent; tot_sal += sal
        if sal < ent: clamp += 1
        if ent == 0: continue          # (0,0)+(0,0): no hay evidencia que perder
        p = (ent - sal) / ent
        if peor is None or p > peor[0]:
            peor = (p, (a1, b1), (a2, b2), s, q)
n = len(pares) ** 2
print(f"  pares de nodos examinados: {n}")
print(f"  sumas que la reticula RECORTA: {clamp} ({100*clamp/n:.1f}%)")
print(f"  evidencia que entra: {tot_ent}   sobrevive: {tot_sal}   "
      f"SE PIERDE EL {100*(tot_ent-tot_sal)/tot_ent:.1f}%")
p, A, B, s, q = peor
print(f"  el peor caso: {A} + {B} = {s}  ->  {q}   (se tira el {100*p:.1f}%)")

# ── y el control: ¿y si NO se suma, sino que se elige el de mas evidencia? ──
mejor = 0
for (a1, b1) in pares:
    for (a2, b2) in pares:
        mejor += max(a1 + b1, a2 + b2)
print(f"\n  CONTROL — quedarse con el mas informado en vez de sumar: {mejor} de evidencia,")
print(f"  contra {tot_sal} sumando y re-cuantizando. Sumar {'GANA' if tot_sal > mejor else 'PIERDE'} "
      f"({100*(tot_sal-mejor)/mejor:+.1f}%).")
print("\n  ⇒ Si sumar pierde contra quedarse con el mejor, la operacion central de nucleacion")
print("    no puede funcionar sobre ESTA reticula. No es que 'no quepa en memoria':")
print("    es que la representacion tira justo lo que la fusion va a buscar.")

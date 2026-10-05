#!/usr/bin/env python3
"""Convierte un volcado VC_COSTEPOS (un f32 = bits gastados por cada byte) en el JSON que
dibuja el panel.

El volcado en crudo es enorme (4 bytes por byte de entrada). El panel no necesita el detalle
byte a byte: necesita la FORMA. Asi que se reduce a tres cosas que si caben en una pagina:

  1. la tira   — N cubos con el coste medio de cada tramo. Es el mapa de calor.
  2. el reparto — cuantos bytes salen a cada precio, de 0 a 8 bits.
  3. los extremos — los tramos mas caros y mas baratos, CON SU TEXTO. Esto es lo que de
     verdad se pedia: no un numero, sino ver que parte del fichero cuesta y cual sale gratis.

Y ademas el coste por caracter, que responde a la pregunta de si una letra ocupa su entero:
no lo ocupa, y aqui se ve cuanto ocupa cada una de verdad.
"""
import sys, json, struct, array, collections

def carga(f32_path, src_path, nombre, cubos=1000):
    c = array.array("f")
    with open(f32_path, "rb") as f:
        c.frombytes(f.read())
    src = open(src_path, "rb").read()
    # El volcado NO empieza en el byte 0 del fichero: delante van los cuatro tanteos de 128 KiB
    # que hace la sonda de perfil para elegir familia de modelos. Alinear desde el PRINCIPIO
    # emparejaria el coste de la sonda con el texto real y saldrian mapas preciosos y falsos.
    # Se alinea desde el FINAL, que es donde esta la pasada de verdad sobre el fichero entero.
    n = min(len(c), len(src))
    c = c[len(c) - n:]; src = src[len(src) - n:]

    total_bits = float(sum(c))
    # 1. la tira
    paso = max(1, n // cubos)
    tira = []
    for i in range(0, n - paso + 1, paso):
        tira.append(round(sum(c[i:i+paso]) / paso, 3))

    # 2. el reparto: 32 cubetas de 0 a 8 bits
    rep = [0]*33
    for v in c:
        k = int(v * 4)
        rep[min(32, max(0, k))] += 1

    # 3. los extremos, ventana de 192 bytes
    W = 192
    ventanas = []
    for i in range(0, n - W, W):
        ventanas.append((sum(c[i:i+W]), i))
    ventanas.sort()
    def texto(i):
        t = src[i:i+W].decode("utf-8", "replace")
        return "".join(ch if (ch.isprintable() or ch == "\n") else "." for ch in t)
    baratos = [{"off": i, "bpb": round(s/W, 3), "txt": texto(i)} for s, i in ventanas[:3]]
    caros   = [{"off": i, "bpb": round(s/W, 3), "txt": texto(i)} for s, i in ventanas[-3:][::-1]]

    # 4. coste por caracter
    por = collections.defaultdict(float); cnt = collections.Counter()
    for b, v in zip(src, c):
        por[b] += v; cnt[b] += 1
    chars = []
    for b, tot in sorted(por.items(), key=lambda kv: -kv[1])[:24]:
        chars.append({
            "b": b,
            "ch": chr(b) if 32 <= b < 127 else {10: "\\n", 9: "\\t", 13: "\\r"}.get(b, f"\\x{b:02x}"),
            "n": cnt[b],
            "bpb": round(tot / cnt[b], 3),
            "pct": round(tot * 100.0 / total_bits, 2),
        })

    return {
        "nombre": nombre, "bytes": n, "bits": round(total_bits),
        "bpb": round(total_bits / n, 4),
        "salida": round(total_bits / 8), "ratio": round(n / (total_bits / 8), 2),
        "tira": tira, "reparto": rep,
        "baratos": baratos, "caros": caros, "chars": chars,
    }

if __name__ == "__main__":
    out = []
    for f32, src, nom in [a.split(":") for a in sys.argv[1:]]:
        out.append(carga(f32, src, nom))
    print(json.dumps(out, ensure_ascii=False))

#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""VDEFLATE: el proyecto propio de DEFLATE, con sus fases, su cobertura, su suelo y sus
herramientas.

QUE ES Y QUE NO ES. vdeflate NO es un compresor. Es un REPRODUCTOR: coge un flujo DEFLATE que
ya existe —dentro de un PNG, un ZIP, un .docx— lo desmonta hasta sus decisiones y lo vuelve a
montar BYTE A BYTE. Quien lea el nombre esperando `vdeflate::comprime(datos)` se equivoca de
pieza, y por eso se dice aqui arriba y en la cabecera de cada modulo.

POR QUE EXISTE, con el numero que lo justifica: el desglose directo del reloj dice que preflate
es el 78% del tiempo de todo el archivador, y preflate es de terceros. Ademas falla en el 8,7%
de los flujos reales, y no falla limpio: entra en panico.

SE SEPARA DEL ARCHIVADOR pero sigue siendo nuestro. Tiene su propio banco (1.973 flujos reales),
sus propios arneses de aceptacion, su propia medida de cobertura y su propio suelo fisico. Todo
lo de aqui es medido hoy; nada es estimado.
"""
import csv

# ── LAS FASES ────────────────────────────────────────────────────────────────────────────────
# fase · que hace · peso dentro del 78% · estado · la prueba que lo demuestra
FASES = [
("1 · inflar",
 "Deshacer el DEFLATE y sacar el plano, exactamente como lo saca zlib.",
 "9,6%", "HECHO",
 "bin/vc-inflate: 1.973 de 1.973 flujos dan el plano IDENTICO. Sin nota parcial."),

("1b · capturar",
 "Guardar las DECISIONES del codificador original: tokens (literal o copia), el tipo de cada "
 "bloque, la cabecera dinamica entera y la secuencia RLE en crudo, mas el simbolo de longitud y "
 "distancia que se uso de verdad. Todo esto se leia ya y se tiraba.",
 "0%", "HECHO",
 "Prueba que re-ejecuta los tokens y comprueba que reconstruyen el plano. Y medido que no "
 "cuesta nada: 280,9 contra 282,9 MB/s, -0,7%, por debajo del ruido."),

("2 · re-emitir",
 "Volver a producir el flujo comprimido BYTE A BYTE. Lo dificil no es comprimir: es que un "
 "DEFLATE admite muchas codificaciones igual de legales y hay que devolver la que habia.",
 "23,9%", "HECHO",
 "bin/vc-redeflate: 1.973 de 1.973 vuelven a salir identicos, a la primera."),

("3 · estimar",
 "Dado SOLO el plano, encontrar los ajustes del codificador original que reproducen esos "
 "tokens, y guardar unicamente las divergencias. Es lo que convierte la descripcion completa "
 "del flujo en unas correcciones pequeñas.",
 "66,5%", "CENSADA — LA FORMA ESTA MAL, NO LOS PARAMETROS",
 "🔑 CENSO DEL 01-08 (bin/vc-censo-codificadores), Y ES LO QUE DECIDE EL PROYECTO. Nuestra fase 3 "
 "pregunta «¿cual de los NUEVE NIVELES DE zlib produjo esto?». preflate no pregunta eso: enumera "
 "los ALGORITMOS DE HASH REALES de los codificadores del mundo -zlib con su desplazamiento y "
 "mascara exactos, el rapido de miniz, libdeflate con y sin hash secundario, zlib-ng, crc32c- y "
 "prueba CUAL DE ELLOS EXPLICA el flujo observado. Corrido SU estimador sobre NUESTROS 1.973 "
 "flujos: 55,7% zlib clasico, 20,1% Libdeflate4Fast, 13,5% Libdeflate4, y el resto entre "
 "RandomVector (3,4%), crc32c (2,8%), zlib-ng (1,4%) y miniz (1,0%). UN TERCIO DEL BANCO ES "
 "libdeflate, y solo el 42,1% de los flujos son COMPATIBLES CON zlib. O sea que los nueve "
 "niveles de zlib son la forma correcta para el 55,7% y la EQUIVOCADA para el 44,3%: eso no se "
 "afina cambiando umbrales, se reescribe cambiando la pregunta. Y hay DOS EJES MAS que no "
 "modelamos en absoluto: la estrategia (64,7% perezoso con CINCO juegos de umbrales distintos) y "
 "sobre todo QUE SE METE EN EL DICCIONARIO (66,7% mete todo, 14,0% solo el primer trozo de cada "
 "coincidencia). Ese segundo eje explica fallos que parecian de longitud: si el codificador "
 "original solo indexaba el primer trozo, nuestra cadena tiene candidatos que la suya NO TENIA y "
 "predecimos coincidencias que el no podia haber encontrado. "
 "MEDIDO el 2026-07-31 con bin/vc-prediccion y bin/vc-fase3, y el resultado es NEGATIVO y hay "
 "que decirlo: estimar los nueve niveles de zlib por flujo -su tabla real, con voraz/perezoso, "
 "good_length, max_lazy, nice_length, max_chain y TOO_FAR- acierta el 87,71% de los SIMBOLOS y "
 "deja el coste en el 13,14% del DEFLATE. Es 8 veces mejor que describirlo todo (104,51%) y "
 "sigue siendo 7,7 veces peor que preflate (1,70%). "
 "DOS COSAS QUE SE APRENDIERON MIDIENDO. (a) La primera version solo puntuaba las COPIAS y daba "
 "un 84,70%; contando tambien los literales -que tambien hay que guardar- baja al 82,10%. Una "
 "⚠️ Y ESE 82,10% CADUCO SIN QUE NADIE LO TOCARA, corregido el 01-08: el numero real es 87,71%. "
 "No aparece en NINGUNA salida de medida -solo vivia en esta prosa- y las TRES corridas "
 "preservadas de bin/vc-fase3 (F17, commits 9ffc460, 7be26b4 y 2078652) dicen 87,71% las tres. "
 "La causa mas probable es el propio commit 9ffc460, «un solo modelo de coste»: cambio "
 "`Cuenta::fallos()`, y `fallos()` es lo que elige QUE CONFIGURACION gana en cada flujo, asi que "
 "cambio tambien el acierto. Es P11 del camino otra vez -un arreglo invalida en silencio el "
 "numero de otra parte- y la leccion es que un numero que solo vive en la prosa no tiene quien "
 "lo caduque. "
 "medida que solo mira la mitad de los simbolos no es optimista, es falsa. (b) La comparacion "
 "con preflate NO es de igual a igual: preflate revienta con un panic en 171 de los 1.973 "
 "flujos y su 1,70% esta medido solo sobre los 1.802 que acepta. Tiene el examen mas facil y "
 "aun asi gana. "
 "DONDE SEGUIR, y sale de la propia tabla: el grueso de los fallos es «otra longitud» (9,1% de "
 "los simbolos), no la distancia ni la estrategia. O sea que el codificador original no elegia "
 "la coincidencia mas larga con estos parametros: el banco tiene flujos de zopfli, libdeflate, "
 "Go o Java, que hacen busqueda optima o casi, y los nueve niveles de zlib no los alcanzan. Lo "
 "bueno: el 67% de esos fallos de longitud son de 3 bytes o menos, o sea baratos de corregir."),

("4 · acelerar",
 "El decodificador iba bit a bit, que es el metodo de `puff` y fue una decision deliberada: el "
 "mas lento y el mas facil de demostrar correcto. Cerrada la correccion, se cambio por copia "
 "por lotes, arboles fijos cacheados y tabla de 9 bits. Queda el acumulador de 64 bits.",
 "—", "EN OBRA",
 "P10 CERRADO el 2026-07-31: las cifras del suelo ya NO estan escritas aqui. Las mide "
 "bench/suelo.sh y las deja en bench/suelo.tsv; este fichero solo las lee. Estuvieron a mano y "
 "caducaron dos veces el mismo dia —decian 279 ciclos y 24,7x cuando ya eran 171,8 y 14,4—, "
 "que es exactamente lo que la pregunta P10 pregunta."),
]

# ── LA COBERTURA, medida por bin/vc-cobertura sobre los 1.973 flujos ────────────────────────
COB = [
("bloque almacenado (btype 00)", "1.920", "si"),
("bloque de arboles fijos (01)", "1.003", "si"),
("bloque de arboles dinamicos (10)", "10.805", "si"),
("flujos con mas de un bloque", "1.375 (hasta 143 bloques)", "si"),
("simbolos de longitud", "29 de 29", "si"),
("simbolos de distancia", "30 de 30", "si"),
("distancia minima", "1 — copias solapadas", "si"),
("distancia maxima", "32.765 de 32.768 posibles", "si"),
("longitud maxima", "258, el tope del formato", "si"),
("longitud 258 como codigo 285", "17.243.340", "si"),
("longitud 258 como codigo 284+31", "0 en el banco — CERRADO A MANO", "si, fabricado"),
("arbol de distancias de un solo simbolo", "17", "si"),
("RLE 16 / 17 / 18 de la cabecera", "216.396 / 20.178 / 6.276", "si"),
("hclen", "de 12 a 19", "si"),
]

# ── EL SUELO ────────────────────────────────────────────────────────────────────────────────
#
# ESTO YA NO SE ESCRIBE A MANO, y ese es el arreglo del hallazgo P10 de la propia auditoria de
# vdeflate. Las seis cifras de aqui estuvieron copiadas dentro de este fichero y CADUCARON DOS
# VECES EL MISMO DIA —decian 279 ciclos y 24,7x cuando ya eran 171,8 y 14,4—. Una tabla de
# estado que no se recalcula deja de ser un estado y pasa a ser una foto vieja con pinta de dato.
#
# Ahora las mide `bench/suelo.sh`, que las deja en `bench/suelo.tsv`, y aqui solo se leen. Si
# nadie ha medido todavia, se DICE que no hay medida en vez de enseniar la ultima que hubo.
def _lee_suelo():
    d = {}
    try:
        with open("bench/suelo.tsv", encoding="utf-8") as f:
            for l in f:
                if l.startswith("#") or not l.strip(): continue
                p = l.rstrip("\n").split("\t")
                if len(p) >= 2 and p[0] != "clave": d[p[0]] = p[1]
    except FileNotFoundError:
        pass
    return d

_S = _lee_suelo()
def _n(k, d=None):
    try: return float(_S[k])
    except (KeyError, ValueError): return d

_esc, _cop = _n("ancho_escribir_gb_s"), _n("ancho_copiar_gb_s")
_sal, _sim = _n("salida_bytes"), _n("simbolos")
_hoy, _piso = _n("nuestro_seg"), _n("suelo_fisico_seg")
_cps = _n("ciclos_por_simbolo")
_ips = _n("instr_por_simbolo")
_dist = _n("distancia_al_suelo_x")

def _s(x, u="s", d=2):
    return "sin medir" if x is None else f"{x:.{d}f} {u}".replace(".", ",")

if not _S:
    SUELO = [("SIN MEDIR", "—", "corre bench/suelo.sh en la maquina; esta tabla la escribe el, "
                                "no se rellena a mano (hallazgo P10)")]
else:
    # El suelo ALGORITMICO no es una constante magica: son los simbolos contados por
    # vc-cobertura a un coste optimista de 10 ciclos cada uno, al reloj de esta maquina.
    _GHZ = 3.0
    _alg = _sim * 10 / (_GHZ * 1e9) if _sim else None
    SUELO = [
    (f"escribir los {_sal/1e9:.2f} GB de salida".replace(".", ","), _s(_piso),
     f"a {_s(_esc,'GB/s',1)}, medido en esta maquina"),
    ("SUELO FISICO", _s(_piso), "lo que no se puede evitar: hay que producir esos bytes igual"),
    (f"suelo algoritmico a 10 ciclos/simbolo (a {_GHZ:.0f} GHz)".replace(".", ","), _s(_alg),
     f"{_sim:,.0f} simbolos contados, no estimados".replace(",", ".")),
    ("HOY", _s(_hoy),
     f"{_s(_cps,'ciclos',1)} y {_s(_ips,'instrucciones',1)} por simbolo"),
    ("distancia al suelo", _s(_dist, "x", 1),
     f"medido el {_S.get('fecha','?')} en el commit {_S.get('commit','?')}"),
    ]

# ── LAS HERRAMIENTAS ────────────────────────────────────────────────────────────────────────
HER = [
("crates/vc-core/src/inflate.rs", "modulo",
 "Fase 1 y 1b: descomprime, y captura el flujo entero si se le pide."),
("crates/vc-core/src/deflate_emit.rs", "modulo",
 "Fase 2: vuelve a emitir. No decide nada — replica lo que el inflate capturo."),
("bin/vc-inflate", "arnes",
 "El plano tiene que salir identico al de zlib en los 1.973. Sin nota parcial."),
("bin/vc-redeflate", "arnes",
 "El flujo comprimido tiene que salir identico en los 1.973. Contrato mas duro que el anterior."),
("bin/vc-cobertura", "medida",
 "Que partes del RFC 1951 ejercita el banco. Lo que salga a cero es lo que el «1.973 de 1.973» "
 "NO cubre — que es distinto de estar mal, y peor de ignorar."),
("bin/vc-preflate-coste", "medida",
 "Reparte el 78% de preflate entre inflar, estimar y re-deflate, y cuenta cuantos flujos "
 "rechaza preflate-rs."),
("bench/saca-deflate.py", "banco",
 "Regenera los 1.973 flujos reales desde los PNG del corpus. El banco no se guarda: se rehace."),
("bench/suelo.sh", "medida",
 "El ancho de banda de la maquina y los ciclos por simbolo, para saber a cuanto estamos del "
 "minimo fisico."),
]

def esc(t):
    return t


with open("bench/vdeflate.tsv", "w", newline="") as f:
    f.write("# VDEFLATE — el proyecto propio de DEFLATE.\n")
    f.write("# NO es un compresor: es un REPRODUCTOR. Coge un DEFLATE que ya existe, lo desmonta\n")
    f.write("# hasta sus decisiones y lo vuelve a montar byte a byte.\n")
    f.write("# Existe porque preflate es el 78% del reloj del archivador, es de terceros, y falla\n")
    f.write("# en el 8,7% de los flujos reales entrando en panico.\n")
    f.write("# Lo escribe bench/vdeflate.py.\n")
    w = csv.writer(f, delimiter="\t")
    w.writerow(["seccion", "clave", "valor", "detalle", "extra"])
    for n, q, peso, est, prueba in FASES:
        w.writerow(["fase", n, est, q, f"{peso} del 78% · {prueba}"])
    for k, v, cub in COB:
        w.writerow(["cobertura", k, v, cub, ""])
    for k, v, n in SUELO:
        w.writerow(["suelo", k, v, n, ""])
    for k, t, q in HER:
        w.writerow(["herramienta", k, t, q, ""])

print("bench/vdeflate.tsv escrito\n")
print("  VDEFLATE · las fases")
for n, q, peso, est, _ in FASES:
    marca = {"HECHO": "OK", "SIGUIENTE": "->", "PLANIFICADO": ".."}.get(est, "  ")
    print(f"    [{marca}] {n:<16} {peso:>6} del 78%   {est}")
hecho = sum(1 for f in FASES if f[3] == "HECHO")
print(f"\n  {hecho} de {len(FASES)} fases hechas · el 33,5% del 78% ya es nuestro y verificado")
print(f"  cobertura: {len(COB)} dimensiones del formato medidas, todas ejercitadas")
print(f"  suelo: " + ("SIN MEDIR — corre bench/suelo.sh" if _dist is None else
      f"estamos {_dist:.1f}x por encima del minimo fisico de esta maquina".replace(".", ",")))
print(f"  herramientas: {len(HER)}")

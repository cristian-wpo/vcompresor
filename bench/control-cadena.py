# -*- coding: utf-8 -*-
# EL CONTROL DEL TOPE DE CADENA: solo pueden moverse los flujos cuyo estimador dice que el
# codificador buscaba MENOS candidatos que nuestra configuracion mas corta (max_chain < 4).
# Todo lo demas tiene que salir IDENTICO byte a byte: si se mueve, el tope esta tocando de mas.
import collections, sys
def carga(p, n):
    d = {}
    for l in open(p, encoding='utf-8', errors='replace'):
        if l.startswith('#') or not l.strip(): continue
        q = l.rstrip().split('\t')
        if len(q) < n or q[0] == 'fichero': continue
        d[q[0]] = q
    return d
ANTES = sys.argv[1] if len(sys.argv) > 1 else '/tmp/f32-antes-cadena.tsv'
antes = carga(ANTES, 5)
ahora = carga('/tmp/f32.tsv', 5)
censo = carga('/tmp/censo2.tsv', 8)
mov = collections.Counter(); tot = collections.Counter()
ac_a = collections.Counter(); ac_b = collections.Counter(); si = collections.Counter()
CORTA = 'CADENA CORTA (max_chain < 4)'
CTRL = 'control (max_chain >= 4)'
detalle = []
for n in set(antes) & set(ahora):
    try: cad = int(censo.get(n, ['', '', '', '', '999'])[4])
    except ValueError: cad = 999
    k = CORTA if cad < 4 else CTRL
    tot[k] += 1
    a, b = int(antes[n][2]), int(ahora[n][2])
    s = int(ahora[n][1])
    si[k] += s; ac_a[k] += a; ac_b[k] += b
    if a != b:
        mov[k] += 1
        if k == CORTA: detalle.append((n, cad, s, a, b))
print('CONTROL DEL TOPE DE CADENA · %d flujos cruzados' % sum(tot.values()))
print('  %-30s %7s %9s %11s %11s %9s' % ('', 'flujos', 'se mueven', 'antes', 'ahora', 'delta'))
for k in sorted(tot):
    pa = 100.0 * ac_a[k] / max(si[k], 1); pb = 100.0 * ac_b[k] / max(si[k], 1)
    print('  %-30s %7d %9d %10.2f%% %10.2f%% %+8.2f p' % (k, tot[k], mov[k], pa, pb, pb - pa))
if detalle:
    print('\n  los que se mueven, uno a uno:')
    print('  %-40s %5s %9s %9s %9s' % ('flujo', 'cad', 'simbolos', 'antes', 'ahora'))
    for n, cad, s, a, b in sorted(detalle, key=lambda x: x[4] - x[3], reverse=True):
        print('  %-40s %5d %9d %9d %9d' % (n[:40], cad, s, a, b))
print()
if mov[CTRL] == 0:
    print('  ✅ CONTROL LIMPIO: ni uno de los %d flujos con cadena >= 4 se ha movido.' % tot[CTRL])
else:
    print('  ⚠️ %d flujos del CONTROL se han movido y NO deberian. El tope toca de mas.' % mov[CTRL])

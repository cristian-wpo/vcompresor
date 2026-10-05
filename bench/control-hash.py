# -*- coding: utf-8 -*-
# EL CONTROL: los flujos que NO cambian de hash tienen que salir IDENTICOS.
import collections, sys
def carga(p, n):
    d = {}
    for l in open(p, encoding='utf-8', errors='replace'):
        if l.startswith('#') or not l.strip(): continue
        q = l.rstrip().split('\t')
        if len(q) < n or q[0] == 'fichero': continue
        d[q[0]] = q
    return d
antes = carga('/tmp/f32-antes.tsv', 5)
ahora = carga('/tmp/f32.tsv', 5)
censo = carga('/tmp/censo2.tsv', 8)
NUEVOS = ('Crc32c', 'MiniZ', 'ZlibNG')
mov = collections.Counter(); tot = collections.Counter()
ac_a = collections.Counter(); ac_b = collections.Counter(); si = collections.Counter()
for n in set(antes) & set(ahora):
    h = censo.get(n, ['', '?'])[1]
    k = 'CAMBIA DE HASH' if any(h.startswith(x) for x in NUEVOS) else 'control (zlib/libdeflate/otros)'
    tot[k] += 1
    a, b = int(antes[n][2]), int(ahora[n][2])
    s = int(ahora[n][1])
    si[k] += s; ac_a[k] += a; ac_b[k] += b
    if a != b: mov[k] += 1
print('CONTROL POR FLUJO · %d flujos cruzados' % sum(tot.values()))
print('  %-32s %7s %9s %11s %11s %9s' % ('', 'flujos', 'se mueven', 'antes', 'ahora', 'delta'))
for k in sorted(tot):
    pa = 100.0*ac_a[k]/max(si[k],1); pb = 100.0*ac_b[k]/max(si[k],1)
    print('  %-32s %7d %9d %10.2f%% %10.2f%% %+8.2f p' % (k, tot[k], mov[k], pa, pb, pb-pa))
print()
if mov['control (zlib/libdeflate/otros)'] == 0:
    print('  ✅ CONTROL LIMPIO: ni uno solo de los flujos que no cambian de hash se ha movido.')
else:
    print('  ⚠️ %d flujos del CONTROL se han movido y no deberian. Algo mas cambio.' % mov['control (zlib/libdeflate/otros)'])

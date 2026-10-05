// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Carril JPEG PROPIO — parsear un JPEG hasta sus coeficientes DCT y volver a emitirlo
//! **byte a byte idéntico**. Todavía no comprime nada: es la puerta de correctitud.
//!
//! Cubre secuencial de Huffman (SOF0/SOF1) **y progresivo (SOF2)**, entrelazado o no, con o
//! sin marcadores de reinicio. Rechaza explícitamente aritmético, sin pérdida y jerárquico:
//! esos caen a store. Preferimos rechazar mucho y no fallar nunca.
//!
//! El progresivo se re-emite siguiendo el algoritmo del codificador de libjpeg (jcphuff),
//! que es el que ha generado prácticamente todos los progresivos que existen. Si un fichero
//! lo hizo otro codificador con otras decisiones, la comparación final lo detecta y el
//! fichero cae a store: nunca se acepta algo que no se haya reproducido byte a byte.
//!
//! Cero dependencias.

use anyhow::{bail, Result};

/// Zigzag: orden en que van los 64 coeficientes dentro del flujo.
#[rustfmt::skip]
const ZIGZAG: [usize; 64] = [
     0, 1, 8,16, 9, 2, 3,10, 17,24,32,25,18,11, 4, 5,
    12,19,26,33,40,48,41,34, 27,20,13, 6, 7,14,21,28,
    35,42,49,56,57,50,43,36, 29,22,15,23,30,37,44,51,
    58,59,52,45,38,31,39,46, 53,60,61,54,47,55,62,63,
];

// ---------- tablas Huffman ----------

#[derive(Clone)]
struct Huff {
    mincode: [i32; 17],
    maxcode: [i32; 17],
    valptr: [i32; 17],
    vals: Vec<u8>,
    ecode: [u16; 256],
    elen: [u8; 256],
    presente: bool,
}

impl Default for Huff {
    fn default() -> Self {
        Huff {
            mincode: [0; 17], maxcode: [0; 17], valptr: [0; 17], vals: Vec::new(),
            ecode: [0; 256], elen: [0; 256], presente: false,
        }
    }
}

impl Huff {
    fn nueva(bits: &[u8; 17], vals: Vec<u8>) -> Result<Huff> {
        let mut h = Huff { vals, presente: true, ..Default::default() };
        let (mut code, mut k) = (0i32, 0i32);
        for l in 1..=16usize {
            h.valptr[l] = k;
            h.mincode[l] = code;
            code += bits[l] as i32;
            h.maxcode[l] = if bits[l] == 0 { -1 } else { code - 1 };
            k += bits[l] as i32;
            code <<= 1;
        }
        if k as usize != h.vals.len() { bail!("DHT incoherente"); }
        let (mut code, mut i) = (0u32, 0usize);
        for l in 1..=16usize {
            for _ in 0..bits[l] {
                let s = h.vals[i] as usize;
                if h.elen[s] != 0 { bail!("DHT con símbolo repetido"); }
                if code > u16::MAX as u32 { bail!("DHT con código fuera de rango"); }
                h.ecode[s] = code as u16;
                h.elen[s] = l as u8;
                code += 1;
                i += 1;
            }
            code <<= 1;
        }
        Ok(h)
    }
}

// ---------- bits ----------

struct BitR<'a> { d: &'a [u8], p: usize, acc: u32, n: u32 }

impl<'a> BitR<'a> {
    fn new(d: &'a [u8]) -> Self { BitR { d, p: 0, acc: 0, n: 0 } }
    fn bit(&mut self) -> Result<u32> {
        if self.n == 0 {
            if self.p >= self.d.len() { bail!("scan truncado"); }
            let b = self.d[self.p];
            self.p += 1;
            if b == 0xFF {
                match self.d.get(self.p) {
                    Some(0) => self.p += 1,
                    _ => bail!("marcador inesperado dentro del scan"),
                }
            }
            self.acc = b as u32;
            self.n = 8;
        }
        self.n -= 1;
        Ok((self.acc >> self.n) & 1)
    }
    fn bits(&mut self, n: u32) -> Result<i32> {
        let mut v = 0i32;
        for _ in 0..n { v = (v << 1) | self.bit()? as i32; }
        Ok(v)
    }
    fn decodifica(&mut self, h: &Huff) -> Result<u8> {
        if !h.presente { bail!("el scan usa una tabla Huffman no declarada"); }
        let mut code = self.bit()? as i32;
        for l in 1..=16usize {
            if h.maxcode[l] >= 0 && code <= h.maxcode[l] {
                let i = (h.valptr[l] + (code - h.mincode[l])) as usize;
                if i >= h.vals.len() { bail!("índice Huffman fuera de tabla"); }
                return Ok(h.vals[i]);
            }
            if l < 16 { code = (code << 1) | self.bit()? as i32; }
        }
        bail!("código Huffman inválido")
    }
    fn relleno(&self) -> (u32, u32) {
        if self.n == 0 { (0, 0) } else { (self.n, self.acc & ((1 << self.n) - 1)) }
    }
}

#[derive(Default)]
struct BitW { out: Vec<u8>, acc: u32, n: u32 }

impl BitW {
    fn bit(&mut self, b: u32) {
        self.acc = (self.acc << 1) | (b & 1);
        self.n += 1;
        if self.n == 8 {
            let v = self.acc as u8;
            self.out.push(v);
            if v == 0xFF { self.out.push(0); }
            self.acc = 0;
            self.n = 0;
        }
    }
    fn bits(&mut self, code: u32, len: u32) {
        for i in (0..len).rev() { self.bit((code >> i) & 1); }
    }
    fn simbolo(&mut self, h: &Huff, s: usize) -> Result<()> {
        if h.elen[s] == 0 { bail!("la tabla Huffman no puede codificar el símbolo {s:02X}"); }
        self.bits(h.ecode[s] as u32, h.elen[s] as u32);
        Ok(())
    }
    /// Cierra el byte en curso con los bits de relleno EXACTOS que traía el original.
    fn cierra(&mut self, nrelleno: u32, relleno: u32) -> Result<()> {
        if self.n == 0 {
            if nrelleno != 0 { bail!("relleno esperado pero el byte ya estaba cerrado"); }
            return Ok(());
        }
        let faltan = 8 - self.n;
        if faltan != nrelleno { bail!("el relleno no cuadra: faltan {faltan}, guardados {nrelleno}"); }
        for i in (0..faltan).rev() { self.bit((relleno >> i) & 1); }
        Ok(())
    }
}

fn extiende(v: i32, t: u32) -> i32 {
    if t == 0 { 0 } else if v < (1 << (t - 1)) { v - (1 << t) + 1 } else { v }
}

/// Desplazamiento aritmético a la derecha, como el IRIGHT_SHIFT de libjpeg.
fn shr(v: i32, n: u32) -> i32 { v >> n }

fn nbits_de(mut v: i32) -> u32 {
    let mut n = 0;
    while v != 0 { n += 1; v >>= 1; }
    n
}

// ---------- estructura ----------

#[derive(Clone)]
struct CompInfo {
    id: u8,
    h: usize,
    v: usize,
    tq: u8,
    /// bloques reales del componente
    bw: usize,
    bh: usize,
    /// bloques asignados (redondeados a MCU completa)
    bwa: usize,
    bha: usize,
}

struct Scan {
    comps: Vec<usize>,
    td: Vec<usize>,
    ta: Vec<usize>,
    ss: usize,
    se: usize,
    ah: u32,
    al: u32,
    ri: usize,
    rellenos: Vec<(u32, u32)>,
    dc: Vec<Huff>,
    ac: Vec<Huff>,
}

enum Pieza { Lit(Vec<u8>), Scan(usize) }

pub struct Jpeg {
    piezas: Vec<Pieza>,
    scans: Vec<Scan>,
    comps: Vec<CompInfo>,
    /// coeficientes por componente, indexados [ci][by * bwa + bx]
    coef: Vec<Vec<[i16; 64]>>,
    progresivo: bool,
    mcux: usize,
    mcuy: usize,
}

impl Jpeg {
    pub fn bloques(&self) -> usize { self.coef.iter().map(|c| c.len()).sum() }
    pub fn es_progresivo(&self) -> bool { self.progresivo }
}

fn long_seg(d: &[u8], p: usize) -> Result<usize> {
    if p + 2 > d.len() { bail!("segmento truncado"); }
    let l = u16::from_be_bytes([d[p], d[p + 1]]) as usize;
    if l < 2 || p + l > d.len() { bail!("longitud de segmento inválida"); }
    Ok(l)
}

/// Dónde acaba el scan: el primer marcador que no sea relleno de bytes ni RSTn.
fn fin_de_scan(d: &[u8], desde: usize) -> usize {
    let mut i = desde;
    while i + 1 < d.len() {
        if d[i] == 0xFF {
            let m = d[i + 1];
            if m != 0x00 && m != 0xFF && !(0xD0..=0xD7).contains(&m) { return i; }
        }
        i += 1;
    }
    d.len()
}

/// Parte el scan en intervalos por los marcadores de reinicio.
fn intervalos(scan: &[u8], ri: usize) -> Vec<&[u8]> {
    if ri == 0 { return vec![scan]; }
    let mut v = Vec::new();
    let (mut ini, mut i) = (0usize, 0usize);
    while i + 1 < scan.len() {
        if scan[i] == 0xFF && (0xD0..=0xD7).contains(&scan[i + 1]) {
            v.push(&scan[ini..i]);
            i += 2;
            ini = i;
        } else {
            i += 1;
        }
    }
    v.push(&scan[ini..]);
    v
}

pub fn parsear(d: &[u8]) -> Result<Jpeg> {
    if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 { bail!("no empieza por SOI"); }

    let mut dc: Vec<Huff> = vec![Huff::default(); 4];
    let mut ac: Vec<Huff> = vec![Huff::default(); 4];
    let mut qt = [false; 4];
    let mut comps: Vec<CompInfo> = Vec::new();
    let (mut ancho, mut alto) = (0usize, 0usize);
    let mut ri = 0usize;
    let mut progresivo = false;
    let (mut mcux, mut mcuy) = (0usize, 0usize);
    let mut coef: Vec<Vec<[i16; 64]>> = Vec::new();

    let mut piezas: Vec<Pieza> = Vec::new();
    let mut scans: Vec<Scan> = Vec::new();
    let mut lit_desde = 0usize;
    let mut p = 2usize;

    loop {
        if p + 1 >= d.len() {
            // se acabó sin EOI explícito: el resto es literal
            piezas.push(Pieza::Lit(d[lit_desde..].to_vec()));
            break;
        }
        if d[p] != 0xFF { bail!("se esperaba un marcador en {p}"); }
        let mut q = p;
        while q < d.len() && d[q] == 0xFF { q += 1; }
        if q >= d.len() { bail!("marcador truncado"); }
        let m = d[q];
        p = q + 1;

        match m {
            0x01 | 0xD0..=0xD7 => {}
            0xD9 => {
                // EOI: el resto (incluido lo que venga detrás) es literal
                piezas.push(Pieza::Lit(d[lit_desde..].to_vec()));
                break;
            }
            0xC0 | 0xC1 | 0xC2 => {
                progresivo = m == 0xC2;
                let l = long_seg(d, p)?;
                let s = &d[p + 2..p + l];
                if s.len() < 6 { bail!("SOF truncado"); }
                alto = u16::from_be_bytes([s[1], s[2]]) as usize;
                ancho = u16::from_be_bytes([s[3], s[4]]) as usize;
                let nc = s[5] as usize;
                if nc == 0 || nc > 4 { bail!("SOF con {nc} componentes"); }
                if s.len() < 6 + nc * 3 { bail!("SOF truncado en componentes"); }
                if ancho == 0 || alto == 0 { bail!("dimensiones nulas"); }
                comps.clear();
                for i in 0..nc {
                    let b = &s[6 + i * 3..];
                    let (h, v) = ((b[1] >> 4) as usize, (b[1] & 15) as usize);
                    if h == 0 || v == 0 || h > 4 || v > 4 { bail!("submuestreo fuera de rango"); }
                    comps.push(CompInfo { id: b[0], h, v, tq: b[2], bw: 0, bh: 0, bwa: 0, bha: 0 });
                }
                let hmax = comps.iter().map(|c| c.h).max().unwrap();
                let vmax = comps.iter().map(|c| c.v).max().unwrap();
                mcux = ancho.div_ceil(8 * hmax);
                mcuy = alto.div_ceil(8 * vmax);
                coef.clear();
                for c in comps.iter_mut() {
                    c.bw = (ancho * c.h).div_ceil(8 * hmax);
                    c.bh = (alto * c.v).div_ceil(8 * vmax);
                    c.bwa = mcux * c.h;
                    c.bha = mcuy * c.v;
                    if c.bwa * c.bha > 64 * 1024 * 1024 { bail!("imagen desmesurada"); }
                    coef.push(vec![[0i16; 64]; c.bwa * c.bha]);
                }
                p += l;
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                bail!("modo no soportado (0x{m:02X}): aritmético, sin pérdida o jerárquico")
            }
            0xC4 => {
                let l = long_seg(d, p)?;
                let mut s = &d[p + 2..p + l];
                while !s.is_empty() {
                    if s.len() < 17 { bail!("DHT truncado"); }
                    let tc = s[0] >> 4;
                    let th = (s[0] & 15) as usize;
                    if th >= 4 || tc > 1 { bail!("DHT fuera de rango"); }
                    let mut bits = [0u8; 17];
                    let mut total = 0usize;
                    for i in 1..=16 { bits[i] = s[i]; total += s[i] as usize; }
                    if s.len() < 17 + total { bail!("DHT truncado en valores"); }
                    let h = Huff::nueva(&bits, s[17..17 + total].to_vec())?;
                    if tc == 0 { dc[th] = h; } else { ac[th] = h; }
                    s = &s[17 + total..];
                }
                p += l;
            }
            0xDB => {
                let l = long_seg(d, p)?;
                let mut s = &d[p + 2..p + l];
                while !s.is_empty() {
                    let pq = s[0] >> 4;
                    let tqi = (s[0] & 15) as usize;
                    if tqi >= 4 { bail!("DQT fuera de rango"); }
                    let n = if pq == 0 { 65 } else { 129 };
                    if s.len() < n { bail!("DQT truncado"); }
                    qt[tqi] = true;
                    s = &s[n..];
                }
                p += l;
            }
            0xDD => {
                let l = long_seg(d, p)?;
                if l < 4 { bail!("DRI truncado"); }
                ri = u16::from_be_bytes([d[p + 2], d[p + 3]]) as usize;
                p += l;
            }
            0xDA => {
                if comps.is_empty() { bail!("SOS sin SOF"); }
                for c in &comps {
                    if !qt[c.tq as usize] { bail!("componente con tabla de cuantización no declarada"); }
                }
                let l = long_seg(d, p)?;
                let s = &d[p + 2..p + l];
                if s.is_empty() { bail!("SOS truncado"); }
                let ns = s[0] as usize;
                if ns == 0 || ns > 4 || s.len() < 1 + ns * 2 + 3 { bail!("SOS truncado"); }
                let mut sc = Scan {
                    comps: Vec::new(), td: Vec::new(), ta: Vec::new(),
                    ss: 0, se: 0, ah: 0, al: 0, ri, rellenos: Vec::new(),
                    dc: dc.clone(), ac: ac.clone(),
                };
                for i in 0..ns {
                    let cid = s[1 + i * 2];
                    let t = s[2 + i * 2];
                    let Some(ci) = comps.iter().position(|c| c.id == cid) else {
                        bail!("SOS nombra un componente que no está en SOF")
                    };
                    sc.comps.push(ci);
                    sc.td.push((t >> 4) as usize);
                    sc.ta.push((t & 15) as usize);
                }
                if sc.td.iter().any(|&x| x >= 4) || sc.ta.iter().any(|&x| x >= 4) {
                    bail!("SOS con identificador de tabla fuera de rango");
                }
                sc.ss = s[1 + ns * 2] as usize;
                sc.se = s[2 + ns * 2] as usize;
                sc.ah = (s[3 + ns * 2] >> 4) as u32;
                sc.al = (s[3 + ns * 2] & 15) as u32;
                if sc.se > 63 || sc.ss > sc.se { bail!("banda espectral inválida"); }
                if sc.al > 13 || sc.ah > 13 { bail!("aproximación sucesiva fuera de rango"); }
                if !progresivo && (sc.ss != 0 || sc.se != 63 || sc.ah != 0 || sc.al != 0) {
                    bail!("secuencial con parámetros de progresivo");
                }

                let fin_cab = p + l;
                piezas.push(Pieza::Lit(d[lit_desde..fin_cab].to_vec()));
                let fin = fin_de_scan(d, fin_cab);
                decodifica_scan(&d[fin_cab..fin], &mut sc, &comps, &mut coef, progresivo, mcux, mcuy)?;
                scans.push(sc);
                piezas.push(Pieza::Scan(scans.len() - 1));
                lit_desde = fin;
                p = fin;
            }
            _ => {
                let l = long_seg(d, p)?;
                p += l;
            }
        }
    }

    if scans.is_empty() { bail!("no hay ningún scan"); }
    Ok(Jpeg { piezas, scans, comps, coef, progresivo, mcux, mcuy })
}

/// Unidades del scan: MCUs si va entrelazado, bloques del componente si no.
fn unidades(sc: &Scan, comps: &[CompInfo], mcux: usize, mcuy: usize) -> usize {
    if sc.comps.len() == 1 {
        let c = &comps[sc.comps[0]];
        c.bw * c.bh
    } else {
        mcux * mcuy
    }
}

/// Índices de bloque que toca la unidad `u`, en orden.
fn bloques_de_unidad(sc: &Scan, comps: &[CompInfo], mcux: usize, u: usize, out: &mut Vec<(usize, usize)>) {
    out.clear();
    if sc.comps.len() == 1 {
        let ci = sc.comps[0];
        let c = &comps[ci];
        let (bx, by) = (u % c.bw, u / c.bw);
        out.push((ci, by * c.bwa + bx));
    } else {
        let (mx, my) = (u % mcux, u / mcux);
        for &ci in &sc.comps {
            let c = &comps[ci];
            for vy in 0..c.v {
                for hx in 0..c.h {
                    let bx = mx * c.h + hx;
                    let by = my * c.v + vy;
                    out.push((ci, by * c.bwa + bx));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn decodifica_scan(
    scan: &[u8], sc: &mut Scan, comps: &[CompInfo], coef: &mut [Vec<[i16; 64]>],
    progresivo: bool, mcux: usize, mcuy: usize,
) -> Result<()> {
    let total = unidades(sc, comps, mcux, mcuy);
    if total == 0 { bail!("scan sin unidades"); }
    if sc.comps.len() > 1 && progresivo && sc.ss != 0 {
        bail!("scan de AC entrelazado: la norma no lo permite");
    }
    let inters = intervalos(scan, sc.ri);
    let mut restantes = total;
    let mut idxs: Vec<(usize, usize)> = Vec::new();

    for (k, inter) in inters.iter().enumerate() {
        let en_este = if sc.ri == 0 { restantes } else { sc.ri.min(restantes) };
        if en_este == 0 {
            if k + 1 < inters.len() { bail!("más intervalos que unidades"); }
            break;
        }
        let mut br = BitR::new(inter);
        let mut pred = vec![0i32; comps.len()];
        let mut eobrun: u32 = 0;

        for u in (total - restantes)..(total - restantes + en_este) {
            bloques_de_unidad(sc, comps, mcux, u, &mut idxs);
            for (n, &(ci, bi)) in idxs.iter().enumerate() {
                let pos_en_scan = if sc.comps.len() == 1 { 0 } else {
                    // qué componente del scan es este bloque
                    sc.comps.iter().position(|&c| c == ci).unwrap()
                };
                let _ = n;
                let blk = &mut coef[ci][bi];
                if !progresivo {
                    dec_baseline(&mut br, sc, pos_en_scan, blk, &mut pred[ci])?;
                } else if sc.ss == 0 {
                    if sc.ah == 0 {
                        dec_dc_first(&mut br, sc, pos_en_scan, blk, &mut pred[ci])?;
                    } else {
                        if br.bit()? != 0 { blk[0] |= 1i16 << sc.al; }
                    }
                } else if sc.ah == 0 {
                    dec_ac_first(&mut br, sc, blk, &mut eobrun)?;
                } else {
                    dec_ac_refine(&mut br, sc, blk, &mut eobrun)?;
                }
            }
        }
        let (nr, vr) = br.relleno();
        if br.p != inter.len() { bail!("sobran {} bytes al final de un intervalo", inter.len() - br.p); }
        sc.rellenos.push((nr, vr));
        restantes -= en_este;
    }
    if restantes != 0 { bail!("faltan {restantes} unidades por decodificar"); }
    Ok(())
}

fn dec_baseline(br: &mut BitR, sc: &Scan, pos: usize, blk: &mut [i16; 64], pred: &mut i32) -> Result<()> {
    let t = br.decodifica(&sc.dc[sc.td[pos]])? as u32;
    if t > 15 { bail!("categoría DC fuera de rango"); }
    *pred += extiende(br.bits(t)?, t);
    blk[0] = i16::try_from(*pred).map_err(|_| anyhow::anyhow!("DC fuera de rango"))?;
    let mut u = 1usize;
    while u < 64 {
        let rs = br.decodifica(&sc.ac[sc.ta[pos]])?;
        let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
        if s == 0 {
            if r == 15 { u += 16; continue; }
            break;
        }
        u += r;
        if u >= 64 { bail!("AC fuera del bloque"); }
        let v = extiende(br.bits(s)?, s);
        blk[ZIGZAG[u]] = i16::try_from(v).map_err(|_| anyhow::anyhow!("AC fuera de rango"))?;
        u += 1;
    }
    Ok(())
}

fn dec_dc_first(br: &mut BitR, sc: &Scan, pos: usize, blk: &mut [i16; 64], pred: &mut i32) -> Result<()> {
    let t = br.decodifica(&sc.dc[sc.td[pos]])? as u32;
    if t > 15 { bail!("categoría DC fuera de rango"); }
    *pred += extiende(br.bits(t)?, t);
    let v = *pred << sc.al;
    blk[0] = i16::try_from(v).map_err(|_| anyhow::anyhow!("DC fuera de rango"))?;
    Ok(())
}

fn dec_ac_first(br: &mut BitR, sc: &Scan, blk: &mut [i16; 64], eobrun: &mut u32) -> Result<()> {
    if *eobrun > 0 { *eobrun -= 1; return Ok(()); }
    let mut k = sc.ss;
    while k <= sc.se {
        let rs = br.decodifica(&sc.ac[sc.ta[0]])?;
        let (r, s) = ((rs >> 4) as u32, (rs & 15) as u32);
        if s == 0 {
            if r != 15 {
                *eobrun = (1 << r) - 1;
                if r > 0 { *eobrun += br.bits(r)? as u32; }
                break;
            }
            k += 16;
        } else {
            k += r as usize;
            if k > sc.se { bail!("AC fuera de la banda"); }
            let v = extiende(br.bits(s)?, s) << sc.al;
            blk[ZIGZAG[k]] = i16::try_from(v).map_err(|_| anyhow::anyhow!("AC fuera de rango"))?;
            k += 1;
        }
    }
    Ok(())
}

fn dec_ac_refine(br: &mut BitR, sc: &Scan, blk: &mut [i16; 64], eobrun: &mut u32) -> Result<()> {
    let p1: i16 = 1 << sc.al;
    let m1: i16 = -1i16 << sc.al;
    let mut k = sc.ss;

    if *eobrun == 0 {
        while k <= sc.se {
            let rs = br.decodifica(&sc.ac[sc.ta[0]])?;
            let (mut r, s) = ((rs >> 4) as i32, (rs & 15) as u32);
            let mut valor: i16 = 0;
            if s == 0 {
                if r != 15 {
                    *eobrun = 1 << r;
                    if r > 0 { *eobrun += br.bits(r as u32)? as u32; }
                    break;
                }
                // r == 15: saltar 16 coeficientes de historia nula
            } else {
                if s != 1 { bail!("refinamiento AC con magnitud != 1"); }
                valor = if br.bit()? != 0 { p1 } else { m1 };
            }
            while k <= sc.se {
                let idx = ZIGZAG[k];
                if blk[idx] != 0 {
                    if br.bit()? != 0 && (blk[idx] & p1) == 0 {
                        blk[idx] = if blk[idx] >= 0 { blk[idx] + p1 } else { blk[idx] + m1 };
                    }
                } else {
                    if r == 0 {
                        if valor != 0 { blk[idx] = valor; }
                        k += 1;
                        break;
                    }
                    r -= 1;
                }
                k += 1;
            }
        }
    }
    if *eobrun > 0 {
        while k <= sc.se {
            let idx = ZIGZAG[k];
            if blk[idx] != 0 && br.bit()? != 0 && (blk[idx] & p1) == 0 {
                blk[idx] = if blk[idx] >= 0 { blk[idx] + p1 } else { blk[idx] + m1 };
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

// ---------- re-emisión ----------

/// Estado del codificador progresivo: EOBRUN pendiente y bits de corrección en cola.
/// Réplica del jcphuff de libjpeg, que es quien generó estos ficheros.
#[derive(Default)]
struct EstadoAc { eobrun: u32, be: Vec<u32> }

impl EstadoAc {
    fn emite_eobrun(&mut self, w: &mut BitW, h: &Huff) -> Result<()> {
        if self.eobrun > 0 {
            let nbits = nbits_de(self.eobrun as i32).saturating_sub(1);
            w.simbolo(h, (nbits as usize) << 4)?;
            if nbits > 0 { w.bits(self.eobrun, nbits); }
            self.eobrun = 0;
        }
        for i in 0..self.be.len() { let b = self.be[i]; w.bit(b); }
        self.be.clear();
        Ok(())
    }
}

pub fn emitir(j: &Jpeg) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    for pieza in &j.piezas {
        match pieza {
            Pieza::Lit(b) => out.extend_from_slice(b),
            Pieza::Scan(i) => emite_scan(&mut out, &j.scans[*i], j, *i)?,
        }
    }
    Ok(out)
}

fn emite_scan(out: &mut Vec<u8>, sc: &Scan, j: &Jpeg, _i: usize) -> Result<()> {
    let total = unidades(sc, &j.comps, j.mcux, j.mcuy);
    let mut restantes = total;
    let mut idxs: Vec<(usize, usize)> = Vec::new();
    let mut rst = 0u8;

    for (k, &(nr, vr)) in sc.rellenos.iter().enumerate() {
        let en_este = if sc.ri == 0 { restantes } else { sc.ri.min(restantes) };
        let mut w = BitW::default();
        let mut pred = vec![0i32; j.comps.len()];
        let mut ea = EstadoAc::default();

        for u in (total - restantes)..(total - restantes + en_este) {
            bloques_de_unidad(sc, &j.comps, j.mcux, u, &mut idxs);
            for &(ci, bi) in idxs.iter() {
                let pos = if sc.comps.len() == 1 { 0 } else { sc.comps.iter().position(|&c| c == ci).unwrap() };
                let blk = &j.coef[ci][bi];
                if !j.progresivo {
                    enc_baseline(&mut w, sc, pos, blk, &mut pred[ci])?;
                } else if sc.ss == 0 {
                    if sc.ah == 0 {
                        enc_dc_first(&mut w, sc, pos, blk, &mut pred[ci])?;
                    } else {
                        w.bit((shr(blk[0] as i32, sc.al) & 1) as u32);
                    }
                } else if sc.ah == 0 {
                    enc_ac_first(&mut w, sc, blk, &mut ea)?;
                } else {
                    enc_ac_refine(&mut w, sc, blk, &mut ea)?;
                }
            }
        }
        if j.progresivo && sc.ss != 0 { ea.emite_eobrun(&mut w, &sc.ac[sc.ta[0]])?; }
        w.cierra(nr, vr)?;
        out.extend_from_slice(&w.out);
        restantes -= en_este;
        if k + 1 < sc.rellenos.len() {
            out.push(0xFF);
            out.push(0xD0 + (rst & 7));
            rst = rst.wrapping_add(1);
        }
    }
    Ok(())
}

fn enc_baseline(w: &mut BitW, sc: &Scan, pos: usize, blk: &[i16; 64], pred: &mut i32) -> Result<()> {
    let diff = blk[0] as i32 - *pred;
    *pred = blk[0] as i32;
    let (t, rep) = categoria(diff);
    w.simbolo(&sc.dc[sc.td[pos]], t as usize)?;
    if t > 0 { w.bits(rep, t); }
    let ha = &sc.ac[sc.ta[pos]];
    let ultimo = (1..64).rev().find(|&u| blk[ZIGZAG[u]] != 0).unwrap_or(0);
    let mut ceros = 0u32;
    for u in 1..=ultimo {
        let v = blk[ZIGZAG[u]] as i32;
        if v == 0 { ceros += 1; continue; }
        while ceros >= 16 { w.simbolo(ha, 0xF0)?; ceros -= 16; }
        let (s, rep) = categoria(v);
        w.simbolo(ha, ((ceros as usize) << 4) | s as usize)?;
        w.bits(rep, s);
        ceros = 0;
    }
    if ultimo < 63 { w.simbolo(ha, 0)?; }
    Ok(())
}

fn categoria(v: i32) -> (u32, u32) {
    let t = nbits_de(v.abs());
    let rep = if v < 0 { (v - 1) as u32 & ((1u32 << t) - 1) } else { v as u32 };
    (t, rep)
}

fn enc_dc_first(w: &mut BitW, sc: &Scan, pos: usize, blk: &[i16; 64], pred: &mut i32) -> Result<()> {
    let val = shr(blk[0] as i32, sc.al);
    let diff = val - *pred;
    *pred = val;
    let (t, rep) = categoria(diff);
    w.simbolo(&sc.dc[sc.td[pos]], t as usize)?;
    if t > 0 { w.bits(rep, t); }
    Ok(())
}

fn enc_ac_first(w: &mut BitW, sc: &Scan, blk: &[i16; 64], ea: &mut EstadoAc) -> Result<()> {
    let ha = &sc.ac[sc.ta[0]];
    let mut r = 0u32;
    for k in sc.ss..=sc.se {
        let mut temp = blk[ZIGZAG[k]] as i32;
        if temp == 0 { r += 1; continue; }
        let temp2;
        if temp < 0 { temp = -temp; temp >>= sc.al; temp2 = !temp; }
        else { temp >>= sc.al; temp2 = temp; }
        if temp == 0 { r += 1; continue; }
        if ea.eobrun > 0 { ea.emite_eobrun(w, ha)?; }
        while r > 15 { w.simbolo(ha, 0xF0)?; r -= 16; }
        let nbits = nbits_de(temp);
        w.simbolo(ha, ((r as usize) << 4) | nbits as usize)?;
        w.bits(temp2 as u32 & ((1u32 << nbits) - 1), nbits);
        r = 0;
    }
    if r > 0 {
        ea.eobrun += 1;
        if ea.eobrun == 0x7FFF { ea.emite_eobrun(w, ha)?; }
    }
    Ok(())
}

fn enc_ac_refine(w: &mut BitW, sc: &Scan, blk: &[i16; 64], ea: &mut EstadoAc) -> Result<()> {
    let ha = &sc.ac[sc.ta[0]];
    // absvalues y el índice del último coeficiente que se vuelve no nulo AHORA
    let mut abs = [0i32; 64];
    let mut eob = 0usize;
    for k in sc.ss..=sc.se {
        let mut t = blk[ZIGZAG[k]] as i32;
        if t < 0 { t = -t; }
        t >>= sc.al;
        abs[k] = t;
        if t == 1 { eob = k; }
    }
    let mut r = 0u32;
    let mut br: Vec<u32> = Vec::new();
    for k in sc.ss..=sc.se {
        let t = abs[k];
        if t == 0 { r += 1; continue; }
        while r > 15 && k <= eob {
            ea.emite_eobrun(w, ha)?;
            w.simbolo(ha, 0xF0)?;
            r -= 16;
            for &b in &br { w.bit(b); }
            br.clear();
        }
        if t > 1 {
            br.push((t & 1) as u32);
            continue;
        }
        ea.emite_eobrun(w, ha)?;
        w.simbolo(ha, ((r as usize) << 4) | 1)?;
        w.bit(if blk[ZIGZAG[k]] < 0 { 0 } else { 1 });
        for &b in &br { w.bit(b); }
        br.clear();
        r = 0;
    }
    if r > 0 || !br.is_empty() {
        ea.eobrun += 1;
        ea.be.extend_from_slice(&br);
        if ea.eobrun == 0x7FFF || ea.be.len() > 1000 { ea.emite_eobrun(w, ha)?; }
    }
    Ok(())
}

/// Puerta: parsea y re-emite. `Ok(())` sólo si sale byte a byte idéntico.
pub fn parseo_ida_y_vuelta(d: &[u8]) -> Result<()> {
    let j = parsear(d)?;
    let v = emitir(&j)?;
    if v.len() != d.len() { bail!("re-emisión con longitud distinta: {} vs {}", v.len(), d.len()); }
    if let Some(off) = v.iter().zip(d).position(|(a, b)| a != b) {
        bail!("re-emisión diverge en el offset {off}");
    }
    Ok(())
}

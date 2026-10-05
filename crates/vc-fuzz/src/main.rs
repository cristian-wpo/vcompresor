// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! F2.5 — fuzzer de round-trip para vcompresor. Regla del brief: una sola discrepancia = fallo total.
//! Genera ficheros patológicos + aleatorios, comprime, verifica y compara byte a byte.
//! Al primer byte distinto: dice QUÉ fichero y en QUÉ offset divergió, y aborta.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self { Rng(seed ^ 0x9E3779B97F4A7C15) }
    fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
}

fn gen(rng: &mut Rng) -> Vec<u8> {
    // tamaños: incluye 0, 1, y saltos de frame (>8 MiB del carril texto)
    let n = match rng.below(100) {
        0..=9   => rng.below(2),                    // vacío o 1 byte
        10..=39 => rng.below(256),                  // diminutos
        40..=74 => rng.below(64 * 1024),            // pequeños
        75..=94 => rng.below(1024 * 1024),          // medianos (cruzan chunks)
        _       => 8 * 1024 * 1024 + rng.below(4 * 1024 * 1024), // cruzan frames de 8 MiB
    };
    match rng.below(7) {
        0 => vec![0u8; n],                                              // todo ceros
        1 => vec![0xFFu8; n],                                           // todo 0xFF
        2 => (0..n).map(|_| rng.next() as u8).collect(),               // aleatorio incompresible
        3 => { let p = b"function foo() { return 42; }\n"; (0..n).map(|i| p[i % p.len()]).collect() } // repetitivo
        4 => (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(7)).collect(), // estructurado
        5 => { // gzip real embebido (ejercita el detector de .gz aunque la ext no sea gz)
            let inner: Vec<u8> = (0..n).map(|i| b"SELECT * FROM t;\n"[i % 16]).collect();
            gzip(&inner)
        }
        _ => { // mezcla: bloques comprimibles + ruido
            let mut v = Vec::with_capacity(n);
            let mut i = 0;
            while i < n { if (i / 997) % 2 == 0 { v.push(b'A' + (i % 26) as u8); } else { v.push(rng.next() as u8); } i += 1; }
            v
        }
    }
}

const CRC_TAB: [u32; 256] = crc_tab();
const fn crc_tab() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[n] = c;
        n += 1;
    }
    t
}
fn crc32(d: &[u8]) -> u32 {
    let mut c = 0xFFFFFFFFu32;
    for &b in d { c = CRC_TAB[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8); }
    c ^ 0xFFFFFFFF
}

/// Escritor de bits de DEFLATE: los campos van LSB primero dentro del byte, pero los
/// códigos Huffman se emiten con el bit más significativo por delante. Es el error clásico.
struct BitW { out: Vec<u8>, acc: u32, n: u32 }
impl BitW {
    fn bits(&mut self, v: u32, n: u32) {
        self.acc |= v << self.n;
        self.n += n;
        while self.n >= 8 { self.out.push(self.acc as u8); self.acc >>= 8; self.n -= 8; }
    }
    fn huff(&mut self, code: u32, n: u32) {
        for i in (0..n).rev() { self.bits((code >> i) & 1, 1); }
    }
    fn fin(mut self) -> Vec<u8> {
        if self.n > 0 { self.out.push(self.acc as u8); }
        self.out
    }
}

/// gzip con DEFLATE de Huffman FIJO, solo literales (sin buscar coincidencias). No es el
/// mejor ratio —da igual, es material de prueba— pero sí es un flujo Huffman de verdad,
/// que es lo que tiene que masticar preflate. Con bloques «stored» el test no valdría nada.
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut w = BitW { out: Vec::with_capacity(data.len() + 64), acc: 0, n: 0 };
    w.bits(1, 1); // BFINAL
    w.bits(1, 2); // BTYPE=01, Huffman fijo
    for &b in data {
        let v = b as u32;
        if v < 144 { w.huff(0x30 + v, 8); } else { w.huff(0x190 + v - 144, 9); }
    }
    w.huff(0, 7); // símbolo 256 = fin de bloque
    let cuerpo = w.fin();
    let mut v = Vec::with_capacity(cuerpo.len() + 18);
    v.extend_from_slice(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]); // cabecera gzip, SO=Unix
    v.extend_from_slice(&cuerpo);
    v.extend_from_slice(&crc32(data).to_le_bytes());
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v
}

fn primer_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    if a.len() != b.len() { return Some(a.len().min(b.len())); }
    a.iter().zip(b).position(|(x, y)| x != y)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let iters: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let per: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(500);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let base = std::env::temp_dir().join(format!("vcfuzz-{seed}"));
    let _ = std::fs::remove_dir_all(&base);

    let mut rng = Rng::new(seed);
    let (mut nfiles, mut nbytes, mut nstore_expand) = (0u64, 0u64, 0u64);
    let t0 = std::time::Instant::now();

    for it in 0..iters {
        let src = base.join(format!("s{it}"));
        let out = base.join(format!("o{it}"));
        std::fs::create_dir_all(&src)?;
        let mut originales: Vec<(PathBuf, Vec<u8>)> = Vec::with_capacity(per);
        let mut in_bytes = 0u64;
        for k in 0..per {
            let sub = if rng.below(3) == 0 { src.join(format!("d{}/f{k}.bin", rng.below(5))) } else { src.join(format!("f{k}.bin")) };
            if let Some(p) = sub.parent() { std::fs::create_dir_all(p)?; }
            let data = gen(&mut rng);
            std::fs::write(&sub, &data)?;
            in_bytes += data.len() as u64;
            originales.push((sub, data));
            nbytes += 0;
        }
        let arch = base.join(format!("a{it}.vc"));
        let st = vc_core::comprimir(&src, &arch, 19, true, true, false, 0)?;

        // no-expansión: incompresible sale ≤ entrada + margen fijo pequeño por fichero
        let arch_sz = std::fs::metadata(&arch)?.len();
        if arch_sz > in_bytes + 1024 + (per as u64) * 96 { nstore_expand += 1; }

        // verificación interna (blake3 de cada fichero)
        let mut fallos = Vec::new();
        let (_nf, _nb, nfail) = vc_core::extraer(&arch, Some(&out), &mut fallos)?;
        if nfail > 0 { for f in &fallos { eprintln!("{f}"); } bail!("VERIFICACIÓN interna falló en iter {it}"); }

        // comparación byte-exacta explícita contra los originales en memoria
        for (p, data) in &originales {
            let rel = p.strip_prefix(&src).unwrap();
            let got = std::fs::read(out.join(rel))?;
            if let Some(off) = primer_diff(data, &got) {
                bail!("ROUND-TRIP FAIL: fichero {} DIVERGE en offset {off} (orig={} bytes, extraído={} bytes) [iter {it}]",
                    rel.display(), data.len(), got.len());
            }
            nfiles += 1;
        }
        let _ = st;
        std::fs::remove_dir_all(&src)?; std::fs::remove_dir_all(&out)?; std::fs::remove_file(&arch)?;
        if it % 50 == 0 || it + 1 == iters {
            let dt = t0.elapsed().as_secs_f64();
            println!("iter {}/{iters}  ficheros_ok={nfiles}  {:.0} files/s  expand={nstore_expand}", it + 1, nfiles as f64 / dt.max(0.001));
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    println!("\nFUZZ OK: {nfiles} ficheros round-trip byte-idénticos en {:.1}s. no-expansión violada: {nstore_expand} veces.", t0.elapsed().as_secs_f64());
    if nstore_expand > 0 { bail!("NO-EXPANSIÓN violada {nstore_expand} veces — revisar store fallback"); }
    let _ = nbytes;
    Ok(())
}

// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿DE CUANTOS BITS TIENEN QUE SER LAS RANURAS DE LA CADENA? El histograma, no la suposicion.
//!
//! LA PREGUNTA ES DEL DUEÑO (01-08): «¿que impide que `prev` no sea solo 4 bits? siempre debemos
//! preguntarnos estas cosas como norma general». Y la norma general YA ESTA en el instrumento —es
//! P3 del camino, «¿reservamos memoria por el caso peor o por el real?»— pero no se le habia
//! aplicado a `prev`.
//!
//! LO QUE DETERMINA SU ANCHO HOY. `prev` guarda la POSICION ABSOLUTA de la ocurrencia anterior con
//! el mismo hash. Con ventana de 32 MiB hacen falta 25 bits, asi que u32 es el hueco natural. No
//! fue una eleccion: sale del tamaño de ventana. Por eso zlib SI usa 16 bits — su ventana son
//! 32 KB y le caben.
//!
//! 4 bits es imposible: 16 valores no direccionan 33 millones de posiciones. Pero 16 PUEDE serlo
//! si en vez de la posicion se guarda EL SALTO -`p - anterior`-, porque los saltos suelen ser
//! cortos. Si el 95% cabe en 16 bits, con un escape para el resto la cadena pasa de 128 MB a
//! 64 MB por hilo, y a 10 hilos son 640 MB.
//!
//! ⚠️ EL ESCAPE NO ES GRATIS Y HAY QUE DECIR SU PRECIO: un salto que no cabe hay que cortarlo
//! -tratarlo como fin de cadena- o guardarlo aparte. Cortarlo PIERDE coincidencias lejanas, que
//! son justo las que mas bytes ahorran. Por eso lo que decide no es «cuantos caben» sino «cuanto
//! peso de las coincidencias que USAMOS vive por encima del tope», y esto cuenta las dos cosas.
//!
//!   uso:  vlz-saltos <fichero-o-directorio>

use std::collections::BTreeMap;

const VENTANA_TOPE: usize = 1 << 25;
const CUBOS: usize = 1 << 20;
const LOG_CUBOS: u32 = CUBOS.trailing_zeros();
const MIN_COPIA: usize = 4;

#[inline(always)]
fn hash4(d: &[u8], p: usize) -> usize {
    let v = u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]);
    ((v.wrapping_mul(2654435761)) >> (32 - LOG_CUBOS)) as usize & (CUBOS - 1)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 { eprintln!("uso: vlz-saltos <fichero-o-dir>"); std::process::exit(2) }
    let p = std::path::Path::new(&a[1]);
    let mut fs: Vec<std::path::PathBuf> = Vec::new();
    if p.is_dir() {
        let mut pila = vec![p.to_path_buf()];
        while let Some(d) = pila.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            let mut v: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            v.sort();
            for q in v { if q.is_dir() { pila.push(q) } else { fs.push(q) } }
        }
    } else { fs.push(p.to_path_buf()) }
    fs.sort();
    let mut d = Vec::new();
    for f in &fs { if let Ok(b) = std::fs::read(f) { d.extend_from_slice(&b) } }
    let n = d.len();
    if n < 64 { eprintln!("muestra vacia"); std::process::exit(3) }

    let ventana = n.next_power_of_two().clamp(1 << 16, VENTANA_TOPE);
    let mascara = ventana - 1;
    let mut head = vec![u32::MAX; CUBOS];
    let mut prev = vec![u32::MAX; ventana];

    // DOS histogramas distintos, y la diferencia es el nudo del asunto:
    //   ENLACES  — todos los saltos que se guardan en la cadena. Dice cuantos CABEN.
    //   USADAS   — solo los saltos de las coincidencias que de verdad se emiten, pesados por los
    //              BYTES que ahorran. Dice cuanto se PIERDE al cortar. Un tope que deja fuera al
    //              0,1% de los enlaces pero al 30% de los bytes ahorrados no sirve.
    let mut enlaces: BTreeMap<u32, u64> = BTreeMap::new();
    let mut usadas: BTreeMap<u32, u64> = BTreeMap::new();
    let mut bytes_usados: BTreeMap<u32, u64> = BTreeMap::new();
    let mut n_enlaces = 0u64;
    let mut n_usadas = 0u64;
    let mut n_bytes = 0u64;

    let mut pos = 0usize;
    while pos + MIN_COPIA <= n {
        let h = hash4(&d, pos);
        let ant = head[h];
        if ant != u32::MAX {
            let salto = pos as u32 - ant;
            if (salto as usize) <= ventana {
                *enlaces.entry(32 - salto.leading_zeros()).or_default() += 1;
                n_enlaces += 1;
            }
        }
        // la coincidencia que un buscador codicioso de esfuerzo 8 tomaria aqui
        let mut cur = head[h];
        let (mut ml, mut md) = (0usize, 0u32);
        let mut pasos = 0;
        let tope = (1usize << 16).min(n - pos);
        while cur != u32::MAX && pasos < 8 {
            let cp = cur as usize;
            let dd = pos - cp;
            if dd == 0 || dd > ventana { break }
            let mut k = 0usize;
            while k < tope && d[cp + k] == d[pos + k] { k += 1 }
            if k > ml { ml = k; md = dd as u32 }
            cur = prev[cp & mascara];
            pasos += 1;
        }
        prev[pos & mascara] = head[h];
        head[h] = pos as u32;
        if ml >= MIN_COPIA && md > 0 {
            let b = 32 - md.leading_zeros();
            *usadas.entry(b).or_default() += 1;
            *bytes_usados.entry(b).or_default() += ml as u64;
            n_usadas += 1; n_bytes += ml as u64;
            pos += ml;
        } else { pos += 1 }
    }

    println!("\n  vlz · {} ficheros · {} B · ventana {} MiB\n", fs.len(), sep(n), ventana >> 20);
    println!("  {:<8} {:>12} {:>8}   {:>12} {:>8}   {:>14} {:>8}",
             "bits", "enlaces", "acum", "copias", "acum", "bytes ahorr.", "acum");
    println!("  {}", "-".repeat(84));
    let (mut ae, mut au, mut ab) = (0u64, 0u64, 0u64);
    for b in 1..=26u32 {
        let e = *enlaces.get(&b).unwrap_or(&0);
        let u = *usadas.get(&b).unwrap_or(&0);
        let y = *bytes_usados.get(&b).unwrap_or(&0);
        if e == 0 && u == 0 { continue }
        ae += e; au += u; ab += y;
        println!("  {:<8} {:>12} {:>7.2}%   {:>12} {:>7.2}%   {:>14} {:>7.2}%",
                 b, sep(e as usize), 100.0 * ae as f64 / n_enlaces.max(1) as f64,
                 sep(u as usize), 100.0 * au as f64 / n_usadas.max(1) as f64,
                 sep(y as usize), 100.0 * ab as f64 / n_bytes.max(1) as f64);
    }
    println!();
    for tope in [8u32, 12, 16, 20, 24] {
        let e: u64 = enlaces.iter().filter(|(&b, _)| b <= tope).map(|(_, &v)| v).sum();
        let y: u64 = bytes_usados.iter().filter(|(&b, _)| b <= tope).map(|(_, &v)| v).sum();
        println!("  con ranuras de {:>2} bits caben el {:>6.2}% de los enlaces y se conservaria el {:>6.2}% de los bytes ahorrados",
                 tope, 100.0 * e as f64 / n_enlaces.max(1) as f64, 100.0 * y as f64 / n_bytes.max(1) as f64);
    }
    println!();
    println!("  ⚠️ LA COLUMNA QUE DECIDE ES LA ULTIMA, no la primera. Un tope que deja fuera al 0,1%");
    println!("     de los ENLACES puede dejar fuera al 30% de los BYTES AHORRADOS: las coincidencias");
    println!("     lejanas son pocas y son las que mas pesan.");
}

fn sep(n: usize) -> String {
    let s = n.to_string();
    let mut r = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 { r.push('.') }
        r.push(c);
    }
    r
}

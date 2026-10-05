// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Gate del Hito 1 del carril JPEG propio: parsear → re-emitir sobre el corpus real.
//! Uso: jpegrt <dir> [max] [hilos]
//!
//! Separa lo que importa: RECHAZADO (no soportado, cae a store, es correcto) de DIVERGE
//! (parseó pero la re-emisión no sale idéntica), que es lo único que da miedo.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct Acum {
    n: u64,
    ok: u64,
    bytes_ok: u64,
    rechazados: Vec<(String, String)>,
    diverge: Vec<(String, String)>,
}

fn buscar(dir: &std::path::Path, max: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= max { return; }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut es: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    es.sort_by_key(|e| e.file_name());
    for e in es {
        if out.len() >= max { return; }
        let p = e.path();
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        if md.is_dir() {
            buscar(&p, max, out);
        } else if md.is_file() {
            let x = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if x == "jpg" || x == "jpeg" { out.push(p); }
        }
    }
}

/// Agrupa el motivo para que el resumen sea legible en vez de 10.000 líneas distintas.
fn motivo(e: &str) -> String {
    for clave in [
        "progresivo", "no soportado", "selección espectral", "scan parcial",
        "marcador inesperado", "scan truncado", "no empieza por SOI", "sobran",
        "Huffman", "fuera de rango", "truncado", "diverge", "longitud distinta",
    ] {
        if e.contains(clave) { return clave.to_string(); }
    }
    e.chars().take(48).collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| ".".into());
    let max: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let hilos: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(6);

    let mut rutas = Vec::new();
    buscar(std::path::Path::new(&dir), max, &mut rutas);
    eprintln!("ficheros: {}  hilos: {hilos}", rutas.len());

    let acum = Arc::new(Mutex::new(Acum::default()));
    let t0 = Instant::now();
    let trozos: Vec<Vec<PathBuf>> = {
        let mut v: Vec<Vec<PathBuf>> = (0..hilos).map(|_| Vec::new()).collect();
        for (i, r) in rutas.into_iter().enumerate() { v[i % hilos].push(r); }
        v
    };
    let mut hs = Vec::new();
    for trozo in trozos {
        let acum = Arc::clone(&acum);
        hs.push(std::thread::spawn(move || {
            let mut loc = Acum::default();
            for r in trozo {
                let Ok(d) = std::fs::read(&r) else { continue };
                loc.n += 1;
                match vc_core::jpeg::parseo_ida_y_vuelta(&d) {
                    Ok(()) => { loc.ok += 1; loc.bytes_ok += d.len() as u64; }
                    Err(e) => {
                        let m = motivo(&e.to_string());
                        let par = m == "diverge" || m == "longitud distinta";
                        let dest = if par { &mut loc.diverge } else { &mut loc.rechazados };
                        dest.push((r.display().to_string(), m));
                    }
                }
            }
            let mut g = acum.lock().unwrap();
            g.n += loc.n;
            g.ok += loc.ok;
            g.bytes_ok += loc.bytes_ok;
            g.rechazados.extend(loc.rechazados);
            g.diverge.extend(loc.diverge);
        }));
    }
    for h in hs { let _ = h.join(); }
    let s = t0.elapsed().as_secs_f64();
    let g = acum.lock().unwrap();

    println!("--- HITO 1: parseo -> re-emisión byte a byte ---");
    println!("ficheros            : {}", g.n);
    println!("BYTE-IDÉNTICOS      : {} ({:.2}%)", g.ok, 100.0 * g.ok as f64 / g.n.max(1) as f64);
    println!("rechazados (a store): {} ({:.2}%)", g.rechazados.len(), 100.0 * g.rechazados.len() as f64 / g.n.max(1) as f64);
    println!("DIVERGEN            : {}   <-- esto es lo que no puede pasar", g.diverge.len());
    println!("velocidad           : {:.1} MB/s ({hilos} hilos, {s:.1}s)", g.bytes_ok as f64 / 1048576.0 / s.max(0.001));

    let mut por: std::collections::BTreeMap<&str, usize> = Default::default();
    for (_, m) in g.rechazados.iter() { *por.entry(m.as_str()).or_default() += 1; }
    if !por.is_empty() {
        println!("\nmotivos de rechazo:");
        let mut v: Vec<_> = por.into_iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (m, n) in v { println!("  {n:6}  {m}"); }
    }
    if !g.diverge.is_empty() {
        println!("\nDIVERGENCIAS (primeras 10):");
        for (f, m) in g.diverge.iter().take(10) { println!("  {m}  {f}"); }
        std::process::exit(1);
    }
}

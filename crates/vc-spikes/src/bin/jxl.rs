// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Spike F2.3 — mide el carril JPEG→JXL sobre el corpus real ANTES de integrarlo.
//! Uso: jxl <dir> [max_ficheros] [esfuerzo] [hilos]
//! Reporta: byte-exactos / rechazados, ratio vs original y vs zstd-19 directo, y MB/s.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct Acum {
    n: u64,
    ok: u64,
    rechazados: u64,
    orig: u64,
    jxl: u64,
    zst: u64,
    orig_ok: u64,
    fallos: Vec<String>,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| ".".into());
    let max: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let esfuerzo: i64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(7);
    let hilos: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);

    fn buscar(dir: &std::path::Path, max: usize, out: &mut Vec<PathBuf>) {
        if out.len() >= max {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut es: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        es.sort_by_key(|e| e.file_name());
        for e in es {
            if out.len() >= max {
                return;
            }
            let p = e.path();
            let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
            if md.is_dir() {
                buscar(&p, max, out);
            } else if md.is_file() {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                if ext == "jpg" || ext == "jpeg" {
                    out.push(p);
                }
            }
        }
    }
    let mut rutas: Vec<PathBuf> = Vec::new();
    buscar(std::path::Path::new(&dir), max, &mut rutas);
    eprintln!("ficheros: {}  esfuerzo: {}  hilos: {}", rutas.len(), esfuerzo, hilos);

    let acum = Arc::new(Mutex::new(Acum::default()));
    let t0 = Instant::now();
    let trozos: Vec<Vec<PathBuf>> = {
        let mut v: Vec<Vec<PathBuf>> = (0..hilos).map(|_| Vec::new()).collect();
        for (i, r) in rutas.into_iter().enumerate() {
            v[i % hilos].push(r);
        }
        v
    };
    let mut hs = Vec::new();
    for trozo in trozos {
        let acum = Arc::clone(&acum);
        hs.push(std::thread::spawn(move || {
            let mut loc = Acum::default();
            for r in trozo {
                let Ok(jpeg) = std::fs::read(&r) else { continue };
                if jpeg.len() < 128 {
                    continue;
                }
                loc.n += 1;
                loc.orig += jpeg.len() as u64;
                loc.zst += zstd::bulk::compress(&jpeg, 19).map(|c| c.len()).unwrap_or(jpeg.len()) as u64;
                match vc_core::jxl::jxl_verificado(&jpeg, esfuerzo) {
                    Some(j) => {
                        loc.ok += 1;
                        loc.jxl += j.len() as u64;
                        loc.orig_ok += jpeg.len() as u64;
                    }
                    None => {
                        loc.rechazados += 1;
                        loc.jxl += jpeg.len() as u64; // caería a store
                        if loc.fallos.len() < 12 {
                            loc.fallos.push(r.display().to_string());
                        }
                    }
                }
            }
            let mut g = acum.lock().unwrap();
            g.n += loc.n;
            g.ok += loc.ok;
            g.rechazados += loc.rechazados;
            g.orig += loc.orig;
            g.jxl += loc.jxl;
            g.zst += loc.zst;
            g.orig_ok += loc.orig_ok;
            for f in loc.fallos {
                if g.fallos.len() < 12 {
                    g.fallos.push(f);
                }
            }
        }));
    }
    for h in hs {
        let _ = h.join();
    }
    let s = t0.elapsed().as_secs_f64();
    let g = acum.lock().unwrap();
    let mb = g.orig as f64 / 1048576.0;
    println!("--- F2.3 spike JPEG->JXL (esfuerzo {esfuerzo}, {hilos} hilos) ---");
    println!("ficheros            : {}", g.n);
    println!("byte-exactos        : {} ({:.2}%)", g.ok, 100.0 * g.ok as f64 / g.n.max(1) as f64);
    println!("rechazados (->store): {}", g.rechazados);
    println!("original            : {} bytes ({:.1} MB)", g.orig, mb);
    println!("zstd-19 directo     : {} bytes ({:.2}% del original)", g.zst, 100.0 * g.zst as f64 / g.orig.max(1) as f64);
    println!("carril JXL          : {} bytes ({:.2}% del original)", g.jxl, 100.0 * g.jxl as f64 / g.orig.max(1) as f64);
    println!("ganancia vs zstd-19 : {:.2}%", 100.0 * (g.zst as f64 - g.jxl as f64) / g.zst.max(1) as f64);
    println!("solo los aceptados  : {:.2}% del original", 100.0 * (g.jxl.saturating_sub(g.orig - g.orig_ok)) as f64 / g.orig_ok.max(1) as f64);
    println!("tiempo              : {s:.1}s  ({:.2} MB/s con {hilos} hilos)", mb / s.max(0.001));
    if !g.fallos.is_empty() {
        println!("ejemplos rechazados:");
        for f in g.fallos.iter() {
            println!("  {f}");
        }
    }
}

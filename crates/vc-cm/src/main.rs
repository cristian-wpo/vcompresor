// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! CLI de vc-cm. El compresor vive en la lib; esto es solo la envoltura.
use std::fs;
use vc_cm::{comprimir, descomprimir};

/// VC_MM_STATS=1 · cuantos arranques del `match` eran de verdad y cuantos solo el mismo hash.
/// Se imprime al final porque los contadores son globales y acumulan todos los tramos.
fn mm_stats() {
    if std::env::var("VC_MM_STATS").is_err() { return; }
    use std::sync::atomic::Ordering::Relaxed;
    let (c, f) = (vc_cm::MM_CIERTOS.load(Relaxed), vc_cm::MM_FALSOS.load(Relaxed));
    let t = c + f;
    if t == 0 { eprintln!("mm: 0 arranques"); return; }
    eprintln!("mm: {t} arranques · {c} ciertos ({:.2}%) · {f} FALSOS ({:.2}%)",
              100.0 * c as f64 / t as f64, 100.0 * f as f64 / t as f64);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 { eprintln!("uso: vc-cm c|d|test <in> [out]"); std::process::exit(2); }
    let data = fs::read(&a[2]).expect("leer entrada");
    match a[1].as_str() {
        "c" => {
            let c = comprimir(&data);
            let bpb = 8.0 * c.len() as f64 / data.len().max(1) as f64;
            eprintln!("in={} out={} bpb={:.4} ratio={:.2}%", data.len(), c.len(), bpb, 100.0 * c.len() as f64 / data.len().max(1) as f64);
            mm_stats();
            // F6·Q6 · la tabla de cobertura, solo si se compilo con `--features cobertura`.
            #[cfg(feature = "cobertura")]
            eprint!("{}", vc_cm::cobertura::informe());
            fs::write(&a[3], &c).unwrap();
        }
        "d" => { fs::write(&a[3], descomprimir(&data).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(3) })).unwrap(); }
        "test" => {
            let t0 = std::time::Instant::now();
            let c = comprimir(&data);
            let tc = t0.elapsed().as_secs_f64();
            let t1 = std::time::Instant::now();
            let d = descomprimir(&c).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(3) });
            let td = t1.elapsed().as_secs_f64();
            let ok = d == data;
            let bpb = 8.0 * c.len() as f64 / data.len().max(1) as f64;
            println!("in={} out={} bpb={:.4} round_trip={} comp={:.1}MB/s decomp={:.1}MB/s",
                data.len(), c.len(), bpb, if ok {"OK"} else {"FAIL"},
                data.len() as f64/1e6/tc.max(1e-6), data.len() as f64/1e6/td.max(1e-6));
            if !ok {
                let off = data.iter().zip(&d).position(|(x,y)| x!=y).unwrap_or(data.len().min(d.len()));
                eprintln!("PRIMER BYTE DISTINTO en offset {off} (len orig={} dec={})", data.len(), d.len());
                std::process::exit(1);
            }
        }
        _ => { eprintln!("modo desconocido"); std::process::exit(2); }
    }
}

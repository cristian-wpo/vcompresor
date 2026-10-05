// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿DONDE ESTA EL 78%? Reparte el coste de preflate entre sus cuatro fases.
//!
//! El desglose por fases dice que preflate es el 78% del reloj de todo el archivador: apagarlo
//! baja de 347,9 s a 75,9 s. Pero `preflate_whole_deflate_stream` hace LAS CUATRO COSAS en una
//! sola llamada —inflar, estimar los parametros del codificador original, y verificar
//! recomprimiendo— asi que ese 78% esta repartido ahi dentro y no se sabe en que proporcion.
//!
//! ESTO DECIDE EL TAMAÑO DEL PROYECTO «PREFLATE PROPIO», y por un factor de diez:
//!
//!   si el grueso es INFLAR      -> ya tenemos la pieza (vc-inflate, 1.973/1.973 a 285 MB/s)
//!                                  y hacerlo nuestro es cablearla: dias.
//!   si el grueso es ESTIMAR     -> hay que construir la busqueda de parametros del codificador,
//!                                  que es la parte dificil de verdad: meses.
//!
//! Escribir codigo antes de saber cual de las dos es, seria elegir el tamaño del proyecto a
//! ciegas.
//!
//! COMO SE REPARTE. No se puede instrumentar preflate-rs por dentro sin tocarlo, asi que se mide
//! por diferencia contra algo que SI controlamos: nuestro inflate hace exactamente la fase 1
//! sobre los mismos flujos. Lo que preflate tarde de mas es lo que cuestan las fases 2, 3 y 4.
//!
//!   uso:  vc-preflate-coste [directorio-del-banco]

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false))
            .collect(),
        Err(e) => {
            eprintln!("no puedo leer {dir}: {e}");
            std::process::exit(2);
        }
    };
    casos.sort();
    if casos.is_empty() {
        eprintln!("el banco esta vacio: regeneralo con bench/saca-deflate.py");
        std::process::exit(2);
    }

    let flujos: Vec<Vec<u8>> = casos.iter().filter_map(|p| std::fs::read(p).ok()).collect();
    let bytes: usize = flujos.iter().map(|f| f.len()).sum();
    println!("banco: {} flujos · {} B de DEFLATE", flujos.len(), bytes);
    println!();

    // ── FASE 1 SOLA: nuestro inflate ────────────────────────────────────────────────────────
    let t = Instant::now();
    let mut ok_n = 0usize;
    let mut plano_n = 0usize;
    for f in &flujos {
        if let Ok(p) = vc_core::inflate::descomprime(f) {
            ok_n += 1;
            plano_n += p.len();
        }
    }
    let t_nuestro = t.elapsed().as_secs_f64();

    // ── preflate_whole_deflate_stream: inflar + ESTIMAR los parametros ──────────────────────
    let t = Instant::now();
    let mut pf_ok = 0usize;
    let mut pf_corr = 0usize;
    let mut planos: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();   // (plano, correcciones)
    for f in &flujos {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (res, texto) =
                preflate_rs::preflate_whole_deflate_stream(f, &preflate_rs::PreflateConfig::default())
                    .ok()?;
            if res.compressed_size != f.len() { return None; }
            Some((texto.text().to_vec(), res.corrections))
        }));
        if let Ok(Some((plano, corr))) = r {
            pf_ok += 1;
            pf_corr += corr.len();
            planos.push((plano, corr));
        }
    }
    let t_estimar = t.elapsed().as_secs_f64();

    // ── recreate_whole_deflate_stream: RE-DEFLATE, la verificacion byte a byte ───────────────
    // Nuestro codigo la corre SIEMPRE («paranoia: si no es byte-exacto, NO entra»), asi que su
    // coste esta dentro del 78% y hay que contarlo aparte.
    let t = Instant::now();
    let mut rec_ok = 0usize;
    for (plano, corr) in &planos {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            preflate_rs::recreate_whole_deflate_stream(plano, corr).ok()
        }));
        if let Ok(Some(_)) = r { rec_ok += 1; }
    }
    let t_recrear = t.elapsed().as_secs_f64();
    let t_preflate = t_estimar + t_recrear;

    let mbs = |b: usize, s: f64| (b as f64) / 1_048_576.0 / s;
    println!("  {:<28} {:>9} {:>10} {:>10}", "", "flujos ok", "seg", "MB/s");
    println!("  {:<28} {:>9} {:>10.3} {:>10.1}", "vc-inflate (inflar, NUESTRO)", ok_n, t_nuestro,
             mbs(bytes, t_nuestro));
    println!("  {:<28} {:>9} {:>10.3} {:>10.1}", "preflate: inflar + estimar", pf_ok, t_estimar,
             mbs(bytes, t_estimar));
    println!("  {:<28} {:>9} {:>10.3} {:>10.1}", "preflate: re-deflate (verificar)", rec_ok,
             t_recrear, mbs(bytes, t_recrear));
    println!("  {:<28} {:>9} {:>10.3} {:>10.1}", "TOTAL que pagamos hoy", pf_ok, t_preflate,
             mbs(bytes, t_preflate));
    println!();
    println!("  plano: {} B · correcciones: {} B ({:.2}% del DEFLATE)", plano_n, pf_corr,
             100.0 * pf_corr as f64 / bytes as f64);
    println!();

    if t_nuestro > 0.0 && t_preflate > 0.0 {
        let p_inflar = 100.0 * t_nuestro / t_preflate;
        let p_estimar = 100.0 * (t_estimar - t_nuestro).max(0.0) / t_preflate;
        let p_verif = 100.0 * t_recrear / t_preflate;
        println!("  REPARTO DEL 78% DEL RELOJ DEL ARCHIVADOR:");
        println!("    inflar                 {:5.1}%   YA ES NUESTRO ({:.0} MB/s)", p_inflar,
                 mbs(bytes, t_nuestro));
        println!("    estimar parametros     {:5.1}%   la parte dificil de construir", p_estimar);
        println!("    re-deflate y verificar {:5.1}%   se puede hacer con un deflate propio", p_verif);
        println!();
        if p_inflar + p_verif > 60.0 {
            println!("    -> Mas de la mitad esta en INFLAR y RE-DEFLATE, que son piezas de formato");
            println!("       conocido. La primera ya la tenemos. El proyecto es abordable.");
        } else {
            println!("    -> El grueso es ESTIMAR los parametros del codificador original. Tener");
            println!("       el inflate no acorta el camino: el proyecto es ese otro, y es grande.");
        }
    }
    if pf_ok != ok_n {
        println!();
        println!("  OJO: preflate acepta {} flujos y nosotros {}. No son el mismo conjunto,",
                 pf_ok, ok_n);
        println!("  asi que los tiempos no son estrictamente comparables. Mirar antes de concluir.");
    }
}

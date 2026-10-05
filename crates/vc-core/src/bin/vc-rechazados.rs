// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿QUE NOS CUESTA HOY LO QUE PREFLATE NO SABE HACER?
//!
//! LA PREGUNTA DEL DUEÑO: teniendo nuestro propio preflate, ¿seria mas flexible de integrar en
//! vcompresor, mas efectivo en todos los sentidos? La parte de «mas efectivo» que NO depende de
//! ganar el pulso de los bytes es esta: la COBERTURA.
//!
//! preflate-rs revienta con un panic en 171 de los 1.973 flujos del banco. El producto lo
//! contiene -`preflate_seguro` lo envuelve en catch_unwind y silencia la traza-, asi que no se
//! cae: el flujo se va por el camino normal y se guarda el DEFLATE TAL CUAL. Y ahi esta el
//! coste, porque un DEFLATE ya comprimido no se deja comprimir otra vez ni deduplica contra
//! nada: dos ficheros con el mismo contenido dentro pero comprimidos con ajustes distintos son,
//! byte a byte, dos cosas sin ningun trozo en comun.
//!
//! Nuestro inflate hace los 1.973. Este programa mide QUE VALE ESO, en bytes, sobre los flujos
//! que preflate rechaza:
//!
//!   · cuantos son y que fraccion del banco pesan
//!   · lo que ocupan hoy: zstd del DEFLATE tal cual, que es lo que hace el producto ahora
//!   · lo que ocuparian por nuestro camino: zstd del PLANO que saca nuestro inflate
//!
//! La diferencia es lo que la cobertura vale POR SI SOLA, sin haber ganado todavia el pulso de
//! las correcciones. Y se compara ademas contra los flujos que preflate SI acepta, para saber si
//! los rechazados son un caso raro y pequeño o el sitio donde se pierde de verdad.
//!
//!   uso:  vc-rechazados [directorio-del-banco]

use std::path::PathBuf;
use vc_core::inflate::descomprime;

/// Nivel de zstd con el que se mide. El mismo por los dos caminos: lo que se compara es la
/// ENTRADA que se le da, no el compresor.
const NIVEL: i32 = 19;

fn zstd_len(d: &[u8]) -> usize {
    zstd::bulk::compress(d, NIVEL).map(|v| v.len()).unwrap_or(d.len())
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();

    // Las trazas de un panic ajeno llenarian la pantalla en un banco de 1.973.
    let anterior = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let (mut n_rech, mut n_ok, mut n_nuestro_no) = (0u64, 0u64, 0u64);
    let (mut b_rech, mut b_ok) = (0usize, 0usize);
    // De los rechazados: lo que ocupan hoy y lo que ocuparian por nuestro camino.
    let (mut z_hoy, mut z_nuestro, mut plano_rech) = (0usize, 0usize, 0usize);
    // Y lo mismo para una muestra de los aceptados, para tener con que comparar.
    let (mut z_ok_hoy, mut z_ok_nuestro, mut plano_ok, mut n_muestra) = (0usize, 0usize, 0usize, 0u64);

    for (i, p) in casos.iter().enumerate() {
        let Ok(d) = std::fs::read(p) else { continue };
        // EL MISMO CRITERIO QUE EL PRODUCTO, no uno mas suelto: `preflate_seguro` exige que no
        // haya panic, que el tamaño cuadre Y que el flujo se reconstruya BYTE A BYTE. Un flujo
        // que preflate «procesa» pero no reconstruye igual es un flujo rechazado para nosotros,
        // porque en un archivador no entra nada que no vuelva exacto.
        let acepta = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cfg = preflate_rs::PreflateConfig::default();
            let (res, texto) = preflate_rs::preflate_whole_deflate_stream(&d, &cfg).ok()?;
            if res.compressed_size != d.len() { return None }
            let rec = preflate_rs::recreate_whole_deflate_stream(texto.text(), &res.corrections).ok()?;
            if rec != d { return None }
            Some(())
        })).ok().flatten().is_some();
        let nuestro = descomprime(&d).ok();

        if acepta {
            n_ok += 1; b_ok += d.len();
            // Una muestra de 1 de cada 20 para no pasarse una hora comprimiendo con zstd 19.
            if i % 20 == 0 {
                if let Some(pl) = &nuestro {
                    n_muestra += 1;
                    plano_ok += pl.len();
                    z_ok_hoy += zstd_len(&d);
                    z_ok_nuestro += zstd_len(pl);
                }
            }
        } else {
            n_rech += 1; b_rech += d.len();
            match &nuestro {
                Some(pl) => {
                    plano_rech += pl.len();
                    z_hoy += zstd_len(&d);          // lo que hace el producto HOY con ellos
                    z_nuestro += zstd_len(pl);      // lo que haria por nuestro camino
                }
                None => n_nuestro_no += 1,
            }
        }
    }
    std::panic::set_hook(anterior);

    let total_b = b_rech + b_ok;
    let pcb = |x: usize| 100.0 * x as f64 / total_b.max(1) as f64;
    println!();
    println!("  LO QUE PREFLATE NO SABE HACER — banco de {} flujos, {} B de DEFLATE",
             n_ok + n_rech, total_b);
    println!();
    println!("  {:<34} {:>8} {:>16} {:>9}", "", "flujos", "bytes", "% del banco");
    println!("  {:<34} {:>8} {:>16} {:>8.2}%", "preflate los acepta", n_ok, b_ok, pcb(b_ok));
    println!("  {:<34} {:>8} {:>16} {:>8.2}%", "preflate REVIENTA (panic)", n_rech, b_rech, pcb(b_rech));
    println!("  {:<34} {:>8}", "de esos, los nuestros tampoco", n_nuestro_no);
    println!();
    println!("  LOS RECHAZADOS, EN BYTES GUARDADOS (zstd -{NIVEL} por los dos caminos):");
    println!("    {:<38} {:>14} B", "DEFLATE tal cual, que es lo de HOY", z_hoy);
    println!("    {:<38} {:>14} B  (plano de {} B)", "el plano de nuestro inflate", z_nuestro, plano_rech);
    if z_nuestro < z_hoy {
        println!("    {:<38} {:>14} B  ({:.2}x mejor)", "AHORRO", z_hoy - z_nuestro,
                 z_hoy as f64 / z_nuestro.max(1) as f64);
    } else {
        println!("    {:<38} {:>14} B  el camino de hoy es MEJOR", "no hay ahorro", z_nuestro - z_hoy);
    }
    println!();
    println!("  PARA COMPARAR, una muestra de {n_muestra} de los que preflate SI acepta:");
    println!("    {:<38} {:>14} B", "DEFLATE tal cual", z_ok_hoy);
    println!("    {:<38} {:>14} B  (plano de {} B)", "el plano", z_ok_nuestro, plano_ok);
    if z_ok_nuestro < z_ok_hoy {
        println!("    {:<38} {:>14} B  ({:.2}x mejor)", "AHORRO", z_ok_hoy - z_ok_nuestro,
                 z_ok_hoy as f64 / z_ok_nuestro.max(1) as f64);
    }
    println!();
    println!("  COMO LEER ESTO. Este ahorro NO depende de ganar el pulso de las correcciones:");
    println!("  es lo que vale PODER ABRIR un flujo que el rival no abre. Y hay que restarle lo");
    println!("  que cuesten nuestras propias correcciones sobre esos flujos, que hoy es mucho.");
    println!("  Si el ahorro es grande y los rechazados pesan, la cobertura sola ya paga; si no,");
    println!("  hay que ganar el pulso de los bytes antes de hablar de sustituir nada.");
}

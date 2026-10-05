// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! EL ARNES DE ACEPTACION de la fase 2: inflar y VOLVER A EMITIR, byte a byte.
//!
//! Va aparte de `vc-inflate` a proposito: aquel comprueba que sacamos el mismo PLANO que zlib,
//! y este que devolvemos el mismo FLUJO COMPRIMIDO. Son dos contratos distintos y el segundo es
//! mucho mas duro — el primero admite una sola respuesta correcta y el segundo tambien, pero
//! rodeada de decenas de respuestas «validas» que serian un fallo.
//!
//! POR QUE IMPORTA ESTA CIFRA. El desglose del 2026-07-30 dice que preflate es el 78% del reloj
//! del archivador y que su re-deflate es el 23,9% de ese 78%. Esta es la pieza que lo sustituye.
//!
//! Y por que no basta con que «comprima bien»: un emisor que recomprima por su cuenta produce un
//! DEFLATE valido y distinto. Pasaria cualquier prueba de formato y romperia la ida y vuelta del
//! archivo, que es el unico contrato que tiene este producto. Por eso el emisor no decide nada:
//! replica lo que el inflate capturo.
//!
//!   uso:  vc-redeflate [directorio-del-banco]

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
        eprintln!("banco vacio: regeneralo con bench/saca-deflate.py");
        std::process::exit(2);
    }

    let (mut ok, mut distinto, mut no_infla, mut no_emite) = (0usize, 0usize, 0usize, 0usize);
    let mut bytes = 0usize;
    let mut ejemplos: Vec<String> = Vec::new();
    let t = Instant::now();

    for c in &casos {
        let Ok(comp) = std::fs::read(c) else { continue };
        bytes += comp.len();
        let (_plano, bloques) = match vc_core::inflate::descomprime_con_bloques(&comp) {
            Ok(v) => v,
            Err(e) => {
                no_infla += 1;
                if ejemplos.len() < 5 {
                    ejemplos.push(format!("{}: no infla ({e})", c.display()));
                }
                continue;
            }
        };
        match vc_core::deflate_emit::emite(&bloques) {
            Ok(rehecho) if rehecho == comp => ok += 1,
            Ok(rehecho) => {
                distinto += 1;
                if ejemplos.len() < 5 {
                    // Donde se separan importa mas que cuanto: el primer byte distinto suele
                    // señalar la fase (cabecera, tabla, datos) en la que esta el fallo.
                    let d = comp.iter().zip(rehecho.iter()).position(|(a, b)| a != b);
                    ejemplos.push(format!(
                        "{}: {} B -> {} B, primer byte distinto en {:?}",
                        c.display(), comp.len(), rehecho.len(), d));
                }
            }
            Err(e) => {
                no_emite += 1;
                if ejemplos.len() < 5 {
                    ejemplos.push(format!("{}: no emite ({e})", c.display()));
                }
            }
        }
    }
    let s = t.elapsed().as_secs_f64();

    println!();
    println!("  EL ARNES DEL RE-DEFLATE — {} flujos, {:.1} MiB de DEFLATE en {:.1} s",
             casos.len(), bytes as f64 / 1048576.0, s);
    println!();
    println!("    identicos:      {:6}", ok);
    println!("    distintos:      {:6}", distinto);
    println!("    no inflan:      {:6}", no_infla);
    println!("    no emiten:      {:6}", no_emite);
    println!();
    if !ejemplos.is_empty() {
        println!("  los primeros que fallan:");
        for e in &ejemplos {
            println!("    {e}");
        }
        println!();
    }
    if ok == casos.len() {
        println!("  PASA: los {} vuelven a salir IDENTICOS.", ok);
    } else {
        println!("  NO PASA: {} de {} identicos ({:.1}%). No hay nota parcial: o salen todos",
                 ok, casos.len(), 100.0 * ok as f64 / casos.len() as f64);
        println!("  o esta pieza no puede sustituir a preflate-rs.");
        std::process::exit(1);
    }
}

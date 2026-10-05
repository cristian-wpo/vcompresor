// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿QUE PARTE DEL FORMATO EJERCITA EL BANCO? La cobertura real de nuestro «1.973 de 1.973».
//!
//! El arnes dice que los 1.973 flujos salen identicos. Eso es 100% DEL BANCO, y hasta hoy nadie
//! habia medido que partes del RFC 1951 toca ese banco. Los flujos salen de los PNG de un
//! corpus, o sea que probablemente los escribio libpng con un rango estrecho de ajustes.
//!
//! Si el banco no trae ni un bloque de arboles fijos, ni una longitud 258, ni una distancia
//! cerca de 32.768, esas ramas de nuestro codigo NUNCA se han ejecutado — y el 100% no dice
//! nada de ellas. «Pasa el banco» y «es correcto» son dos afirmaciones distintas, y la segunda
//! solo vale hasta donde llegue la primera.
//!
//! Esto no opina: cuenta. Cada dimension del formato con cuantos flujos la ejercitan y cuantas
//! veces. Lo que salga a CERO es la lista de lo que no sabemos si funciona.
//!
//!   uso:  vc-cobertura [directorio-del-banco]

use std::path::PathBuf;
use vc_core::inflate::{descomprime_con_bloques, Bloque, Token};

#[derive(Default)]
struct Cuenta {
    flujos: usize,
    bl_almacenado: usize,
    bl_fijo: usize,
    bl_dinamico: usize,
    flujos_multibloque: usize,
    max_bloques: usize,
    literales: u64,
    copias: u64,
    // los simbolos de longitud y distancia que se han visto de verdad
    sim_long: [u64; 29],
    sim_dist: [u64; 30],
    long_258_como_284: u64,
    long_258_como_285: u64,
    max_dist: u16,
    min_dist: u16,
    max_long: u16,
    // la cabecera dinamica
    rle_16: u64,
    rle_17: u64,
    rle_18: u64,
    hclen_min: usize,
    hclen_max: usize,
    hdist_1: usize, // arboles de distancia con un solo simbolo: caso raro y famoso
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false))
            .collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();

    let mut c = Cuenta { min_dist: u16::MAX, hclen_min: 99, ..Default::default() };
    for p in &casos {
        let Ok(d) = std::fs::read(p) else { continue };
        let Ok((_plano, bloques)) = descomprime_con_bloques(&d) else { continue };
        c.flujos += 1;
        if bloques.len() > 1 { c.flujos_multibloque += 1; }
        c.max_bloques = c.max_bloques.max(bloques.len());
        for b in &bloques {
            match b {
                Bloque::Almacenado { .. } => c.bl_almacenado += 1,
                Bloque::Comprimido { cabecera, tokens, .. } => {
                    match cabecera {
                        None => c.bl_fijo += 1,
                        Some(h) => {
                            c.bl_dinamico += 1;
                            c.hclen_min = c.hclen_min.min(h.hclen);
                            c.hclen_max = c.hclen_max.max(h.hclen);
                            if h.hdist == 1 { c.hdist_1 += 1; }
                            for &(s, _) in &h.secuencia {
                                match s {
                                    16 => c.rle_16 += 1,
                                    17 => c.rle_17 += 1,
                                    18 => c.rle_18 += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                    for t in tokens {
                        match *t {
                            Token::Lit(_) => c.literales += 1,
                            Token::Copia { long, dist, sim_long, sim_dist } => {
                                c.copias += 1;
                                let i = sim_long as usize - 257;
                                if i < 29 { c.sim_long[i] += 1; }
                                if (sim_dist as usize) < 30 { c.sim_dist[sim_dist as usize] += 1; }
                                if long == 258 {
                                    if sim_long == 285 { c.long_258_como_285 += 1; }
                                    else { c.long_258_como_284 += 1; }
                                }
                                c.max_dist = c.max_dist.max(dist);
                                c.min_dist = c.min_dist.min(dist);
                                c.max_long = c.max_long.max(long);
                            }
                        }
                    }
                }
            }
        }
    }

    println!();
    println!("  COBERTURA DEL BANCO — {} flujos leidos de {}", c.flujos, dir);
    println!();
    println!("  TIPOS DE BLOQUE            flujos/bloques");
    let m = |n: usize| if n == 0 { "  <- NUNCA. Esa rama no se ha ejecutado jamas" } else { "" };
    println!("    almacenado (btype 00)    {:>10}{}", c.bl_almacenado, m(c.bl_almacenado));
    println!("    arboles fijos (01)       {:>10}{}", c.bl_fijo, m(c.bl_fijo));
    println!("    arboles dinamicos (10)   {:>10}{}", c.bl_dinamico, m(c.bl_dinamico));
    println!("    flujos con >1 bloque     {:>10}{}   (maximo visto: {})",
             c.flujos_multibloque, m(c.flujos_multibloque), c.max_bloques);
    println!();
    println!("  SIMBOLOS                    veces");
    println!("    literales                {:>10}", c.literales);
    println!("    copias                   {:>10}", c.copias);
    let sl_vistos = c.sim_long.iter().filter(|&&x| x > 0).count();
    let sd_vistos = c.sim_dist.iter().filter(|&&x| x > 0).count();
    println!("    simbolos de longitud     {:>10} de 29 usados", sl_vistos);
    println!("    simbolos de distancia    {:>10} de 30 usados", sd_vistos);
    if sl_vistos < 29 {
        let faltan: Vec<usize> = (0..29).filter(|&i| c.sim_long[i] == 0).map(|i| i + 257).collect();
        println!("      NUNCA vistos (codigo): {:?}", faltan);
    }
    if sd_vistos < 30 {
        let faltan: Vec<usize> = (0..30).filter(|&i| c.sim_dist[i] == 0).collect();
        println!("      NUNCA vistos (codigo): {:?}", faltan);
    }
    println!();
    println!("  CASOS CON TRAMPA");
    println!("    longitud 258 como 285    {:>10}{}", c.long_258_como_285, m(c.long_258_como_285 as usize));
    println!("    longitud 258 como 284+31 {:>10}{}", c.long_258_como_284, m(c.long_258_como_284 as usize));
    println!("    arbol de distancias de 1 {:>10}{}", c.hdist_1, m(c.hdist_1));
    println!("    distancia minima vista   {:>10}{}", c.min_dist,
             if c.min_dist == 1 { "   (copia solapada: cubierto)" } else { "  <- sin copias solapadas de distancia 1" });
    println!("    distancia maxima vista   {:>10}   (el tope del formato es 32.768)", c.max_dist);
    println!("    longitud maxima vista    {:>10}   (el tope del formato es 258)", c.max_long);
    println!();
    println!("  CABECERA DINAMICA           veces");
    println!("    RLE 16 (repite anterior) {:>10}{}", c.rle_16, m(c.rle_16 as usize));
    println!("    RLE 17 (3-10 ceros)      {:>10}{}", c.rle_17, m(c.rle_17 as usize));
    println!("    RLE 18 (11-138 ceros)    {:>10}{}", c.rle_18, m(c.rle_18 as usize));
    println!("    hclen                    {:>10}..{}", c.hclen_min, c.hclen_max);
    println!();
    println!("  LO QUE SALGA A CERO ARRIBA es la lista de lo que el «1.973 de 1.973» NO cubre.");
    println!("  No dice que este mal: dice que no lo sabemos, que es distinto y peor de ignorar.");
}

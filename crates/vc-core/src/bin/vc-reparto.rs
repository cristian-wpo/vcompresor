// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿HACE FALTA PARALELIZAR DENTRO DE UN FLUJO, O BASTA CON REPARTIR FLUJOS ENTEROS?
//!
//! POR QUE EXISTE ESTA MEDIDA. Se afirmo —sin medir nada— que un flujo DEFLATE no se puede
//! decodificar en paralelo «porque los codigos son de longitud variable y no se puede ni partir
//! el flujo». La segunda mitad es FALSA: existe pugz, un gunzip paralelo publicado, y funciona
//! porque un decodificador de Huffman SE RESINCRONIZA SOLO — arrancando en una posicion de bits
//! cualquiera, converge a la frontera correcta en unas decenas de bits. Lo que si es cierto es
//! que las coincidencias copian de lo ya escrito, y esa dependencia hay que resolverla en una
//! segunda pasada.
//!
//! Asi que la pregunta no es «¿se puede?» sino «¿nos hace falta?», y eso depende de UN dato que
//! nadie habia mirado: como se reparte el trabajo entre los flujos.
//!
//!   Si el trabajo esta repartido entre muchos flujos parecidos, repartir flujos enteros entre
//!   hilos —que es facil, seguro y no toca el decodificador— da todo el paralelismo posible.
//!
//!   Si un puñado de flujos gigantes se lleva la mayor parte, repartir flujos deja hilos
//!   parados esperando al mas grande, y entonces SI hace falta partir por dentro.
//!
//! Se mide el reparto: tamaños, cuanto se lleva el mayor, cuanto los diez mayores, y cuantos
//! bloques trae cada uno (las fronteras de bloque son el sitio natural por donde partir).
//!
//!   uso:  vc-reparto [directorio-del-banco]

use std::path::PathBuf;
use vc_core::inflate::{descomprime_con_bloques, Bloque};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();

    // (bytes de plano que produce, bytes de deflate, bloques) — el plano es el trabajo real
    let mut v: Vec<(usize, usize, usize)> = Vec::new();
    for p in &casos {
        let Ok(d) = std::fs::read(p) else { continue };
        let Ok((plano, bl)) = descomprime_con_bloques(&d) else { continue };
        let comprimidos = bl.iter().filter(|b| matches!(b, Bloque::Comprimido { .. })).count();
        v.push((plano.len(), d.len(), comprimidos.max(1)));
    }
    if v.is_empty() { eprintln!("banco vacio"); std::process::exit(2); }

    let total: usize = v.iter().map(|x| x.0).sum();
    let n = v.len();
    v.sort_by_key(|x| std::cmp::Reverse(x.0));

    println!();
    println!("  EL REPARTO DEL TRABAJO — {} flujos, {:.2} GB de plano en total", n, total as f64 / 1e9);
    println!();
    let pct = |x: usize| 100.0 * x as f64 / total as f64;
    println!("  el flujo MAYOR                {:>12} B de plano   {:5.2}% del total", v[0].0, pct(v[0].0));
    for k in [10usize, 50, 100, 500] {
        if k <= n {
            let s: usize = v[..k].iter().map(|x| x.0).sum();
            println!("  los {k:3} mayores               {:>12} B            {:5.2}%", s, pct(s));
        }
    }
    let mediana = v[n / 2].0;
    println!("  la MEDIANA                    {:>12} B", mediana);
    println!();

    // Lo que decide: repartiendo FLUJOS ENTEROS entre H hilos con el reparto voraz (el mayor
    // primero al hilo mas libre), ¿cuanto tarda el hilo mas cargado? Ese es el limite duro de
    // repartir por flujos, y se compara con el ideal total/H.
    println!("  {:>6} {:>16} {:>16} {:>10}", "hilos", "ideal (total/h)", "el mas cargado", "eficiencia");
    for h in [2usize, 4, 8, 12, 16] {
        let mut carga = vec![0usize; h];
        for &(p, _, _) in &v {
            let i = carga.iter().enumerate().min_by_key(|(_, c)| **c).map(|(i, _)| i).unwrap();
            carga[i] += p;
        }
        let mayor = *carga.iter().max().unwrap();
        let ideal = total / h;
        println!("  {h:>6} {:>16} {:>16} {:>9.1}%", ideal, mayor, 100.0 * ideal as f64 / mayor as f64);
    }
    println!();
    println!("  La eficiencia dice cuanto se aprovecha repartiendo FLUJOS ENTEROS, sin tocar el");
    println!("  decodificador. Si esta cerca del 100%, partir por dentro de un flujo —que es");
    println!("  dificil y arriesgado— no aporta nada y la respuesta es repartir flujos.");
    println!();

    let bloques: usize = v.iter().map(|x| x.2).sum();
    let multi = v.iter().filter(|x| x.2 > 1).count();
    println!("  Y si algun dia hiciera falta partir por dentro, las fronteras naturales son los");
    println!("  bloques: {} en total, {:.2} por flujo, y {} flujos ({:.1}%) traen mas de uno.",
             bloques, bloques as f64 / n as f64, multi, 100.0 * multi as f64 / n as f64);
    println!("  El mayor de todos trae {} bloques.", v.iter().map(|x| x.2).max().unwrap());
}

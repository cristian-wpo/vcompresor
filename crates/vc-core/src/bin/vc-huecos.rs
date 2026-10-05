// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! BUSCAR UN HUECO: ¿se puede comprimir MAS gastando MENOS? Barrido de parametros de zstd.
//!
//! DE DONDE SALE LA HIPOTESIS, y no es una corazonada: son dos medidas de hoy.
//!
//!   · buscar coincidencias es >=99% del tiempo de zstd a nivel 19 (barrido de estrategias)
//!   · zstd gana el 30,37% del tamaño por mirar mas alla de los 32 KB de DEFLATE, y la mayor
//!     parte de esa ganancia ya esta a 512 KB
//!
//! O sea que pagamos una busqueda carisima, y un tercio de lo que compra es encontrar cosas
//! LEJOS. Pero zstd tiene un buscador de largo alcance APARTE —el LDM, «long distance
//! matching»— que es groseramente mas barato: una tabla hash gorda con paso largo, pensada para
//! encontrar repeticiones a megabytes de distancia sin recorrer cadenas. No lo usamos.
//!
//! LA PREGUNTA CONCRETA: ¿una estrategia BARATA con LDM encendido bate a una estrategia CARA sin
//! LDM, en bytes Y en reloj a la vez? Si la respuesta es si, ahi hay un hueco.
//!
//! ================ POR QUE ESTO SI SE PUEDE PARALELIZAR ================
//!
//! La norma de la casa es que las medidas de tiempo van EN SERIE, porque dos relojes a la vez se
//! estorban y se falsean. Pero este barrido mide BYTES en su primera fase, y los bytes que saca
//! un compresor de una entrada dada son los mismos por cargada que este la maquina — es el mismo
//! razonamiento por el que `sonda_niveles` se paralelizo, y alli se verifico dando el mismo sha
//! de archivo en dos ejecuciones.
//!
//! Asi que: FASE 1 en paralelo y solo bytes. FASE 2 en serie, cronometrando SOLO a los
//! finalistas. Nunca las dos cosas a la vez.
//!
//!   uso:  vc-huecos <fichero-plano> [hilos]

use std::time::Instant;
use zstd::zstd_safe::CParameter as P;
use zstd::zstd_safe::zstd_sys::ZSTD_strategy::*;

/// Un ajuste a probar: un nombre y lo que le cambia a zstd.
struct Ajuste { nombre: &'static str, nivel: i32, params: Vec<P> }

fn rejilla() -> Vec<Ajuste> {
    let mut v = Vec::new();
    let mut add = |n: &'static str, niv: i32, p: Vec<P>| v.push(Ajuste { nombre: n, nivel: niv, params: p });

    // LA REFERENCIA: lo que hace el producto hoy.
    add("-19 (lo de hoy)", 19, vec![]);
    add("-9", 9, vec![]);
    add("-12", 12, vec![]);

    // VENTANA MAS GRANDE, sin tocar nada mas. Hoy -19 usa 8 MB; la muestra son 128 MB, asi que
    // con 27 la ventana cubre la muestra entera y todas las repeticiones son alcanzables.
    add("-19 ventana 64 MB", 19, vec![P::WindowLog(26)]);
    add("-19 ventana 128 MB", 19, vec![P::WindowLog(27)]);

    // EL CANDIDATO: LDM encendido. Busca lejos con una tabla aparte en vez de con la cadena.
    add("-19 + LDM", 19, vec![P::EnableLongDistanceMatching(true), P::WindowLog(27)]);
    add("-12 + LDM", 12, vec![P::EnableLongDistanceMatching(true), P::WindowLog(27)]);
    add("-9 + LDM", 9, vec![P::EnableLongDistanceMatching(true), P::WindowLog(27)]);
    add("-6 + LDM", 6, vec![P::EnableLongDistanceMatching(true), P::WindowLog(27)]);

    // EL HUECO QUE SE BUSCA: estrategia BARATA con LDM. Si esto bate a -19 en bytes y en reloj,
    // estamos pagando una busqueda cara para encontrar lo que una tabla gorda encuentra gratis.
    add("lazy2 + LDM", 19, vec![P::Strategy(ZSTD_lazy2),
                                P::EnableLongDistanceMatching(true), P::WindowLog(27)]);
    add("btlazy2 + LDM", 19, vec![P::Strategy(ZSTD_btlazy2),
                                  P::EnableLongDistanceMatching(true), P::WindowLog(27)]);
    add("greedy + LDM", 19, vec![P::Strategy(ZSTD_greedy),
                                 P::EnableLongDistanceMatching(true), P::WindowLog(27)]);

    // AFINAR EL LDM: cuanto de fino hila la tabla larga. Mas hash y menos minimo = encuentra
    // mas, cuesta mas memoria.
    add("-19 + LDM fino", 19, vec![P::EnableLongDistanceMatching(true), P::WindowLog(27),
                                   P::LdmHashLog(24), P::LdmMinMatch(32)]);
    add("lazy2 + LDM fino", 19, vec![P::Strategy(ZSTD_lazy2),
                                     P::EnableLongDistanceMatching(true), P::WindowLog(27),
                                     P::LdmHashLog(24), P::LdmMinMatch(32)]);

    // Y EL EXTREMO, para saber donde esta el techo aunque no sea usable.
    add("-22 ventana 128 MB", 22, vec![P::WindowLog(27)]);
    add("-22 + LDM fino", 22, vec![P::EnableLongDistanceMatching(true), P::WindowLog(27),
                                   P::LdmHashLog(24), P::LdmMinMatch(32)]);
    v
}

fn comprime(d: &[u8], a: &Ajuste) -> Option<usize> {
    let mut c = zstd::bulk::Compressor::new(a.nivel).ok()?;
    for p in &a.params { c.set_parameter(*p).ok()?; }
    c.compress(d).ok().map(|v| v.len())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ruta = args.get(1).cloned().unwrap_or_else(|| "/tmp/plano.bin".into());
    // Pocos hilos A PROPOSITO: con ventana de 128 MB cada compresor pide mas de un giga de
    // tablas. Diez a la vez se comen la maquina y el barrido acabaria midiendo el intercambio.
    let hilos: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
    let d = std::fs::read(&ruta).expect("no puedo leer el plano");
    let rej = rejilla();

    println!();
    println!("  BUSCANDO UN HUECO — {} B de plano REAL, {} ajustes", d.len(), rej.len());
    println!();
    println!("  FASE 1: solo BYTES, en paralelo ({hilos} a la vez). Se puede porque los bytes que");
    println!("  saca un compresor de una entrada dada NO dependen de la carga de la maquina.");
    println!();

    let dd = &d;
    let mut bytes: Vec<Option<usize>> = vec![None; rej.len()];
    let t0 = Instant::now();
    std::thread::scope(|s| {
        let mut pend: Vec<(usize, &Ajuste)> = rej.iter().enumerate().collect();
        while !pend.is_empty() {
            let lote: Vec<(usize, &Ajuste)> = pend.drain(..hilos.min(pend.len())).collect();
            let hs: Vec<_> = lote.into_iter()
                .map(|(i, a)| (i, s.spawn(move || comprime(dd, a))))
                .collect();
            for (i, h) in hs { bytes[i] = h.join().ok().flatten(); }
        }
    });
    println!("  ({:.0} s de reloj para la fase 1; no significa nada, iba en paralelo)",
             t0.elapsed().as_secs_f64());
    println!();

    let base = bytes[0].unwrap_or(d.len());
    let mut orden: Vec<usize> = (0..rej.len()).collect();
    orden.sort_by_key(|&i| bytes[i].unwrap_or(usize::MAX));
    println!("  {:<24}{:>14}{:>10}{:>16}", "ajuste", "bytes", "% entrada", "vs lo de hoy");
    println!("  {}", "-".repeat(64));
    for &i in &orden {
        let Some(b) = bytes[i] else {
            println!("  {:<24}{:>14}", rej[i].nombre, "FALLO"); continue };
        println!("  {:<24}{:>14}{:>9.3}%{:>15.2}%", rej[i].nombre, b,
                 100.0 * b as f64 / d.len() as f64, 100.0 * (b as f64 - base as f64) / base as f64);
    }

    // ── FASE 2: EL RELOJ, EN SERIE Y SOLO A LOS FINALISTAS ──────────────────────────────────
    //
    // Aqui SI se mide tiempo, asi que se hace de uno en uno. Se cronometran la referencia y los
    // cuatro que mejor comprimieron: lo que se busca es un ajuste que gane EN LAS DOS COSAS, y
    // eso solo se ve con los dos numeros al lado.
    println!();
    println!("  FASE 2: el RELOJ, EN SERIE y solo a los finalistas. Aqui si se mide tiempo.");
    println!();
    // SE CRONOMETRA TODO LO QUE GANE EN BYTES, no los cuatro primeros.
    //
    // La primera version cogia el top 4 por tamaño, y con eso se dejaba fuera justo a los
    // candidatos que buscamos: en el corpus del Mac, `-6 + LDM` comprime un 5,22% MEJOR que -19
    // usando una estrategia baratisima, pero como no estaba entre los cuatro mas pequeños no se
    // llego a cronometrar. El hueco es «mejor en bytes Y en reloj»: filtrar por bytes antes de
    // mirar el reloj puede tirar exactamente lo que se estaba buscando.
    let mut fin: Vec<usize> = orden.iter().copied()
        .filter(|&i| bytes[i].map(|b| b < base).unwrap_or(false)).collect();
    if !fin.contains(&0) { fin.push(0); }
    println!("  {:<24}{:>14}{:>10}{:>12}{:>16}", "ajuste", "bytes", "seg", "MB/s", "vs hoy (reloj)");
    println!("  {}", "-".repeat(76));
    let mut t_base = 0.0f64;
    // La referencia primero, para tener contra que comparar.
    let mut ord2 = vec![0usize];
    ord2.extend(fin.iter().copied().filter(|&i| i != 0));
    for i in ord2 {
        let t = Instant::now();
        let b = comprime(&d, &rej[i]);
        let sg = t.elapsed().as_secs_f64();
        if i == 0 { t_base = sg }
        let Some(b) = b else { continue };
        println!("  {:<24}{:>14}{:>10.3}{:>12.1}{:>15.2}x", rej[i].nombre, b, sg,
                 d.len() as f64 / 1e6 / sg, sg / t_base.max(1e-9));
    }
    println!();
    println!("  EL HUECO, SI LO HAY, es un ajuste que aparezca ARRIBA en la tabla de bytes Y con");
    println!("  un «vs hoy» menor que 1,00x en la del reloj. Comprimir mas gastando menos. Si no");
    println!("  aparece ninguno, eso tambien es un resultado y se escribe igual.");
}

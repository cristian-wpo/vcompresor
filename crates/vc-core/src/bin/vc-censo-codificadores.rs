// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿DE QUE CODIFICADORES VIENE NUESTRO BANCO DE DEFLATE? El censo, no la inferencia.
//!
//! POR QUE EXISTE. Nuestra fase 3 estima «cual de los NUEVE NIVELES DE zlib» produjo el flujo y
//! acierta el 82,10% de los simbolos. El grueso de lo que falla es «otra longitud» (9,1%), y la
//! conclusion que escribimos en `bench/vdeflate.tsv` fue: «el codificador original no elegia la
//! coincidencia mas larga con estos parametros: el banco tiene flujos de zopfli, libdeflate, Go
//! o Java, y los nueve niveles de zlib no los alcanzan».
//!
//! ESO ERA UNA INFERENCIA. Razonable, pero nadie la habia medido — y decidia el tamaño del
//! proyecto: si el banco es zlib casi entero, la fase 3 hay que AFINARLA; si esta repartido
//! entre codificadores, hay que REESCRIBIRLA con otra forma.
//!
//! Y la forma que hace falta la teniamos DENTRO DEL REPO desde el 31-07 sin abrirla: preflate no
//! estima un nivel. Enumera los ALGORITMOS DE HASH REALES —zlib con su desplazamiento y su
//! mascara exactos, el rapido de miniz, libdeflate con y sin hash secundario, zlib-ng, crc32c— y
//! prueba CUAL DE ELLOS EXPLICA el flujo observado, quedandose con el que necesita recorrer la
//! cadena menos veces. Por eso sus correcciones cuestan 1,700% y nuestra marca sola 4,618%: no
//! corrigen contra un deflate generico, IDENTIFICAN el codificador y codifican las desviaciones.
//!
//! Esto le pregunta a SU estimador, flujo a flujo, y cuenta. Es la medida barata que decide.
//!
//!   uso:  vc-censo-codificadores [directorio-del-banco]     (por defecto /tmp/deflate-banco)

use std::collections::BTreeMap;
use std::path::PathBuf;

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
    if casos.is_empty() {
        eprintln!("el banco esta vacio: regeneralo con bench/saca-deflate.py");
        std::process::exit(2);
    }

    // El panico de preflate se traga aparte: revienta en el 8,7% de los flujos reales y eso ya
    // esta medido. Aqui interesa el censo de los que SI clasifica, y cuantos no.
    std::panic::set_hook(Box::new(|_| {}));

    // --por-flujo FICHERO: el censo linea a linea, para cruzarlo con vc-fase3 --por-flujo y
    // contestar si NUESTRO acierto se hunde justo en los flujos que NO son zlib.
    let por_flujo: Option<String> = std::env::args().collect::<Vec<_>>()
        .windows(2).find(|w| w[0] == "--por-flujo").map(|w| w[1].clone());
    let mut lineas: Vec<String> = Vec::new();

    let mut hash: BTreeMap<String, usize> = BTreeMap::new();
    let mut estrategia: BTreeMap<String, usize> = BTreeMap::new();
    let mut politica: BTreeMap<String, usize> = BTreeMap::new();
    let mut cadena: BTreeMap<String, usize> = BTreeMap::new();
    let mut compat = 0usize;
    let mut ok = 0usize;
    let mut fallo = 0usize;
    let mut bytes_ok = 0usize;
    let mut bytes_total = 0usize;

    for p in &casos {
        let Ok(f) = std::fs::read(p) else { continue };
        bytes_total += f.len();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            preflate_rs::parametros_estimados(&f)
        }));
        match r {
            Ok(Ok(par)) => {
                ok += 1;
                bytes_ok += f.len();
                if par.zlib_compatible { compat += 1 }
                *hash.entry(format!("{:?}", par.hash_algorithm)).or_default() += 1;
                *estrategia.entry(format!("{:?}", par.matching_type)).or_default() += 1;
                // La politica se AGRUPA: `AddFirst(n)` sale con ~150 valores distintos de n, casi
                // todos con una o dos apariciones. Ese abanico no es informacion, es que el
                // estimador no converge y devuelve la longitud maxima que vio. Lo que importa es
                // la FAMILIA -¿mete todo, o solo el primer trozo?-, no el n exacto.
                let pol = match par.add_policy {
                    preflate_rs::DictionaryAddPolicy::AddAll => "AddAll (mete todo)".to_string(),
                    preflate_rs::DictionaryAddPolicy::AddFirst(n) if n <= 6 =>
                        format!("AddFirst({n}) — modo rapido de zlib"),
                    preflate_rs::DictionaryAddPolicy::AddFirst(_) =>
                        "AddFirst(n) con n>6 — COLA SIN CONVERGER".to_string(),
                    preflate_rs::DictionaryAddPolicy::AddFirstAndLast(_) =>
                        "AddFirstAndLast(n) — COLA SIN CONVERGER".to_string(),
                    o => format!("{o:?}"),
                };
                *politica.entry(pol).or_default() += 1;
                // la cadena en tramos, que el valor exacto no dice nada y el orden de magnitud si
                let c = par.max_chain;
                let tramo = if c <= 4 { "1-4 (rapido)" }
                    else if c <= 32 { "5-32" }
                    else if c <= 128 { "33-128" }
                    else if c <= 1024 { "129-1024" }
                    else { ">1024 (maxima)" };
                *cadena.entry(tramo.to_string()).or_default() += 1;
                if por_flujo.is_some() {
                    lineas.push(format!("{}\t{:?}\t{}\t{:?}\t{}\t{}\t{:?}\t{}",
                        p.file_name().and_then(|x| x.to_str()).unwrap_or("?"),
                        par.hash_algorithm, par.zlib_compatible, par.matching_type, par.max_chain,
                        par.nice_length, par.add_policy, par.min_len)
                        .replace(' ', ""));
                }
            }
            _ => {
                fallo += 1;
                if por_flujo.is_some() {
                    lineas.push(format!("{}\tSIN-CLASIFICAR\t-\t-\t-\t-\t-\t-",
                        p.file_name().and_then(|x| x.to_str()).unwrap_or("?")));
                }
            }
        }
    }
    if let Some(f) = &por_flujo {
        let mut t = String::from(
            "fichero\thash\tzlib_compat\testrategia\tmax_chain\tnice_length\tadd_policy\tmin_len\n");
        for l in &lineas { t.push_str(l); t.push('\n'); }
        let _ = std::fs::write(f, t);
        eprintln!("  por-flujo: {} lineas en {}", lineas.len(), f);
    }

    let tot = ok + fallo;
    println!("\n  CENSO DE CODIFICADORES · {} flujos · {} B de DEFLATE\n", tot, sep(bytes_total));
    println!("  clasificados {} ({:.1}%)  ·  su estimador falla o revienta en {} ({:.1}%)",
             ok, 100.0 * ok as f64 / tot as f64, fallo, 100.0 * fallo as f64 / tot as f64);
    println!("  compatibles con zlib: {} de {} ({:.1}% de los clasificados)",
             compat, ok, 100.0 * compat as f64 / ok.max(1) as f64);
    println!("  bytes clasificados: {} de {}\n", sep(bytes_ok), sep(bytes_total));

    tabla("ALGORITMO DE HASH — o sea, QUE CODIFICADOR lo produjo", &hash, ok);
    tabla("ESTRATEGIA — voraz o perezoso, con sus umbrales", &estrategia, ok);
    tabla("QUE SE METE EN EL DICCIONARIO (add policy)", &politica, ok);
    tabla("PROFUNDIDAD DE CADENA que hizo falta", &cadena, ok);

    println!("  COMO SE LEE ESTO, y es la pregunta que decide la fase 3:");
    println!("  si el hash sale casi todo `Zlib{{..}}` con pocas variantes, nuestra fase 3 —que");
    println!("  estima los nueve niveles de zlib— tiene la forma correcta y hay que AFINARLA.");
    println!("  Si sale repartido entre miniz, libdeflate y zlib-ng, la forma esta MAL: no es");
    println!("  «que nivel de zlib» sino «que codificador», y eso se reescribe, no se afina.");
    println!();
}

fn tabla(titulo: &str, m: &BTreeMap<String, usize>, tot: usize) {
    println!("  {}", titulo);
    println!("  {}", "-".repeat(76));
    let mut v: Vec<(&String, &usize)> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (k, n) in v {
        let pct = 100.0 * *n as f64 / tot.max(1) as f64;
        let barra = "#".repeat(((pct / 2.0) as usize).min(40));
        println!("    {:>6}  {:>5.1}%  {:<41} {}", n, pct, recorta(k, 41), barra);
    }
    println!();
}

fn recorta(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { format!("{}…", &s[..n - 1]) }
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

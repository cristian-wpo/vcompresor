// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! EL BANCO DE vlz. La ida y vuelta PRIMERO; el ratio solo si pasa.
//!
//! El orden no es una formalidad: lo dice el propio `vlz/src/lib.rs` y tiene razón — «hasta que
//! la vuelta sea exacta sobre todo lo que le echemos, ningún número de tamaño significa nada».
//! Un códec que comprime mucho y no devuelve los bytes no comprime: pierde.
//!
//! Por eso esto ABORTA en cuanto una ida y vuelta falla, y no imprime ni un tamaño de esa
//! muestra. Si se imprimieran juntos, el primero que mira la tabla se queda con el número bonito.
//!
//! uso:  vlz-banco <fichero-o-directorio> [esfuerzos separados por coma] [-o SALIDA]
//!
//! CON `-o SALIDA` escribe el blob comprimido a disco y exige UN SOLO esfuerzo. Existe para que
//! `bench/mide.sh` —la bascula— pueda PESARLO: mide.sh sniffa el `-o` del comando y de ahi saca
//! `bytes_out`. Sin esto, las medidas de vlz vivian solo en los mensajes de commit y la linea de
//! tiempo del panel salia plana en la parte de vlz. La causa no era «el Mac no tiene bascula»:
//! era que vlz nunca pasaba por mide.sh.
//!
//! Un solo esfuerzo por medida, y no es capricho: una linea del banco con tres esfuerzos dentro
//! no es una medida, son tres pegadas, y el panel no puede compararla contra nada.
//!
//! Contra zstd se compara FUERA, con el binario de zstd sobre los mismos ficheros: meter zstd
//! como dependencia de vlz seria justo lo contrario de para lo que existe vlz.

use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("uso: vlz-banco <fichero-o-dir> [esfuerzos, p.ej. 1,3,6]");
        std::process::exit(2);
    }
    // -o SALIDA: escribe el blob para que la bascula lo pese
    let salida: Option<String> = a.iter().position(|x| x == "-o").and_then(|i| a.get(i + 1).cloned());
    let esfuerzos: Vec<usize> = a
        .get(2)
        .filter(|x| *x != "-o")
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![1, 3, 6]);
    if salida.is_some() && esfuerzos.len() != 1 {
        eprintln!("con -o hace falta UN SOLO esfuerzo: una medida con tres dentro no es una medida");
        std::process::exit(2);
    }

    // Los ficheros, en orden estable: la misma entrada tiene que dar la misma tabla.
    let p = std::path::Path::new(&a[1]);
    let mut ficheros: Vec<std::path::PathBuf> = Vec::new();
    if p.is_dir() {
        let mut pila = vec![p.to_path_buf()];
        while let Some(d) = pila.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            let mut v: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            v.sort();
            for q in v {
                if q.is_dir() { pila.push(q) } else { ficheros.push(q) }
            }
        }
    } else {
        ficheros.push(p.to_path_buf());
    }
    ficheros.sort();
    if ficheros.is_empty() {
        eprintln!("no hay ficheros en {}", a[1]);
        std::process::exit(3);
    }

    // ══════════════════════════════════════════════════════════════════════════════════════
    // SE CONCATENA, y hay que decirlo alto porque cambia los numeros a la mitad.
    //
    // Es lo que hace el producto: el archivador mete hasta 64 MiB de ficheros en un frame y los
    // comprime juntos. Medir fichero a fichero mediria sobre todo el arranque en frio del modelo.
    //
    // CUANTO CAMBIA, medido el 01-08 (esfuerzo 64, los mismos ficheros):
    //     tipo       uno a uno    concatenado
    //     codigo    10.493.824      5.196.532    -50,5%
    //     logs       1.908.197      1.008.737    -47,1%
    //     csvjson    3.649.809      1.826.984    -49,9%
    // JUNTAR LOS FICHEROS VALE LA MITAD DEL ARCHIVO. Con 6.791 ficheros de codigo a 4,7 KB de
    // media, cada uno arrancaba el modelo desde cero. Es la juntura que el censo tenia apuntada
    // sin medir —«un tramo de vc-cm -> el siguiente: EL MODELO YA APRENDIDO se tira»— y aqui sale
    // el numero: ~50%. Diez veces mas que cualquier perilla del codec.
    //
    // ⚠️ Y POR ESO HAY QUE MIRAR QUE MEJORA CADA CAMBIO EN LOS DOS MONTAJES. La longitud con
    // escape da -4,6% en logs concatenado y -0,7% fichero a fichero: lo que desbloquea son
    // coincidencias LARGAS, y esas viven ENTRE ficheros. El numero concatenado es el bueno para
    // el producto, pero citarlo sin decir el montaje es citar la mitad.
    //
    // ⚠️ Y VAN ORDENADOS POR NOMBRE, o sea agrupados por directorio, que maximiza el parecido
    // entre vecinos. El archivador recorre el arbol y hace algo parecido, asi que es
    // representativo — pero no es una muestra repartida y no debe leerse como tal.
    // ══════════════════════════════════════════════════════════════════════════════════════
    let mut datos = Vec::new();
    for f in &ficheros {
        if let Ok(b) = std::fs::read(f) { datos.extend_from_slice(&b) }
    }
    println!("\n  vlz · {} ficheros · {} B\n", ficheros.len(), sep(datos.len()));

    println!("  {:<10} {:>16} {:>9} {:>10} {:>10}  {}", "esfuerzo", "bytes", "ratio", "comp MB/s", "desc MB/s", "ida y vuelta");
    println!("  {}", "-".repeat(78));
    for e in esfuerzos {
        let t0 = Instant::now();
        let c = vlz::comprime(&datos, e);
        let tc = t0.elapsed().as_secs_f64();
        let t1 = Instant::now();
        let d = match vlz::descomprime(&c) {
            Ok(d) => d,
            Err(msg) => {
                println!("  {:<10} {:>16} {:>9} {:>10} {:>10}  ⚠️ NO DESCOMPRIME: {}", e, "—", "—", "—", "—", msg);
                eprintln!("\n  ABORTA: la ida y vuelta ha fallado. No se imprime ningun tamaño de esta muestra:");
                eprintln!("  un ratio de un codec que no devuelve los bytes no es un ratio.");
                std::process::exit(4);
            }
        };
        let td = t1.elapsed().as_secs_f64();
        if d != datos {
            let off = d.iter().zip(datos.iter()).position(|(x, y)| x != y);
            println!("  {:<10} {:>16} {:>9} {:>10} {:>10}  ⚠️ DEVUELVE OTROS BYTES", e, "—", "—", "—", "—");
            eprintln!("\n  ABORTA: primer byte distinto en {:?} (len orig {} dec {})", off, datos.len(), d.len());
            std::process::exit(4);
        }
        // Se escribe DESPUES de la ida y vuelta, nunca antes: un fichero en disco de un flujo
        // que no se ha verificado es una mentira con fecha.
        if let Some(p) = &salida {
            if let Err(e) = std::fs::write(p, &c) {
                eprintln!("no puedo escribir {p}: {e}");
                std::process::exit(5);
            }
        }
        let mb = datos.len() as f64 / 1e6;
        println!("  {:<10} {:>16} {:>8.2}% {:>10.1} {:>10.1}  OK bit a bit",
                 e, sep(c.len()), 100.0 * c.len() as f64 / datos.len().max(1) as f64,
                 mb / tc.max(1e-9), mb / td.max(1e-9));
    }
    println!();
    println!("  Contra zstd se compara FUERA, sobre los mismos bytes concatenados. Meter zstd");
    println!("  aqui como dependencia seria lo contrario de para lo que existe vlz.");
}

/// Los miles con punto, que un número de nueve cifras sin separar no se lee.
fn sep(n: usize) -> String {
    let s = n.to_string();
    let mut r = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 { r.push('.') }
        r.push(c);
    }
    r
}

//! EL REPARTO DEL COSTE DE preflate: CUANTO ES TOKENS, CUANTO SON TABLAS, Y SOBRE QUE FLUJOS.
//!
//! 🔑 LA PREGUNTA. Publicabamos «preflate 1,700%, nosotros 2,218%» como si fueran comparables. Y
//! no lo eran por DOS motivos, no uno:
//!
//!   1. la suya es `corr.len()` de su salida COMPLETA —con las correcciones de las TABLAS DE
//!      HUFFMAN, siete de sus diecisiete canales— y la nuestra cuenta solo tokens;
//!   2. 🚨 y sobre todo: la suya es la media de los flujos que preflate SABE HACER. Los que no
//!      consume no aparecen en su media — y no cuestan 1,7%, cuestan el 100%, porque hay que
//!      guardarlos tal cual.
//!
//! COMO. Su propio derivador —`impl<A, B> PredictionEncoder for (A, B)`, escrito por ellos para
//! verificar dos codificadores— deja colgar un contador al lado del de verdad. Ve las mismas
//! llamadas en el mismo orden y no puede alterar ni un byte de la salida.
//!
//! ⚠️ EL CONTRASTE QUE HACE HONESTO EL REPARTO. El contador no reproduce su aritmetico: cuenta las
//! mismas magnitudes que cobramos nosotros (rachas + exp-Golomb). Asi que el TOTAL modelado se
//! imprime al lado de sus BYTES REALES. Si los dos numeros no se parecen, el reparto no vale y hay
//! que decirlo, no publicarlo.
//!
//! Escribe /tmp/preflate-veredicto.tsv con el veredicto de CADA flujo, para poder cruzarlo con
//! nuestra fase 3 y comparar por fin sobre la misma poblacion.
//!
//!   uso:  vc-preflate-reparto [directorio-del-banco]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Su estimador revienta en una parte de los flujos y el panico es ruido conocido, no un fallo
    // nuestro. Se silencia mientras corre.
    std::panic::set_hook(Box::new(|_| {}));

    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco2".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();

    preflate_rs::reparto_reiniciar();
    let (mut n, mut ok, mut bytes_ok, mut corr_bytes) = (0usize, 0usize, 0usize, 0usize);
    let mut bytes_no = 0usize;
    // Cuantos traen sobras detras del flujo, y cuanto pesan. Se dice en voz alta porque es lo
    // que el filtro viejo confundia con «preflate no sabe».
    let (mut con_sobras, mut bytes_sobra) = (0usize, 0usize);
    // 🚨 POR QUE RECHAZA CADA FLUJO. Sin esto, «preflate cuesta 1,703%» es una media sobre los que
    // SABE HACER y la publicabamos como si fuera sobre el banco entero.
    let mut porque: BTreeMap<String, usize> = BTreeMap::new();
    let mut veredicto: Vec<(String, String, usize, usize)> = Vec::new();
    // SUS correcciones concatenadas, para poder preguntarnos si su CABAC deja bytes en la mesa.
    // Nosotros comprimimos las nuestras por fuera; si las suyas tambien encogen, la comparacion
    // nos favorecia y hay que decirlo.
    let mut salida: Vec<u8> = Vec::new();

    let t0 = std::time::Instant::now();
    for p in &casos {
        let Ok(d) = std::fs::read(p) else { continue };
        n += 1;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match preflate_rs::preflate_whole_deflate_stream(
                    &d, &preflate_rs::PreflateConfig::default()) {
                // 🚨 SE DIVIDE POR LO QUE DE VERDAD CONSUME, no por el tamaño del fichero.
                //
                // La version anterior exigia `compressed_size == d.len()` y descartaba el flujo
                // entero si no cuadraba. Ese filtro venia heredado de vc-preflate-coste.rs y
                // nadie lo habia mirado: dejaba fuera 1.601 de 2.522 flujos —el 63%— que
                // preflate procesa PERFECTAMENTE, solo que el .raw trae sobras del contenedor
                // detras del flujo. O sea que el «1,700% de preflate» que llevamos citando
                // semanas es la media de un TERCIO del banco, elegido por un criterio que no
                // tiene nada que ver con si preflate sabe o no sabe.
                Ok((res, _t)) => Ok((res.corrections.len(), res.compressed_size, res.corrections)),
                Err(e) => Err(format!("{:?}", e.exit_code())),
            }
        }));
        let nom = p.file_name().and_then(|x| x.to_str()).unwrap_or("?").to_string();
        match r {
            Ok(Ok((c, cs, corr))) => {
                salida.extend_from_slice(&corr);
                ok += 1; corr_bytes += c; bytes_ok += cs;
                if cs != d.len() { con_sobras += 1; bytes_sobra += d.len() - cs }
                veredicto.push((nom, "OK".to_string(), c, cs));
            }
            Ok(Err(e)) => {
                *porque.entry(e.clone()).or_insert(0usize) += 1; bytes_no += d.len();
                veredicto.push((nom, e, 0, d.len()));
            }
            Err(_) => {
                *porque.entry("PANICO".to_string()).or_insert(0usize) += 1; bytes_no += d.len();
                veredicto.push((nom, "PANICO".to_string(), 0, d.len()));
            }
        }
    }
    let seg = t0.elapsed().as_secs_f64();
    let _ = std::panic::take_hook();

    if let Ok(mut f) = std::fs::File::create("/tmp/preflate-veredicto.tsv") {
        let _ = writeln!(f, "fichero\tveredicto\tcorrecciones\tdeflate");
        for (a, b, c, e) in &veredicto { let _ = writeln!(f, "{a}\t{b}\t{c}\t{e}"); }
    }

    let _ = std::fs::write("/tmp/preflate-corr.bin", &salida);
    let rep = preflate_rs::reparto_leer();
    let h_hist = |h: &[u64]| -> f64 {
        let t: u64 = h.iter().sum();
        if t == 0 { return 0.0 }
        h.iter().filter(|&&c| c > 0)
         .map(|&c| { let p = c as f64 / t as f64; -(c as f64) * p.log2() }).sum()
    };
    let mantisa = |h: &[u64]| -> f64 {
        h.iter().enumerate().map(|(bl, &c)| c as f64 * (bl.max(1) - 1) as f64).sum()
    };
    let coste = |i: usize| -> f64 {
        h_hist(&rep.bl_racha[i]) + mantisa(&rep.bl_racha[i])
      + h_hist(&rep.bl_valor[i]) + mantisa(&rep.bl_valor[i])
    };
    let pc = |bits: f64| 100.0 * (bits / 8.0) / bytes_ok.max(1) as f64;

    println!();
    println!("  🚨 preflate NO CONSUME {} de {} flujos ({:.1}%), que son {} B ({:.1}% del banco)",
             n - ok, n, 100.0 * (n - ok) as f64 / n.max(1) as f64,
             bytes_no, 100.0 * bytes_no as f64 / (bytes_ok + bytes_no).max(1) as f64);
    println!("     Un flujo que no consume no cuesta 1,7%: cuesta el 100%, guardado tal cual.");
    for (k, v) in &porque { println!("     {v:>6}  {k}"); }
    if con_sobras > 0 {
        println!();
        println!("  {con_sobras} flujos traen SOBRAS detras del DEFLATE ({bytes_sobra} B en total).");
        println!("     preflate los hace bien; solo consume menos bytes de los que ocupa el .raw.");
        println!("     El filtro viejo los daba por perdidos y por eso su media salia de 830.");
    }

    println!();
    println!("  EL REPARTO DEL COSTE DE preflate — {ok} de {n} flujos, {bytes_ok} B de DEFLATE, {seg:.1} s");
    println!();
    println!("  {:<28} {:>12} {:>14} {:>9}", "canal", "rachas", "correcciones", "coste");
    let (mut tot, mut tot_tabla, mut tot_token) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..preflate_rs::NCAT_PF {
        let r: u64 = rep.bl_racha[i].iter().sum();
        let v: u64 = rep.bl_valor[i].iter().sum();
        let c = coste(i);
        tot += c;
        if preflate_rs::es_de_tabla(i) { tot_tabla += c } else { tot_token += c }
        if r == 0 && v == 0 { continue }
        println!("  {:<28} {:>12} {:>14} {:>8.3}%", preflate_rs::nombre_canal(i), r, v, pc(c));
    }
    println!();
    println!("  {:<28} {:>27} {:>8.3}%", "LAS TABLAS DE HUFFMAN", "", pc(tot_tabla));
    println!("  {:<28} {:>27} {:>8.3}%", "TODO LO DEMAS (tokens y bloques)", "", pc(tot_token));
    println!("  {:<28} {:>27} {:>8.3}%", "TOTAL modelado", "", pc(tot));
    println!();
    let real = 100.0 * corr_bytes as f64 / bytes_ok.max(1) as f64;
    println!("  {:<28} {:>27} {:>8.3}%", "SUS BYTES DE VERDAD", "", real);
    let desv = if real > 0.0 { 100.0 * (pc(tot) - real) / real } else { 0.0 };
    println!();
    println!("  ⚠ EL CONTRASTE: el modelo se desvia un {desv:+.1}% de sus bytes reales.");
    if desv.abs() > 25.0 {
        println!("    MAS DE UN 25%: el reparto NO es de fiar y no se publica. El contador no");
        println!("    reproduce su aritmetico adaptativo, y aqui esa diferencia ya pesa demasiado.");
    } else {
        println!("    Por debajo del 25%: el REPARTO (que fraccion son tablas) se sostiene, aunque");
        println!("    el valor absoluto no sea suyo. Es lo que se necesita para comparar canales.");
        println!();
        println!("  🔑 TOKEN A TOKEN, SU CIFRA COMPARABLE CON LA NUESTRA ES {:.3}% — Y SOLO SOBRE",
                 real * tot_token / tot.max(1e-9));
        println!("     LOS {ok} FLUJOS QUE CONSUME. Para compararla hay que medir lo nuestro sobre");
        println!("     ESOS MISMOS flujos: se cruzan /tmp/preflate-veredicto.tsv y /tmp/f32.tsv.");
    }
}

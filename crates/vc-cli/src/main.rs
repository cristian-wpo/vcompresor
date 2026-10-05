// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! CLI de vcompresor. Los argumentos se parsean a mano: son seis subcomandos y unas pocas
//! opciones, no justifican arrastrar clap y sus 17 crates.
//!
//! LA INTERFAZ ESTA EN INGLES Y EL CODIGO EN ESPAÑOL, a proposito. El codigo lo leemos
//! nosotros; el CLI lo lee quien compre la herramienta, y un compresor con las opciones en
//! español no se puede vender fuera. Los nombres en español siguen aceptandose como alias:
//! hay guiones y notas escritos contra ellos y romperlos no aporta nada.

use anyhow::{bail, Result};
use std::path::PathBuf;
use std::time::Instant;

#[cfg(target_os = "linux")]
mod fusefs;

const AYUDA: &str = "\
vcompresor — CAS archiver: CDC dedup + lanes + zstd/store. blake3 round-trip ALWAYS.

USAGE
  vcompresor compress   <input>...    -o <archive.vc> [--level N] [--max] [--exclude PAT]
  vcompresor compress   --stdin       -o <archive.vc> [--name X]
  vcompresor compress   <input>...    -o <archive.vc> --price <EUR/TB-month> [--retention M]
  vcompresor add        <archive.vc>  <input>... [--name X]

  <input> can be repeated: `compress a.mp4 b.mp4 notes/ -o copy.vc` saves exactly
  those three, with paths relative to the folder that holds them all.
  vcompresor extract    <archive.vc>  -o <directory> [--file PATTERN]
  vcompresor list       <archive.vc>  [--file PATTERN]
  vcompresor forget     <archive.vc>  <snapshot>

ENCRYPTION (ChaCha20-Poly1305, key derived with Argon2id)
  vcompresor compress <input> -o <archive.vc> --encrypt --password-file <file>
  vcompresor extract  <archive.vc> -o <dir> --password-file <file>
  The password is NEVER taken as a command-line argument: it would show up in
  `ps` for every user on the machine and stay in the shell history. Use
  --password-file, or the VC_PASSWORD environment variable.
  vcompresor verify     <archive.vc>
  vcompresor info       <archive.vc>
  vcompresor mount      <archive.vc>  <mountpoint>

OPTIONS
  -o, --out <path>     destination (required for compress and extract)
      --level <N>      zstd level (default 19)
      --max            best ratio: the text and other lanes use vc-cm, our own
                       context-mixing codec, instead of zstd. Compresses a lot
                       more and is a lot slower.

  PICK THE SETTING BY WHAT IT COSTS YOU, not by what it compresses:
      --price <EUR>    what one TB-month of storage costs you. Given this,
                       vcompresor probes YOUR data, prices every setting -disk
                       plus the CPU to get there- and uses the cheapest one.
      --retention <M>  how many months you keep the data (default 12). Storage is
                       a monthly rent and CPU is paid once, so how long you keep
                       the data decides which of the two wins.
      --cpu <EUR/h>    what an hour of this machine costs you (default 0.0639,
                       a Hetzner AX41-NVMe). A cloud VM billed by the hour costs
                       far more and that changes the answer.

      --no-jpeg        disable the JPEG->JXL lane: much faster both ways, ~16% worse on .jpg
      --no-group       do NOT reorder files by similarity before packing. Grouping is
                       ON by default: on a quiet machine, with two repetitions, it costs
                       0.78% of cycles -below the bench's own 2.05% noise floor, so not
                       measurable- and buys 0.218% of size, which IS exact. An earlier
                       version of this help claimed 13.3%: that came from comparing a run
                       taken at load 53.8 against one at load 56.3.
      --solid[=MiB]    frame size in MiB (32-256). DEFAULT SINCE 2026-07-31: 64 MiB.
                       Measured end to end on 2.62 GB: 8/2/4 MiB frames leave
                       1,041,108,269 B in 177.9 s using 1,912 MB of RAM; 64 MiB frames
                       with the window matched to them leave 1,001,200,708 B in 173.1 s
                       using 1,694 MB. That is -3.833% of bytes with LESS memory and no
                       extra clock. 64 and not 128 because it was measured: 128 MiB
                       leaves 1,020,496,155 B and 256 MiB leaves 1,020,301,616 -both
                       WORSE than 64- while using 1.48x the RAM. The curve saturates
                       around 32-64 MiB and then gets worse.
      --no-solid       go back to the small frames (8/2/4 MiB by lane). Use it if you
                       pull single files out of big archives often: with the solid
                       default, extracting one file decompresses up to 64 MiB instead
                       of up to 8. `--file` and mount work in BOTH modes; what changes
                       is what they cost.
      --exclude <PAT>  with compress/add: skip what matches. Same patterns as
                       --file, and repeatable: --exclude node_modules --exclude '*.tmp'
      --file <PATTERN> with extract/list: only what matches. An exact path, a
                       directory (everything under it), or a wildcard like '*.log'.
                       Only the frames holding those files are decompressed, so
                       pulling one file out of a huge archive costs seconds. How many
                       seconds depends on the frame size: with the default (64 MiB)
                       it decompresses up to 64 MiB per file; with --no-solid, up to 8.
      --name <X>       name of the snapshot inside the archive
      --stdin          read the input from a pipe instead of a file or directory
      --progress       print progress to stderr even when it is not a terminal.
                       For a GUI or a wrapper reading it from a pipe; off by
                       default so it does not fill logs with repainted bars.
  -h, --help           this help
  -V, --version        version

Spanish names are still accepted as aliases: comprimir, descomprimir, anadir/añadir,
verificar, montar, --nivel, --sin-jpeg, --sin-agrupar, --nombre, --precio, --retencion.
";

/// EL TAMAÑO DE FRAME POR DEFECTO, en MiB.
///
/// ⚠️ LA JUSTIFICACION DE ESTE 64 CAMBIO EL MISMO DIA QUE SE ESCRIBIO. A las 13:48 el barrido
/// decia que 128 y 256 MiB daban MAS bytes que 64 y que «la curva satura en 32-64 y despues
/// empeora». A las 21:16, mismo guion y mismo arbol de 43.298 ficheros, decia lo contrario:
///
///     talla      bytes            vs 64      reloj    vs 64     RSS
///     32 MiB     1.007.488.561   +0,641%    153,4 s  +2,56%   1.891 MB
///     64 MiB     1.001.071.046        —     149,6 s      —    1.910 MB
///     128 MiB      998.993.211   -0,208%    151,4 s  +1,21%   2.410 MB
///     256 MiB      996.133.714   -0,493%    150,8 s  +0,82%   2.731 MB
///
/// La curva se dio la vuelta porque en medio entro LA VENTANA IGUALADA AL FRAME. Antes un frame
/// de 256 MiB no servia de nada: zstd solo miraba 4 MiB hacia atras y el resto del frame le era
/// invisible. Ahora se aprovecha entero. Es el mismo efecto que se llevo el -0,972% del CDC.
///
/// 64 SIGUE SIENDO EL DEFECTO, pero por otro motivo — y el motivo viejo hay que dejar de citarlo:
///   · NO porque mas grande sea peor en bytes. Ya no lo es: 256 MiB da un 0,493% menos.
///   · SI porque ese 0,493% cuesta +43,0% de pico de RAM (1.910 -> 2.731 MB) y CUADRUPLICA lo que
///     vale sacar un fichero suelto: hasta 256 MiB descomprimidos en vez de 64.
///   · Y el reloj NO decide nada aqui: el +0,82% de 256 esta DENTRO del suelo de ruido (2,96%).
///
/// Quien quiera ese 0,493% y tenga la RAM lo pide con `--solid=256`. El defecto sirve a quien no
/// elige, y para ese la RAM y la granularidad pesan mas que medio punto de bytes.
///
/// Pasa a ser el DEFECTO por decision del dueño (31-07), y no por la medida: lo que la medida
/// dice es lo que CUESTA la granularidad fina. Si hace falta o no es una pregunta de producto y
/// la contesta quien lo vende. La respuesta fue «que haya dos opciones»: esta y --no-solid.
const SOLIDO_DEF: usize = 64;

struct Args {
    libres: Vec<String>,
    /// --stdin: la entrada llega por tuberia en vez de ser un fichero o un directorio.
    stdin: bool,
    /// --progress: pintar el avance aunque la salida de error no sea un terminal.
    progreso: bool,
    /// --file X: extraer SOLO lo que case. Ruta exacta, directorio, o comodin (*.log).
    fichero: Option<String>,
    /// --exclude X: saltar lo que case. Se puede repetir.
    excluir: Vec<String>,
    /// --password-file: de donde leer la contrasena. NUNCA como argumento suelto.
    pass_fichero: Option<String>,
    /// --encrypt: crear el archivo cifrado.
    cifrar: bool,
    /// --name X: como se llama la instantanea dentro del archivo.
    nombre: Option<String>,
    out: Option<PathBuf>,
    nivel: i32,
    sin_jpeg: bool,
    sin_agrupar: bool,
    maximo: bool,
    solido: usize,
    /// --price: EUR por TB-mes. Si viene, el nivel se elige por dinero y no por bytes.
    precio: Option<f64>,
    /// --retention: meses que se guarda el dato. Decide si gana el disco o la CPU.
    meses: f64,
    /// --cpu: EUR por hora de la maquina que comprime.
    eur_hora: f64,
}

fn parsear<I: Iterator<Item = String>>(mut it: I) -> Result<Args> {
    let mut a = Args {
        libres: Vec::new(), stdin: false, progreso: false, fichero: None, excluir: Vec::new(), pass_fichero: None, cifrar: false, nombre: None, out: None, nivel: 19,
        sin_jpeg: false, sin_agrupar: false, maximo: false, solido: SOLIDO_DEF,
        precio: None, meses: 12.0, eur_hora: vc_core::EUR_HORA_DEFECTO,
    };
    // Un numero con coma decimal se acepta igual que con punto: quien escribe --price 5,55
    // esta diciendo lo mismo que quien escribe 5.55, y fallar ahi es maltratar al usuario por
    // una convencion tipografica.
    let num = |v: &str, que: &str| -> Result<f64> {
        v.replace(',', ".").parse::<f64>()
            .map_err(|_| anyhow::anyhow!("{que}: «{v}» is not a number"))
    };
    while let Some(t) = it.next() {
        match t.as_str() {
            "--stdin" => a.stdin = true,
            "--progress" => a.progreso = true,
            "--password-file" | "--fichero-contrasena" => {
                a.pass_fichero = Some(it.next().ok_or_else(|| anyhow::anyhow!("--password-file needs a path"))?);
            }
            "--encrypt" | "--cifrar" => a.cifrar = true,
            "--exclude" | "--excluir" => {
                a.excluir.push(it.next().ok_or_else(|| anyhow::anyhow!("--exclude needs a pattern"))?);
            }
            "--file" | "--fichero" => {
                a.fichero = Some(it.next().ok_or_else(|| anyhow::anyhow!("--file needs a pattern"))?);
            }
            "--name" | "--nombre" => {
                a.nombre = Some(it.next().ok_or_else(|| anyhow::anyhow!("--name needs a value"))?);
            }
            "-o" | "--out" => {
                a.out = Some(PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("-o needs a path"))?));
            }
            "--level" | "--nivel" => {
                let v = it.next().ok_or_else(|| anyhow::anyhow!("--level needs a number"))?;
                a.nivel = v.parse().map_err(|_| anyhow::anyhow!("--level: «{v}» is not a number"))?;
            }
            "--price" | "--precio" => {
                let v = it.next().ok_or_else(|| anyhow::anyhow!("--price needs a number"))?;
                a.precio = Some(num(&v, "--price")?);
            }
            "--retention" | "--retencion" => {
                let v = it.next().ok_or_else(|| anyhow::anyhow!("--retention needs a number"))?;
                a.meses = num(&v, "--retention")?;
            }
            "--cpu" => {
                let v = it.next().ok_or_else(|| anyhow::anyhow!("--cpu needs a number"))?;
                a.eur_hora = num(&v, "--cpu")?;
            }
            "--no-jpeg" | "--sin-jpeg" => a.sin_jpeg = true,
            // AGRUPAR VA ENCENDIDO POR DEFECTO. ⚠️ Este comentario decia «cuesta el 13,3%
            // del tiempo y pierde a los cuatro precios», y ese 13,3% ESTA MUERTO: la ayuda de
            // este mismo fichero explica que salia de comparar una pasada a carga 53,8 contra
            // otra a 56,3. Remedido en maquina quieta con dos repeticiones: cuesta el 0,78% de
            // los ciclos —POR DEBAJO del suelo de ruido del banco, 2,05%, o sea no medible— y
            // compra el 0,218% de tamaño, que si es exacto. Dos cifras contradictorias en el
            // mismo fichero durante semanas; la de aqui era la vieja.
            "--group" | "--agrupar" => a.sin_agrupar = false,
            "--no-group" | "--sin-agrupar" => a.sin_agrupar = true,
            "--max" => a.maximo = true,
            // --solid, o --solid=N con N en MiB. Sin valor, 128: el punto donde la ganancia
            // ya es casi toda y la memoria todavia es razonable.
            // Vuelve a los frames pequeños (8/2/4 MiB). Para quien saque ficheros sueltos a
            // menudo: con el defecto solido, extraer uno descomprime hasta 64 MiB en vez de 8.
            // `--file` y el montaje funcionan en los dos modos; lo que cambia es lo que cuesta.
            "--no-solid" | "--sin-solido" => a.solido = 0,
            _ if t == "--solid" || t.starts_with("--solid=") => {
                let n: usize = t.strip_prefix("--solid=").and_then(|v| v.parse().ok()).unwrap_or(128);
                // PERILLA DE BANCO (F6·Q3) · `VC_BANCO_SOLIDO=1` baja el suelo del clamp a 1 MiB.
                // El defecto NO se toca: 32 sigue siendo el minimo que el producto acepta. Hace
                // falta porque la pregunta de F6·Q3 es si la recuperacion aguanta con frames
                // PEQUEÑOS (~11 MiB, unos 12 frames), y por debajo de 32 no habia forma de
                // pedirlo. No marca el archivo: un frame de 11 MiB produce un .vc perfectamente
                // valido y legible: lo que cambia es el troceado, no el codec.
                let suelo = if std::env::var("VC_BANCO_SOLIDO").map(|v| v != "0" && !v.is_empty())
                    .unwrap_or(false) { 1 } else { 32 };
                a.solido = n.clamp(suelo, 256);
            }
            _ if t.starts_with('-') => bail!("unknown option: {t}\n\ntry: vcompresor --help"),
            _ => a.libres.push(t),
        }
    }
    Ok(a)
}

fn mb(b: u64) -> f64 { b as f64 / 1_048_576.0 }

/// Debajo de esto, elegir por dinero no tiene sentido: la diferencia entre el mejor y el peor
/// ajuste son centimos, y la sonda costaria mas que la decision que informa.
const MINIMO_PARA_SONDA: u64 = 64 << 20;

/// Pone en `nivel`/`maximo` el ajuste mas barato para el precio que ha dado el usuario, y
/// enseña la tabla entera. Que se vea la tabla no es adorno: una decision automatica que no se
/// puede auditar es una decision en la que no se puede confiar.
fn elige_por_dinero(a: &mut Args, entradas: &[PathBuf]) -> Result<()> {
    let Some(precio) = a.precio else { return Ok(()) };
    if a.stdin {
        eprintln!("note: --price needs to read the input twice, which a pipe cannot do.");
        eprintln!("      Using --level {} as given.", a.nivel);
        return Ok(());
    }
    let total = vc_core::tamano_entrada(entradas)?;
    if total < MINIMO_PARA_SONDA {
        eprintln!("note: input is {:.1} MiB. Too small for the setting to move real money;",
                  mb(total));
        eprintln!("      using --level {} as given.", a.nivel);
        return Ok(());
    }

    // -1 es --max, que no es un nivel de zstd sino otro codec. Va en la misma lista porque la
    // pregunta del usuario es "que me sale mas barato", y --max es una respuesta posible.
    let candidatos = [1, 3, 5, 9, 12, 15, 19, -1];
    // LA SONDA NO PUEDE COSTAR MAS QUE LA DECISION QUE INFORMA. Con muestra fija de 32 MiB
    // tardaba 55 s en decidir sobre un arbol que al nivel elegido se comprime en 2 s. Ambas
    // muestras van en proporcion a la entrada, con techo: asi la sonda es siempre una fraccion
    // pequena del trabajo, y en los archivos grandes -donde la decision vale dinero de verdad-
    // llega a la muestra completa.
    let tope = (32usize << 20).min((total as usize / 16).max(2 << 20));
    // El CM va aparte y mucho mas corto: vc-cm a un hilo hace ~0,2 MiB/s, veinte veces mas
    // lento que zstd-19, asi que con la misma muestra se comeria el 90% del tiempo de sonda.
    // Por debajo de 256 MiB no se sondea: en un trabajo de dos minutos el ajuste no mueve
    // dinero de verdad y no compensa hacer esperar a nadie.
    let tope_cm = if total < (256 << 20) { 0 }
                  else { tope.min((total as usize / 128).max(1 << 20)) };
    let t = Instant::now();
    let sonda = vc_core::sonda_niveles(entradas, &candidatos, tope, tope_cm)?;
    let cands = vc_core::evalua_dinero(&sonda, total, precio, a.meses, a.eur_hora);

    let mejor = cands.iter().enumerate()
        .min_by(|(_, x), (_, y)| x.total().partial_cmp(&y.total()).unwrap())
        .map(|(i, _)| i).unwrap();

    println!("pricing {:.1} MiB at {:.2} EUR/TB-month over {:.0} months, CPU at {:.4} EUR/h",
             mb(total), precio, a.meses, a.eur_hora);
    println!("probed {:.1} MiB of your data in {:.1}s, spread across the whole tree",
             mb(tope as u64), t.elapsed().as_secs_f64());
    if tope_cm == 0 {
        println!("(--max not probed: under 256 MiB the setting does not move real money)");
    } else if tope_cm < tope {
        println!("(--max probed on {:.1} MiB of that: it runs at ~0.2 MiB/s single-threaded)",
                 mb(tope_cm as u64));
    }
    // Se muestran los euros del trabajo Y los euros por TB de entrada. Lo primero es lo que
    // paga quien lanza ESTE comando; lo segundo es lo unico comparable, porque no depende del
    // tamaño de la prueba. Sin la segunda columna, una entrada pequena imprime 0,00 en todo y
    // la tabla deja de servir para lo unico que existe: poder auditar la decision.
    let por_tb = 1e12 / total as f64;
    println!();
    println!("  {:<8}{:>14}{:>9}{:>10}{:>11}{:>10}{:>12}",
             "setting", "size", "ratio", "time", "disk EUR", "CPU EUR", "EUR per TB");
    println!("  {}", "-".repeat(74));
    for (i, c) in cands.iter().enumerate() {
        let etq = if c.nivel < 0 { "--max".to_string() } else { format!("-{}", c.nivel) };
        println!("  {:<8}{:>14}{:>8.2}%{:>9.0}s{:>11.4}{:>10.4}{:>12.2}{}",
                 etq, c.bytes, 100.0 * c.bytes as f64 / total as f64, c.segundos,
                 c.eur_disco, c.eur_cpu, c.total() * por_tb,
                 if i == mejor { "  <-- cheapest" } else { "" });
    }
    let m = &cands[mejor];
    let peor = cands.iter().map(|c| c.total()).fold(f64::MIN, f64::max);
    println!("\n  cheapest: {:.4} EUR for this run, {:.2} EUR per TB of input",
             m.total(), m.total() * por_tb);
    println!("  saves {:.2} EUR per TB against the most expensive setting on this table",
             (peor - m.total()) * por_tb);
    if m.nivel < 0 {
        println!("  -> using --max: at your price and retention, the ratio is worth its CPU.");
        a.maximo = true;
    } else {
        println!("  -> using --level {}.", m.nivel);
        a.nivel = m.nivel;
        a.maximo = false;
    }
    if tope < (32 << 20) {
        println!("  note: probe sample was under 32 MiB, so --max is UNDERestimated here.");
    }
    println!();

    // ── Y AHORA LOS CARRILES, QUE ES DONDE ESTA EL DINERO ───────────────────────────────────
    //
    // Hasta hoy `--price` elegia el nivel de zstd y nada mas. Medido el 2026-07-31 sobre el
    // arbol de 2,7 GB: entre el nivel 17 y el 19 se mueve un 0,8% del tamaño, mientras que el
    // carril de preflate compra el 7,073% de los bytes y cuesta el 59,2% de la CPU, y el de JXL
    // compra el 3,933% y cuesta el 35,4%. A precio S3 los dos se pagan de sobra; a precio disco
    // y un año de retencion los dos PIERDEN — y estan siempre encendidos, o sea que en disco se
    // estaban tirando ~2.593 EUR/PB-año, el 38% del coste total. Optimizabamos la perilla que no
    // importa mientras la que importa ni se tocaba.
    //
    // La decision se toma con la tuberia REAL sobre ficheros REALES del usuario, no con nuestra
    // tabla: nuestro arbol tiene 12.569 JPEG y el suyo puede no tener ninguno.
    elige_carriles(a, entradas, total, precio, por_tb)?;
    Ok(())
}

/// Apaga los carriles que, al precio y el plazo del usuario, cuestan mas de lo que ahorran.
///
/// SE DECIDEN TRES —preflate, PDF y JXL— y el motivo de que no sean cinco es honesto: los otros
/// tres (e8e9, agrupar y log-ts) tienen un coste POR DEBAJO DEL SUELO DE RUIDO de la maquina,
/// 2,96% medido. Decidir sobre ellos seria publicar ruido como si fuera una decision.
///
/// El PDF entra el 2026-08-02 y su sitio natural es este, no una sentencia global: compra el
/// 16,62% de los bytes de pdfoffice pagando 1,67x de reloj y 5,43x de RAM. Si eso se paga o no
/// depende del precio del disco y del plazo — o sea, del usuario.
fn elige_carriles(a: &mut Args, entradas: &[PathBuf], total: u64, precio: f64, por_tb: f64)
    -> Result<()>
{
    // La muestra de carriles va aparte y mas corta que la de niveles: aqui se corre la tuberia
    // ENTERA tres veces, no zstd sobre un bufer.
    let tope = (8usize << 20).min((total as usize / 64).max(2 << 20));
    let t = Instant::now();
    let s = match vc_core::sonda_carriles(entradas, a.nivel.max(1), tope) {
        Ok(v) => v,
        Err(e) => { println!("note: lane probe failed ({e}); leaving lanes as they are."); return Ok(()) }
    };
    let Some(base) = s.iter().find(|c| c.nombre == "completo") else {
        println!("note: lane probe had no baseline; leaving lanes as they are."); return Ok(());
    };
    if base.entrada == 0 { return Ok(()) }
    let esc = total as f64 / base.entrada as f64;

    println!("probed the pipeline itself on {:.1} MiB of your files in {:.1}s, one lane at a \
time,",
             mb(base.entrada), t.elapsed().as_secs_f64());
    // El valor de un carril DEPENDE del codec que venga detras, asi que se sondea con el nivel
    // ya elegido y se dice. Medido: preflate compra el 12,4% con -19 y el 4,9% con un nivel
    // bajo. No es ruido; es que comprimir mejor despues cambia lo que aporta transformar antes.
    println!("at -{} (the level just chosen: a lane is worth a different amount at each level)",
             a.nivel);
    println!();
    println!("  {:<10}{:>14}{:>12}{:>13}{:>13}{:>10}",
             "lane", "buys", "costs CPU", "disk EUR", "CPU EUR", "verdict");
    println!("  {}", "-".repeat(72));
    let mut apaga_preflate = false;
    let mut apaga_jxl = false;
    let mut apaga_pdf = false;
    let mut inertes: Vec<&str> = Vec::new();
    for c in s.iter().filter(|c| c.nombre != "completo") {
        // Lo que COMPRA el carril: bytes que no se guardan. Se alquilan todos los meses.
        let compra = (c.bytes as i64 - base.bytes as i64) as f64 * esc;
        // Lo que CUESTA: CPU. Se paga una vez.
        let coste_s = (base.segundos - c.segundos) * esc;
        let eur_disco = compra / 1e12 * precio * a.meses;
        let eur_cpu = coste_s / 3600.0 * a.eur_hora;
        let paga = eur_disco > eur_cpu;
        // ¿O es que este carril no toca NADA en estos datos? Los bytes son deterministas —dos
        // vueltas de la misma configuracion dan el mismo archivo, y esta verificado—, asi que
        // un 0,000% es una medida, no ruido: significa que aqui no hay con que trabajar. Un
        // corpus sin JPEG no le da nada que hacer al carril de JPEG. Apagarlo sigue siendo lo
        // correcto, pero el MOTIVO es otro y decirlo mal es mentir con un numero correcto.
        let pct = 100.0 * compra / total.max(1) as f64;
        let inerte = pct.abs() < 0.01;
        println!("  {:<10}{:>13.3}%{:>11.0}s{:>13.4}{:>13.4}{:>10}",
                 c.nombre, pct, coste_s, eur_disco, eur_cpu,
                 if inerte { "no-op" } else if paga { "keep" } else { "OFF" });
        if !paga {
            match c.nombre {
                "preflate" => { if inerte { inertes.push("preflate") } else { apaga_preflate = true } }
                "jxl" => { if inerte { inertes.push("jxl") } else { apaga_jxl = true } }
                "pdf" => { if inerte { inertes.push("pdf") } else { apaga_pdf = true } }
                _ => {}
            }
        }
    }
    println!();
    if apaga_preflate || apaga_jxl || apaga_pdf {
        let mut cual = Vec::new();
        if apaga_preflate { std::env::set_var("VC_SIN_PREFLATE", "1"); cual.push("preflate"); }
        if apaga_jxl { a.sin_jpeg = true; cual.push("jxl"); }
        // El PDF no se apaga si preflate ya se apago: sin preflate no hay carril de PDF que
        // apagar, y decir dos veces la misma decision haria pensar que son dos ahorros.
        if apaga_pdf && !apaga_preflate { std::env::set_var("VC_SIN_PDF", "1"); cual.push("pdf"); }
        println!("  -> turning OFF: {}. At your price and retention they cost more CPU than",
                 cual.join(", "));
        println!("     the disk they save. This is the decision --price existed for and was");
        println!("     not making: the zstd level moves 0.8% of the size; a lane moves this.");
    } else if inertes.is_empty() {
        println!("  -> keeping every lane: at your price and retention they all pay for themselves.");
    }
    if !inertes.is_empty() {
        // Se apagan igual —no pueden ganar nada— pero se dice el motivo de verdad.
        for n in &inertes {
            match *n {
                "preflate" => std::env::set_var("VC_SIN_PREFLATE", "1"),
                "pdf" => std::env::set_var("VC_SIN_PDF", "1"),
                _ => a.sin_jpeg = true,
            }
        }
        println!("  -> {} does nothing on THIS data (it buys 0.000%), so it is off. That is not",
                 inertes.join(" and "));
        println!("     a price decision: there is simply nothing here for it to work on.");
    }
    println!("  (e8e9, grouping and log-ts are NOT decided: their cost is below this machine's");
    println!("   noise floor, and deciding on noise is worse than not deciding.)");
    let _ = por_tb;
    println!();
    Ok(())
}

fn main() -> Result<()> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{AYUDA}");
        return Ok(());
    }
    if argv.iter().any(|a| a == "-V" || a == "--version") {
        println!("vcompresor {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let cmd = argv.remove(0);
    let mut a = parsear(argv.into_iter())?;
    let pos = |a: &Args, i: usize, que: &str| -> Result<PathBuf> {
        a.libres.get(i).map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("missing {que}\n\ntry: vcompresor --help"))
    };
    let destino = |a: &Args| -> Result<PathBuf> {
        a.out.clone().ok_or_else(|| anyhow::anyhow!("missing «-o <path>»\n\ntry: vcompresor --help"))
    };
    // El avance solo sale por su cuenta si la salida de error es un terminal. Con --progress sale
    // igualmente, para quien lo lee por una tuberia.
    vc_core::pon_progreso(a.progreso);

    // LA CLAVE, antes de cualquier comando.
    //
    // Al CREAR: sal nueva de 16 bytes aleatorios. Al ABRIR: la sal y los parametros salen de la
    // cabecera del propio archivo, no de aqui — asi un archivo hecho con otros parametros se
    // sigue abriendo cuando los de por defecto cambien.
    {
        let crea = matches!(cmd.as_str(), "compress" | "comprimir");
        if crea && a.cifrar {
            let pass = vc_core::cripto::contrasena(a.pass_fichero.as_deref(), true)?;
            let sal = vc_core::cripto::azar(vc_core::cripto::SAL)?;
            let params = vc_core::cripto::Params::default();
            eprintln!("deriving key (Argon2id, {} MiB, {} passes)…", params.mib, params.pasadas);
            let k = vc_core::cripto::deriva(&pass, &sal, params)?;
            vc_core::pon_sal(sal);
            vc_core::pon_clave(Some(k));
        } else if !crea {
            // Cualquier comando que ABRE un archivo: mirar si esta cifrado y pedir la clave.
            if let Some(arch) = a.libres.first() {
                let ruta = PathBuf::from(arch);
                if ruta.exists() {
                    let (cifrado, sal, params) = vc_core::cabecera_cripto(&ruta)?;
                    if cifrado {
                        let pass = vc_core::cripto::contrasena(a.pass_fichero.as_deref(), false)?;
                        eprintln!("deriving key (Argon2id, {} MiB, {} passes)…", params.mib, params.pasadas);
                        let k = vc_core::cripto::deriva(&pass, &sal, params)?;
                        vc_core::pon_clave(Some(k));
                    }
                }
            }
        }
    }

    match cmd.as_str() {
        "compress" | "comprimir" => {
            // Con --stdin no hay ruta de entrada: el dato llega por la tuberia.
            // VARIAS ENTRADAS: todos los posicionales. Antes solo se cogia el primero, asi que
            // la interfaz grafica, para poder guardar una seleccion de cuatro ficheros, mandaba
            // LA CARPETA QUE LOS CONTIENE — y guardaba los 11 GB de Descargas en vez de los
            // 170 MB seleccionados. La limitacion estaba aqui; el arreglo tambien.
            let entradas: Vec<PathBuf> = if a.stdin { vec![PathBuf::from("-")] }
                else {
                    if a.libres.is_empty() { bail!("missing the input to compress"); }
                    a.libres.iter().map(PathBuf::from).collect()
                };
            for e in &entradas {
                if !a.stdin && !e.exists() { bail!("{}: no such file or directory", e.display()); }
            }
            let out = destino(&a)?;
            // Las exclusiones viajan por entorno: `comprimir_pleno` ya tiene diez parametros y
            // cuatro envolturas, y un onceavo obligaria a tocarlas todas para algo que solo
            // consulta el recorrido.
            if !a.excluir.is_empty() { std::env::set_var("VC_EXCLUDE", a.excluir.join("\n")); }
            elige_por_dinero(&mut a, &entradas)?;
            let t = Instant::now();
            let st = vc_core::comprimir_pleno(&entradas, &out, a.nivel, !a.sin_jpeg, !a.sin_agrupar,
                                              a.maximo, a.solido, None,
                                              a.nombre.as_deref().unwrap_or(""), a.stdin)?;
            let s = t.elapsed().as_secs_f64();
            println!("input:      {:>12} B ({:.1} MiB, {} files; {} symlinks skipped)", st.in_bytes, mb(st.in_bytes), st.files, st.simbolicos_saltados);
            println!("output:     {:>12} B ({:.1} MiB)  ratio {:.2}%", st.out_bytes, mb(st.out_bytes), 100.0 * st.out_bytes as f64 / st.in_bytes.max(1) as f64);
            println!("dedup:      {:>12} B saved ({} unique chunks of {})", st.dedup_bytes, st.chunks_unicos, st.chunks_total);
            println!("frames:     {} ({} stored as incompressible)", st.frames, st.frames_store);
            // Se DICE cuantas se saltaron. Un respaldo que omite cosas en silencio es
            // exactamente el defecto que tenian los ficheros especiales antes de F2.67.
            if st.excluidos > 0 { println!("excluded:   {} entries skipped by --exclude", st.excluidos); }
            if st.preflate_files > 0 {
                println!("preflate:   {} .gz unwrapped ({:.1} MiB DEFLATE -> {:.1} MiB plain, {:.1} MiB of corrections)",
                    st.preflate_files, mb(st.preflate_deflate), mb(st.preflate_plain), mb(st.preflate_corr));
            }
            if st.jxl_files > 0 || st.jxl_rechazados > 0 {
                println!("jxl:        {} .jpg transcoded ({:.1} MiB -> {:.1} MiB, {:.2}%), {} rejected to store",
                    st.jxl_files, mb(st.jxl_orig), mb(st.jxl_out),
                    100.0 * st.jxl_out as f64 / st.jxl_orig.max(1) as f64, st.jxl_rechazados);
            }
            println!("time:       {:.1}s  ({:.1} MB/s)", s, mb(st.in_bytes) / s.max(0.001));
            // SI EL ARCHIVO ES MAS GRANDE QUE LA ENTRADA, se dice. Le pasa a todo archivador
            // sobre dato ya comprimido -video, JPEG, un .zip- porque la cabecera, el indice y
            // el pie cuestan lo que cuestan. Medido (bench/cruce-tamano.tsv): el suelo son
            // 118 B, cada fichero cuesta ~25 B, y sobre dato incompresible NUNCA se baja del
            // 100%. Es inevitable; callarlo no lo era.
            if st.out_bytes > st.in_bytes {
                let de_mas = st.out_bytes - st.in_bytes;
                println!();
                println!("note: the archive is {} B larger than the input ({:.1}%).",
                         de_mas, 100.0 * st.out_bytes as f64 / st.in_bytes.max(1) as f64);
                if st.in_bytes < 4096 {
                    println!("      That is the fixed cost of the container (118 B) plus ~25 B");
                    println!("      per file. On anything this small it cannot be avoided.");
                } else {
                    println!("      This data does not compress -already-compressed video, images");
                    println!("      or archives- so all that is left is the container overhead.");
                    println!("      Deduplication and snapshots still work; the size does not shrink.");
                }
            }
        }
        "add" | "añadir" | "anadir" => {
            // El parseador ya reparte posicionales en `a.libres` y opciones en campos. Yo
            // habia supuesto que argv era un vector plano y me invente un bucle de opciones
            // que no compilaba: aqui se usa lo que hay.
            let arch = pos(&a, 0, "the .vc archive to add to")?;
            if a.libres.len() < 2 { bail!("missing the input to add"); }
            let entradas: Vec<PathBuf> = a.libres[1..].iter().map(PathBuf::from).collect();
            // Sin nombre, la fecha. Es lo que uno quiere el 99% de las veces y evita que dos
            // copias del mismo arbol choquen de rutas dentro del archivo.
            let nombre = a.nombre.clone().unwrap_or_else(|| {
                let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs()).unwrap_or(0);
                format!("snapshot-{s}")
            });
            let t = Instant::now();
            let st = vc_core::anadir(&arch, &entradas, &nombre, a.nivel, !a.sin_jpeg,
                                     !a.sin_agrupar, a.maximo, a.solido)?;
            let s = t.elapsed().as_secs_f64();
            println!("snapshot «{nombre}»: {} files, {:.1} MiB of input", st.files, mb(st.in_bytes));
            // SIEMPRE se dice, aunque sea cero. Este numero es la apuesta del incremental hecha
            // visible: son los ficheros que se dieron por buenos sin volver a leerlos, porque su
            // talla y su mtime no habian cambiado. Un cero significa que se rehizo todo el
            // trabajo; el total, que no cambio nada. Si el respaldo sale raro, se mira aqui.
            println!("unchanged:  {:>12} files reused whole ({:.1} MiB not re-read)",
                st.saltados_iguales, mb(st.bytes_saltados));
            println!("dedup:      {:>12} B reused ({} new chunks of {})",
                st.dedup_bytes, st.chunks_unicos, st.chunks_total);
            println!("archive:    {:>12} B", std::fs::metadata(&arch).map(|m| m.len()).unwrap_or(0));
            println!("time:       {:.1}s", s);
        }
        "list" | "listar" => {
            // Saber QUE hay dentro sin extraerlo. Antes no habia forma: `info` daba cuentas,
            // no la lista.
            let arch = pos(&a, 0, "the .vc archive")?;
            let (idx, _f) = vc_core::abrir(&arch)?;
            let pat = a.fichero.as_deref();
            let (mut n, mut bytes) = (0u64, 0u64);
            for fr in &idx.files {
                if let Some(p) = pat {
                    if !vc_core::casa_patron(&fr.path, p) { continue; }
                }
                let tipo = if fr.link_target.is_some() { 'l' }
                           else if fr.mode & 0o170000 == 0o040000 { 'd' }
                           else if fr.enlace_a.is_some() { 'h' }
                           else { '-' };
                println!("{tipo}{:04o} {:>12} {}", fr.mode & 0o7777, fr.size, fr.path);
                n += 1; bytes += fr.size;
            }
            eprintln!("{n} entries, {:.1} MiB of original data", mb(bytes));
        }
        "forget" | "olvidar" => {
            let arch = pos(&a, 0, "the .vc archive")?;
            let nombre = a.nombre.clone()
                .or_else(|| a.libres.get(1).cloned())
                .ok_or_else(|| anyhow::anyhow!("which snapshot? use --name X or give it as the second argument"))?;
            let t0 = Instant::now();
            let (antes, despues) = vc_core::olvidar(&arch, &nombre)?;
            println!("snapshot «{nombre}» removed and archive compacted");
            println!("before:     {:>12} B ({:.1} MiB)", antes, mb(antes));
            println!("after:      {:>12} B ({:.1} MiB)", despues, mb(despues));
            println!("freed:      {:>12} B ({:.1}%)", antes - despues,
                     100.0 * (antes - despues) as f64 / antes.max(1) as f64);
            println!("time:       {:.1}s", t0.elapsed().as_secs_f64());
        }
        "extract" | "descomprimir" => {
            let arch = pos(&a, 0, "the .vc archive")?;
            let out = destino(&a)?;
            let t = Instant::now();
            let mut fallos = Vec::new();
            let (nf, nb, nfail) = vc_core::extraer_patron(&arch, Some(&out), &mut fallos,
                                                          a.fichero.as_deref())?;
            for f in &fallos { eprintln!("{f}"); }
            let s = t.elapsed().as_secs_f64();
            println!("extracted {} files, {:.1} MiB in {:.1}s ({:.1} MB/s)", nf, mb(nb), s, mb(nb) / s.max(0.001));
            if nfail > 0 { bail!("{nfail} file(s) with WRONG blake3 — round-trip broken"); }
            println!("blake3 round-trip: OK ({nf}/{nf})");
        }
        "verify" | "verificar" => {
            let arch = pos(&a, 0, "the .vc archive")?;
            let t = Instant::now();
            let mut fallos = Vec::new();
            let (nf, nb, nfail) = vc_core::extraer(&arch, None, &mut fallos)?;
            for f in &fallos { eprintln!("{f}"); }
            let s = t.elapsed().as_secs_f64();
            if nfail > 0 { bail!("VERIFICATION FAILED: {nfail}/{nf} corrupt files"); }
            println!("OK: {} files, {:.1} MiB verified byte-exact in {:.1}s ({:.1} MB/s)", nf, mb(nb), s, mb(nb) / s.max(0.001));
        }
        "mount" | "montar" => {
            let arch = pos(&a, 0, "the .vc archive")?;
            let punto = pos(&a, 1, "the mountpoint")?;
            #[cfg(target_os = "linux")]
            { fusefs::montar(&arch, &punto)?; }
            #[cfg(not(target_os = "linux"))]
            { let _ = (arch, punto); bail!("mount is Linux-only for now"); }
        }
        "info" => {
            let arch = pos(&a, 0, "the .vc archive")?;
            let (idx, _f) = vc_core::abrir(&arch)?;
            let raw: u64 = idx.files.iter().map(|f| f.size).sum();
            let stored = idx.frames.iter().filter(|f| f.codec == 0).count();
            println!("files: {}   chunks: {}   frames: {} ({} stored)   zstd level: {}", idx.files.len(), idx.chunks.len(), idx.frames.len(), stored, idx.nivel);
            println!("original bytes: {} ({:.1} MiB)", raw, mb(raw));
            for (l, nombre) in [(0u8, "text"), (1, "media"), (2, "other")] {
                let n = idx.frames.iter().filter(|f| f.lane == l).count();
                if n > 0 { println!("  {nombre} lane: {n} frames"); }
            }
        }
        _ => bail!("unknown command: {cmd}\n\ntry: vcompresor --help"),
    }
    Ok(())
}

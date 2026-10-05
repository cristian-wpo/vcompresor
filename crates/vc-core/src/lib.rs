// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! vc-core — contenedor CAS: CDC dedup + carriles + frames seekables + zstd/store.
//! v0.3: transformada preflate para .gz (despliega DEFLATE a texto plano, byte-exacto o fallback).
//! v0.4: transformada JXL para .jpg (transcodifica con jbrd, byte-exacto o fallback).
//! Regla dura: correctitud > ratio > velocidad. blake3 por fichero e índice. Fallback store SIEMPRE.

pub mod cdc;
pub mod inflate;
pub mod deflate_emit;
pub mod deflate;
pub mod formato;
pub mod cripto;
pub mod jpeg;
pub mod jxl;

use anyhow::{bail, Context, Result};
use preflate_rs::{preflate_whole_deflate_stream, recreate_whole_deflate_stream, PreflateConfig};

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// VCMP0005: índice con serialización propia (varints + deltas), sin serde/bincode.
/// El MAGIC sube en cada cambio de formato para que un binario viejo que abra un archivo
/// nuevo falle con «versión de formato distinta» y no con un error de deserialización.
pub const MAGIC: &[u8; 8] = b"VCMP0016";
/// Cabecera fija de VCMP0015: MAGIC + cifrado + sal + parámetros de Argon2 + relleno.
pub const CABECERA: usize = 40;

/// Contenedores que este binario sabe LEER. El de escritura es siempre `MAGIC`.
///
/// Esta lista es la diferencia entre "una herramienta" y "un producto": un binario nuevo TIENE
/// que abrir lo que escribio uno viejo. Antes esto era `if head != MAGIC { bail }` y cada
/// mejora invalidaba todo lo archivado.
///
/// Anadir una version antigua aqui obliga a conservar tambien su lector de indice. Por eso la
/// version del CONTENEDOR solo sube cuando cambia la disposicion del archivo — nunca por
/// cambiar el modelo, que ahora se versiona aparte (vc_cm::MODELO).
// El 0012 se queda LEIBLE: un archivo de ayer no lleva huellas de trozo y se abre igual —solo
// pierde la capacidad de que le anadan una instantanea encima, que es una funcion que cuando
// se escribio no existia.
const CONTENEDORES_LEIBLES: [&[u8; 8]; 5] = [b"VCMP0016", b"VCMP0015", b"VCMP0014", b"VCMP0013", b"VCMP0012"];
// Los tres numeros que deciden DONDE se corta, y con eso todo el dedup.
//
// BARRIDOS EL 01-08 sobre 755 MB del arbol del servidor (bench/cdc-tallas.sh), y el optimo NO
// era el que habia:
//
//     min / medio / max            bytes        vs 64/256/1024      reloj
//     256 / 1024 / 4096 KiB    316.715.399         -0,0294%        46,6 s   <- este
//      64 /  256 / 1024 KiB    316.808.436          0,0000%        47,0 s   <- el de antes
//    1024 / 4096 / 16384 KiB   316.803.213         -0,0016%        48,3 s
//      16 /   64 /  256 KiB    316.848.631         +0,0127%        47,0 s
//       8 /   32 /  128 KiB    316.943.660         +0,0427%        47,9 s
//       4 /   16 /   64 KiB    317.210.513         +0,1269%        47,3 s
//       2 /    8 /   32 KiB    317.657.019         +0,2679%        47,7 s
//
// Es POCO -93.037 B de 316,8 MB- pero es determinista y no cuesta reloj. La curva tiene maximo:
// subir a 1024/4096/16384 vuelve a empeorar.
//
// ⚠️ Y LO QUE DE VERDAD SE APRENDIO AQUI ES DEL INSTRUMENTO, no del numero: hasta hoy el barrido
// probaba CINCO tallas y las cinco eran MAS PEQUEÑAS que el ajuste vigente. Con ese diseño solo
// podia CONFIRMAR el ajuste, nunca refutarlo por arriba — y el optimo estaba justo por arriba.
// Es el mismo fallo que puso PESO_LONG de vlz en 5: medir en media parte del espacio y concluir
// sobre el todo.
//
// OJO SI ALGUN DIA ESTO ES UNA OPCION DE USUARIO: cambiar estos numeros cambia los cortes, asi
// que dos instantaneas tomadas con parametros distintos NO deduplican entre si. Cambiar el
// defecto hoy significa que lo archivado antes no deduplica contra lo nuevo — y hoy se puede
// porque el producto no ha salido. Cuando salga, esto deja de ser gratis.
//
// Se lee UNA vez y se guarda: no hay lectura de entorno en ningun camino caliente.
const CDC_MIN_DEF: usize = 256 * 1024;
const CDC_AVG_DEF: usize = 1024 * 1024;
const CDC_MAX_DEF: usize = 4096 * 1024;

/// El troceado del PLANO del preflate, que va aparte del de los ficheros normales.
///
/// MEDIDO: trocear ese plano fino da -3,554% en png y -4,355% en png-normal, y apagando el
/// carril preflate esas ganancias se desploman a -0,173% y +0,118%. La ganancia es de trocear
/// el plano desplegado, no los ficheros.
///
/// Va aparte porque el plano NO es un fichero del usuario: es un intermedio que ya vive en su
/// propio carril y solo dedupica contra otros planos. Asi se coge la ganancia grande sin tocar
/// el troceado de logs y sql, que con el fino PIERDEN (+2,352% y +1,116%).
fn cdc_params_plano() -> (usize, usize, usize) {
    static P: std::sync::OnceLock<(usize, usize, usize)> = std::sync::OnceLock::new();
    *P.get_or_init(|| {
        let leer = |k: &str, d: usize| {
            std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok()).filter(|&x| x > 0).unwrap_or(d)
        };
        let (mn, av, mx) = (leer("VC_CDC_PLANO_MIN", 4 * 1024),
                            leer("VC_CDC_PLANO_AVG", 16 * 1024),
                            leer("VC_CDC_PLANO_MAX", CDC_MAX_DEF));
        if !(mn < av && av <= mx) {
            eprintln!("aviso: VC_CDC_PLANO_* incoherentes ({mn}/{av}/{mx}); se usan los de siempre");
            return (4 * 1024, 16 * 1024, CDC_MAX_DEF);
        }
        (mn, av, mx)
    })
}

fn cdc_params() -> (usize, usize, usize) {
    static P: std::sync::OnceLock<(usize, usize, usize)> = std::sync::OnceLock::new();
    *P.get_or_init(|| {
        let leer = |k: &str, d: usize| {
            std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok()).filter(|&x| x > 0).unwrap_or(d)
        };
        let (mn, av, mx) = (leer("VC_CDC_MIN", CDC_MIN_DEF),
                            leer("VC_CDC_AVG", CDC_AVG_DEF),
                            leer("VC_CDC_MAX", CDC_MAX_DEF));
        // Un orden imposible daria cortes absurdos sin fallar: se avisa y se cae al defecto.
        if !(mn < av && av <= mx) {
            eprintln!("aviso: VC_CDC_* incoherentes ({mn}/{av}/{mx}); se usan los de siempre");
            return (CDC_MIN_DEF, CDC_AVG_DEF, CDC_MAX_DEF);
        }
        if (mn, av, mx) != (CDC_MIN_DEF, CDC_AVG_DEF, CDC_MAX_DEF) {
            eprintln!("[cdc] min={mn} avg={av} max={mx} (por VC_CDC_*)");
        }
        (mn, av, mx)
    })
}
const STORE_PCT: u128 = 98;
pub const CODEC_STORE: u8 = 0;
pub const CODEC_ZSTD: u8 = 1;
/// Nuestro compresor de mezcla de contextos (`vc-cm`), modo de máximo ratio.
pub const CODEC_CM: u8 = 2;
const PREFLATE_MAX: u64 = 256 * 1024 * 1024;
const JXL_MAX_JPEG: u64 = 128 * 1024 * 1024;
/// Rodilla medida en el corpus real (F2.3): el esfuerzo 3 da toda la ganancia (16,0% sobre
/// zstd-19); el 9 cuesta 6× más tiempo por sólo +0,85%. Ver docs/f2.3-jpeg-jxl.md.
const JXL_ESFUERZO: i64 = 3;
/// Ficheros por lote de pre-transcodificación paralela: acota la memoria en vuelo.
const JXL_LOTE: usize = 64;

pub const LANES: usize = 3;
pub const LANE_TEXT: u8 = 0;
pub const LANE_MEDIA: u8 = 1;
pub const LANE_OTHER: u8 = 2;
const FRAME_TARGET: [usize; LANES] = [8 * 1024 * 1024, 2 * 1024 * 1024, 4 * 1024 * 1024];
/// Bajo `--max` el carril texto usa frames MUCHO más grandes.
///
/// Cada frame arranca el modelo desde cero. Con un modelo débil eso da igual —se calienta en
/// unos KB—, pero con uno fuerte el arranque es caro: es tiempo en el que el modelo está
/// prediciendo peor de lo que sabe. Medido en el corpus de código: 14 frames de 8 MiB dan
/// 11.598.908 bytes; 4 frames de 32 MiB dan 11.057.710, **un 4,7% menos**.
///
/// El precio es paralelismo: menos frames son menos hilos a la vez. Que es exactamente lo que
/// `--max` declara que estás dispuesto a pagar. Fuera de `--max` no cambia nada.
fn frame_target(l: usize) -> usize {
    // --solid manda sobre todo lo demás: es una decisión explícita del que comprime.
    let sol = solido();
    if sol > 0 { return sol << 20; }
    if usar_cm() && (l == 0 || l == 2) { 32 * 1024 * 1024 } else { FRAME_TARGET[l] }
}

fn lane_for(path: &Path) -> u8 {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    const TEXT: &[&str] = &["php","js","ts","css","html","htm","md","txt","sql","csv","json","log","xml","yml","yaml","tpl","sh","py","kt","c","h","cpp","hpp","rs","ini","conf","lock","svg","twig","vue","env"];
    const MEDIA: &[&str] = &["jpg","jpeg","png","gif","webp","mp4","mkv","webm","mov","mp3","ogg","zip","gz","xz","zst","7z","rar","jar","apk","woff","woff2","pdf"];
    if TEXT.contains(&ext.as_str()) { return LANE_TEXT; }
    if MEDIA.contains(&ext.as_str()) { return LANE_MEDIA; }
    // Ni una cosa ni la otra: se MIRA. La lista de extensiones es un atajo para lo conocido,
    // no la autoridad — no tiene go, java, rb, cs, swift, toml, scss, blade ni la mitad de lo
    // que hay ahí fuera, y con --max sólo el carril texto usa el modelo de contextos. Un
    // proyecto en Go se estaba comprimiendo con zstd sin que nadie se enterara.
    if parece_texto(path) { LANE_TEXT } else { LANE_OTHER }
}

/// ¿Los primeros 4 KB parecen texto? Sin adivinar codificación ni cargar tablas: un byte nulo
/// descarta, y basta con que la inmensa mayoría sean imprimibles o espaciado. Un binario
/// tratado como texto sólo pierde un poco de ratio; un texto tratado como binario pierde
/// mucho más, así que ante la duda conviene decir que sí.
/// La misma prueba, pero sobre bytes que ya tenemos en memoria. Los miembros desplegados de
/// un zip no existen como fichero en disco, asi que no se les puede aplicar la version que
/// abre por ruta.
fn parece_texto_bytes(d: &[u8]) -> bool {
    let n = d.len().min(4096);
    if n == 0 { return false; }
    let mut raros = 0usize;
    for &b in &d[..n] {
        if b == 0 { return false; }
        let ok = b >= 0x20 || b == b'\n' || b == b'\r' || b == b'\t';
        if !ok && b < 0x80 { raros += 1; }
    }
    raros * 32 < n
}

fn parece_texto(ruta: &Path) -> bool {
    let Ok(mut f) = File::open(ruta) else { return false };
    let mut buf = [0u8; 4096];
    let n = match f.read(&mut buf) { Ok(n) if n > 0 => n, _ => return false };
    let mut raros = 0usize;
    for &b in &buf[..n] {
        if b == 0 { return false; }
        let ok = b >= 0x20 || b == b'\n' || b == b'\r' || b == b'\t';
        // Los bytes altos cuentan como buenos: son UTF-8 de texto en cualquier idioma que no
        // sea inglés, y descartarlos mandaría al carril equivocado media Europa.
        if !ok && b < 0x80 { raros += 1; }
    }
    raros * 32 < n
}

/// (modo, mtime, uid, gid, rdev) de UNA sola llamada al nucleo, sin seguir enlaces.
///
/// Antes eran dos funciones, `meta_de` y `duenos_de`, cada una con su propio
/// symlink_metadata sobre la MISMA ruta, escritas una debajo de otra y llamadas una debajo de
/// otra en los nueve sitios. Dos viajes al nucleo para leer dos mitades del mismo struct.
///
/// La razon estaba escrita en el codigo: «va aparte para no tocar sus nueve llamadas». Es
/// decir, se duplicaba una llamada al sistema para no editar nueve lineas. Se editaron.
fn atributos_de(ruta: &Path) -> (u32, i64, u32, u32, u64) {
    match std::fs::symlink_metadata(ruta) {
        Ok(md) => (md.mode(), md.mtime(), md.uid(), md.gid(), md.rdev()),
        Err(_) => (0o644, 0, 0, 0, 0),
    }
}

/// Lee los atributos extendidos de una ruta, SIN seguir enlaces simbolicos.
///
/// Las llamadas difieren entre Linux y macOS —macOS lleva dos argumentos mas— asi que se
/// envuelven por separado en vez de suponer que una llamada del sistema tiene la misma firma en
/// todas partes.
fn xattrs_de(ruta: &Path) -> Vec<(String, Vec<u8>)> {
    use std::ffi::{CString, OsStr};
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(ruta.as_os_str().as_bytes()) else { return Vec::new() };

    #[cfg(target_os = "macos")]
    let n = unsafe { libc::listxattr(c.as_ptr(), std::ptr::null_mut(), 0, libc::XATTR_NOFOLLOW) };
    #[cfg(not(target_os = "macos"))]
    let n = unsafe { libc::llistxattr(c.as_ptr(), std::ptr::null_mut(), 0) };
    if n <= 0 { return Vec::new(); }

    let mut lista = vec![0i8; n as usize];
    #[cfg(target_os = "macos")]
    let n = unsafe { libc::listxattr(c.as_ptr(), lista.as_mut_ptr(), lista.len(), libc::XATTR_NOFOLLOW) };
    #[cfg(not(target_os = "macos"))]
    let n = unsafe { libc::llistxattr(c.as_ptr(), lista.as_mut_ptr(), lista.len()) };
    if n <= 0 { return Vec::new(); }

    let bytes: Vec<u8> = lista[..n as usize].iter().map(|&b| b as u8).collect();
    let mut out = Vec::new();
    for nombre in bytes.split(|&b| b == 0) {
        if nombre.is_empty() { continue; }
        let s = String::from_utf8_lossy(nombre).into_owned();
        // Estos dos los pone el sistema al crear el fichero; volver a ponerlos a mano da un
        // fichero peor, no mejor.
        if s == "com.apple.ResourceFork" || s == "com.apple.FinderInfo" { continue; }
        let Ok(cn) = CString::new(nombre) else { continue };
        #[cfg(target_os = "macos")]
        let tam = unsafe { libc::getxattr(c.as_ptr(), cn.as_ptr(), std::ptr::null_mut(), 0, 0, libc::XATTR_NOFOLLOW) };
        #[cfg(not(target_os = "macos"))]
        let tam = unsafe { libc::lgetxattr(c.as_ptr(), cn.as_ptr(), std::ptr::null_mut(), 0) };
        if tam < 0 { continue; }
        let mut v = vec![0u8; tam as usize];
        #[cfg(target_os = "macos")]
        let leido = unsafe { libc::getxattr(c.as_ptr(), cn.as_ptr(), v.as_mut_ptr() as *mut libc::c_void, v.len(), 0, libc::XATTR_NOFOLLOW) };
        #[cfg(not(target_os = "macos"))]
        let leido = unsafe { libc::lgetxattr(c.as_ptr(), cn.as_ptr(), v.as_mut_ptr() as *mut libc::c_void, v.len()) };
        if leido < 0 { continue; }
        v.truncate(leido as usize);
        let _ = OsStr::new("");
        out.push((s, v));
    }
    // Ordenados: dos ejecuciones sobre el mismo fichero tienen que dar el mismo archivo, y el
    // orden en que el sistema los lista no esta garantizado.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Vuelve a poner los atributos extendidos. Un fallo NO es error: hay atributos que el sistema
/// no deja escribir (la cuarentena, por ejemplo, en algunos casos) y abortar una restauracion
/// entera por eso seria peor que perderlos.
fn pon_xattrs(dest: &Path, xs: &[(String, Vec<u8>)]) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(dest.as_os_str().as_bytes()) else { return };
    for (nombre, valor) in xs {
        let Ok(cn) = CString::new(nombre.as_bytes()) else { continue };
        #[cfg(target_os = "macos")]
        unsafe { libc::setxattr(c.as_ptr(), cn.as_ptr(), valor.as_ptr() as *const libc::c_void, valor.len(), 0, libc::XATTR_NOFOLLOW) };
        #[cfg(not(target_os = "macos"))]
        unsafe { libc::lsetxattr(c.as_ptr(), cn.as_ptr(), valor.as_ptr() as *const libc::c_void, valor.len(), 0) };
    }
}

/// (dispositivo, inodo, cuantos enlaces) — para reconocer que dos rutas son EL MISMO fichero.
fn inodo_de(ruta: &Path) -> Option<(u64, u64, u64)> {
    let md = std::fs::symlink_metadata(ruta).ok()?;
    if md.nlink() <= 1 { return None; }
    Some((md.dev(), md.ino(), md.nlink()))
}

/// Qué es cada cosa que aparece en el recorrido. Un directorio VACÍO es una entrada de pleno
/// derecho: si no se guarda no se recrea, y «como estaba» incluye los directorios vacíos.
#[derive(Clone, Copy, PartialEq)]
enum Tipo { Fichero, Symlink, DirVacio, Especial }

/// Recorre `dir` en profundidad SIN seguir enlaces, ordenando cada nivel por nombre para que
/// el archivo salga igual en cualquier máquina.
fn recorrer(dir: &Path, out: &mut Vec<(PathBuf, Tipo)>) -> Result<()> {
    let mut entradas: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .with_context(|| format!("leer directorio {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entradas.sort_by_key(|e| e.file_name());
    for e in entradas {
        let p = e.path();
        let md = std::fs::symlink_metadata(&p).with_context(|| format!("stat {}", p.display()))?;
        if md.is_symlink() { out.push((p, Tipo::Symlink)); }
        else if md.is_dir() {
            // Si de todo el subárbol no ha salido NADA, es una rama vacía y hay que
            // anotarla. Si ha salido algo, los directorios de en medio se recrean solos
            // como padres de lo que haya dentro.
            let antes = out.len();
            recorrer(&p, out)?;
            if out.len() == antes { out.push((p, Tipo::DirVacio)); }
        }
        else if md.is_file() { out.push((p, Tipo::Fichero)); }
        // Fifos y dispositivos. Antes se caian por el borde del if sin decir nada: el
        // recorrido los veia y simplemente no los metia en ninguna rama.
        else { out.push((p, Tipo::Especial)); }
    }
    Ok(())
}

/// Un flujo DEFLATE desplegado. `trozos` son los tramos del fichero original que ocupaba
/// (varios si venía partido, como los IDAT de un PNG).
#[derive(Clone)]
pub struct Hueco {
    pub trozos: Vec<(u32, u32)>,
    pub correcciones: Vec<u8>,
    /// Cuantos bytes del `plano` compartido consume este hueco. Con `interior`, son los
    /// bytes del plano INTERIOR, no los del miembro: el miembro se reconstruye a partir de
    /// ellos antes de volver a comprimirse como deflate.
    pub len_plano: u32,
    /// El miembro desplegado era a su vez un contenedor con DEFLATE dentro (un PNG en un
    /// .docx, que es el caso que vale el 16,92%). `None` = miembro normal, y entonces el
    /// archivo sale byte a byte igual que antes de que esto existiera.
    pub interior: Option<Box<Interior>>,
    /// Que clase de region es. Empezo siendo un `crudo: bool` y crecio a clase en cuanto hizo
    /// falta una tercera: dos booleanos independientes harian representable "no crudo pero
    /// jxl", que no significa nada y aun asi habria que decidir que hacer si apareciera.
    ///   0 HUECO_DEFLATE  era un flujo deflate; se recrea con preflate
    ///   1 HUECO_CRUDO    miembro ALMACENADO; los bytes del plano SON la region
    ///   2 HUECO_JXL      miembro ALMACENADO transcodificado; hay que volver de JXL a JPEG
    pub clase: u8,
}

pub const HUECO_DEFLATE: u8 = 0;
pub const HUECO_CRUDO: u8 = 1;
pub const HUECO_JXL: u8 = 2;

/// Una transformada DEFLATE anidada dentro de un hueco.
#[derive(Clone)]
pub struct Interior {
    pub esqueleto: Vec<u8>,
    pub huecos: Vec<Hueco>,
}

#[derive(Clone)]
pub enum Transform {
    /// Flujos DEFLATE desplegados a texto plano: cubre gzip, png, zip y todo lo que es un
    /// zip por dentro (docx, xlsx, pptx, jar, apk, epub). Los chunks referencian el PLANO;
    /// el `esqueleto` es el resto del fichero, literal.
    Deflate { esqueleto: Vec<u8>, huecos: Vec<Hueco> },
    /// .jpg transcodificado: los chunks referencian el flujo JXL, que YA lleva dentro el
    /// `jbrd` con todo lo necesario para recrear el JPEG original bit a bit. Sin datos
    /// laterales: la reconstrucción se verificó al empaquetar.
    JpegJxl,
    /// Ejecutable x86 con las llamadas pasadas a direcciones absolutas. Sin datos laterales:
    /// la vuelta es determinista y se verificó al empaquetar.
    E8e9,
    /// Log con cabecera de fecha: el timestamp va aparte y en diferencias.
    LogTs,
}

#[derive(Clone)]
pub struct FrameRec { pub offset: u64, pub csize: u64, pub rsize: u64, pub codec: u8, pub lane: u8 }
#[derive(Clone)]
pub struct ChunkRec {
    pub frame: u32, pub off: u32, pub len: u32,
    /// Huella del contenido (blake3 recortado a 128 bits, igual que la de fichero).
    ///
    /// Guarda QUE ES el trozo, no solo donde esta. Sin esto no se puede deduplicar contra un
    /// archivo que ya existe sin descomprimirlo entero, y por tanto no hay instantaneas
    /// incrementales. Cuesta 0,1% en datos grandes -medido- y hasta 5,9% en arboles de
    /// ficheros diminutos, que es justo donde lo incremental no hace falta.
    pub hash: [u8; 16],
}
/// Huella de fichero: blake3 recortado a 128 bits.
///
/// Un hash es aleatorio por definición, así que no se comprime: cada byte que guardemos de
/// más es un byte que el archivo arrastra entero. Con 27.162 ficheros, pasar de 32 a 16
/// bytes quita 434 KB de lastre incompresible. Y 128 bits siguen siendo una garantía
/// absurda: para que dos ficheros distintos chocaran harían falta del orden de 2^64.
fn recorta(h: blake3::Hash) -> [u8; 16] {
    let mut o = [0u8; 16];
    o.copy_from_slice(&h.as_bytes()[..16]);
    o
}
fn huella(d: &[u8]) -> [u8; 16] { recorta(blake3::hash(d)) }

/// Memoria fisica de la maquina, en MiB. 0 si no se puede saber.
///
/// Se lee de /proc/meminfo y no de una constante: un archivador que supone cuanta RAM hay es
/// un archivador que funciona en la maquina del que lo escribio y tumba la del que lo usa.
/// Si no se puede leer -otro sistema, un contenedor raro- se devuelve un valor conservador en
/// vez de uno optimista: quedarse corto cuesta tiempo, pasarse cuesta la maquina.
fn memoria_fisica_mb() -> usize {
    std::fs::read_to_string("/proc/meminfo").ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("MemTotal:")).map(String::from))
        .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<usize>().ok()))
        .map(|kb| kb / 1024)
        .unwrap_or(4096)
}

/// Los primeros 128 bits de un hash que ya viene en bytes.
fn recorta_bytes(h: &[u8; 32]) -> [u8; 16] {
    let mut o = [0u8; 16];
    o.copy_from_slice(&h[..16]);
    o
}

#[derive(Clone)]
#[derive(Default)]
pub struct FileRec {
    pub path: String, pub size: u64, pub chunks: Vec<u64>, pub hash: [u8; 16], pub transform: Option<Transform>,
    /// permisos unix (bits de modo); mtime en segundos desde epoch; si es symlink, su destino.
    pub mode: u32, pub mtime: i64, pub link_target: Option<String>,
    /// Dueno y grupo. Sin esto, restaurar un respaldo de sistema como root dejaba TODO del
    /// usuario que restaura, que es peor que no restaurarlo: parece que ha funcionado.
    pub uid: u32, pub gid: u32,
    /// Para fifos y dispositivos: el numero de dispositivo. 0 en un fichero normal.
    pub rdev: u64,
    /// Atributos extendidos: (nombre, valor). En macOS ahi viven el comentario del Finder, las
    /// etiquetas de color y la cuarentena; en Linux, SELinux y las capabilities. Se perdian en
    /// silencio, y `tar` no los pierde.
    pub xattr: Vec<(String, Vec<u8>)>,
    /// ENLACE DURO: indice del fichero cuyo contenido comparte. El segundo y siguientes
    /// enlaces al mismo inodo no guardan contenido, guardan a quien apuntan — y al extraer se
    /// crea un enlace de verdad en vez de una copia.
    pub enlace_a: Option<u32>,
}
#[derive(Default)]
pub struct Index { pub files: Vec<FileRec>, pub chunks: Vec<ChunkRec>, pub frames: Vec<FrameRec>, pub nivel: i32 }

#[derive(Default)]
pub struct Stats {
    pub in_bytes: u64, pub out_bytes: u64, pub files: u64,
    pub chunks_total: u64, pub chunks_unicos: u64, pub dedup_bytes: u64,
    pub frames: u64, pub frames_store: u64, pub simbolicos_saltados: u64, pub hilos: usize,
    /// Cuantas entradas se saltaron por --exclude. Se cuenta y se dice: un respaldo que se salta
    /// cosas en silencio es exactamente el problema que tenia con los ficheros especiales.
    pub excluidos: u64,
    /// Ficheros que NO se volvieron a procesar porque no habian cambiado desde la instantanea
    /// anterior. Se cuentan y se dicen: un respaldo que se salta cosas en silencio es
    /// indistinguible de uno roto.
    pub saltados_iguales: u64,
    pub bytes_saltados: u64,
    pub preflate_files: u64, pub preflate_deflate: u64, pub preflate_plain: u64, pub preflate_corr: u64,
    /// Miembros que resultaron ser a su vez contenedores y volvieron al router.
    pub deflate_recursivos: u64,
    pub deflate_rechazados: u64,
    pub jxl_files: u64, pub jxl_rechazados: u64, pub jxl_orig: u64, pub jxl_out: u64,
}

struct PendingFrame { id: u32, lane: u8, data: Vec<u8> }
struct DoneFrame { id: u32, lane: u8, codec: u8, rsize: u64, bytes: Vec<u8> }

struct Assembler {
    idx: Index,
    lanes: Vec<Vec<u8>>,
    pendientes: Vec<Vec<u64>>,
    dedup: HashMap<[u8; 32], u64>,
    st: Stats,
    next_frame: u32,
    tx: mpsc::SyncSender<PendingFrame>,
}

impl Assembler {
    fn flush_lane(&mut self, l: usize) -> Result<()> {
        if self.lanes[l].is_empty() { return Ok(()); }
        let id = self.next_frame;
        self.next_frame += 1;
        for cid in self.pendientes[l].drain(..) { self.idx.chunks[cid as usize].frame = id; }
        let data = std::mem::replace(&mut self.lanes[l], Vec::with_capacity(frame_target(l) + cdc_params().2));
        // EL TRABAJO QUE DE VERDAD QUEDA POR HACER. Ver `PROG_PLANO_TOTAL`.
        PROG_PLANO_TOTAL.fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);
        cron(F_ESPERAR, || self.tx.send(PendingFrame { id, lane: l as u8, data })).context("pool de compresión caído")?;
        self.st.frames += 1;
        Ok(())
    }

    fn add_chunk(&mut self, lane: u8, data: &[u8]) -> Result<u64> {
        // La clave del mapa lleva SOLO los 128 bits que se guardan en el indice. Si aqui se
        // usaran los 256 y en el indice 128, un trozo que ya estaba no se reconoceria al
        // continuar el archivo y se volveria a comprimir: lo incremental no funcionaria y
        // nadie se enteraria, porque el resultado seria correcto, solo grande.
        let mut h = [0u8; 32];
        h[..16].copy_from_slice(&recorta(cron(F_HASH, || blake3::hash(data))));
        if let Some(&id) = self.dedup.get(&h) {
            self.st.dedup_bytes += data.len() as u64;
            return Ok(id);
        }
        let l = lane as usize;
        if !self.lanes[l].is_empty() && self.lanes[l].len() + data.len() > frame_target(l) {
            self.flush_lane(l)?;
        }
        let id = self.idx.chunks.len() as u64;
        // El hash ya estaba calculado tres lineas mas arriba para el mapa de deduplicacion.
        // Guardarlo no cuesta ni un ciclo mas: lo unico que cambia es que deja de tirarse.
        self.idx.chunks.push(ChunkRec {
            frame: u32::MAX, off: self.lanes[l].len() as u32, len: data.len() as u32,
            hash: recorta_bytes(&h),
        });
        cron(F_DEDUP, || self.lanes[l].extend_from_slice(data));
        self.pendientes[l].push(id);
        self.dedup.insert(h, id);
        self.st.chunks_unicos += 1;
        // Cada trozo nuevo son ~28 B de indice que habra que comprimir al cerrar (el registro:
        // frame, offset, longitud y 16 B de huella), mas su referencia desde el fichero.
        anota_indice(28 + 8);
        Ok(id)
    }
}

/// Quita del fichero los tramos ocupados por flujos deflate: lo que queda es el esqueleto,
/// que vuelve literal. Falla si los tramos se solapan o se salen (índice incoherente).
fn esqueleto_de(d: &[u8], huecos: &[Hueco]) -> Option<Vec<u8>> {
    let mut segs: Vec<(u32, u32)> = Vec::new();
    for h in huecos { segs.extend_from_slice(&h.trozos); }
    segs.sort_by_key(|s| s.0);
    let mut out = Vec::with_capacity(d.len());
    let mut pos = 0usize;
    for (o, l) in segs {
        let (o, l) = (o as usize, l as usize);
        if o < pos || o.checked_add(l)? > d.len() { return None; }
        out.extend_from_slice(&d[pos..o]);
        pos = o + l;
    }
    out.extend_from_slice(&d[pos..]);
    Some(out)
}

/// Rehace el fichero original intercalando el esqueleto con los flujos deflate recreados.
pub fn rehace_deflate_pub(esqueleto: &[u8], huecos: &[Hueco], plano: &[u8]) -> Result<Vec<u8>> { rehace_deflate(esqueleto, huecos, plano) }

fn rehace_deflate(esqueleto: &[u8], huecos: &[Hueco], plano: &[u8]) -> Result<Vec<u8>> {
    let mut cuerpos: Vec<Vec<u8>> = Vec::with_capacity(huecos.len());
    let mut p = 0usize;
    for h in huecos {
        let fin = p.checked_add(h.len_plano as usize).ok_or_else(|| anyhow::anyhow!("desbordamiento"))?;
        if fin > plano.len() { bail!("el plano no da para todos los flujos"); }
        // Si el miembro venia desplegado, primero se rehace EL MIEMBRO a partir de su plano
        // interior; solo entonces se le puede volver a aplicar el deflate de fuera. El orden
        // importa: de dentro hacia fuera, igual que se construyo.
        let miembro;
        let cuerpo_plano: &[u8] = match &h.interior {
            Some(it) => { miembro = rehace_deflate(&it.esqueleto, &it.huecos, &plano[p..fin])?;
                          &miembro }
            None => &plano[p..fin],
        };
        // Un hueco que no era deflate no pasa por preflate. El CRUDO ya son sus bytes; el
        // JXL hay que devolverlo a JPEG antes.
        if h.clase != HUECO_DEFLATE {
            let total: usize = h.trozos.iter().map(|&(_, l)| l as usize).sum();
            let region = if h.clase == HUECO_JXL {
                jxl::jxl_a_jpeg(cuerpo_plano, total)
                    .ok_or_else(|| anyhow::anyhow!("no se pudo devolver el JXL a JPEG"))?
            } else { cuerpo_plano.to_vec() };
            if region.len() != total {
                bail!("region de clase {} con longitud {} y se esperaban {total}", h.clase, region.len());
            }
            cuerpos.push(region);
            p = fin;
            continue;
        }
        let c = recreate_whole_deflate_stream(cuerpo_plano, &h.correcciones)
            .map_err(|e| anyhow::anyhow!("recrear deflate: {e}"))?;
        let esperado: u64 = h.trozos.iter().map(|&(_, l)| l as u64).sum();
        if c.len() as u64 != esperado { bail!("el flujo recreado mide {} y se esperaban {esperado}", c.len()); }
        cuerpos.push(c);
        p = fin;
    }
    let mut segs: Vec<(u32, u32, usize)> = Vec::new();
    for (i, h) in huecos.iter().enumerate() {
        for &(o, l) in &h.trozos { segs.push((o, l, i)); }
    }
    segs.sort_by_key(|s| s.0);
    let mut out = Vec::with_capacity(esqueleto.len() + plano.len());
    let (mut esq, mut pos) = (0usize, 0u32);
    let mut cur = vec![0usize; huecos.len()];
    for (o, l, i) in segs {
        if o < pos { bail!("tramos solapados en el índice"); }
        let n = (o - pos) as usize;
        if esq + n > esqueleto.len() { bail!("esqueleto corto"); }
        out.extend_from_slice(&esqueleto[esq..esq + n]);
        esq += n;
        let c = &cuerpos[i];
        let fin = cur[i] + l as usize;
        if fin > c.len() { bail!("tramo fuera del flujo recreado"); }
        out.extend_from_slice(&c[cur[i]..fin]);
        cur[i] = fin;
        pos = o + l;
    }
    out.extend_from_slice(&esqueleto[esq..]);
    Ok(out)
}

/// `preflate-rs` ENTRA EN PÁNICO con algunos flujos reales (visto con PNG del corpus:
/// `tree_predictor.rs:169`). Un pánico de una dependencia no puede tumbar el empaquetado
/// entero, así que se atrapa y el fichero cae al camino normal. Es, de paso, el mejor
/// argumento para que este carril acabe siendo nuestro.
///
/// LA JUNTURA QUE HACÍA EL TRABAJO DOS VECES (2026-07-31). Esta función recreaba el flujo y lo
/// comparaba byte a byte —la línea `rec != cuerpo`—, y ADEMÁS le pasaba a preflate su config por
/// defecto, que trae `verify_compression: true` y le hace **recrear el flujo entero otra vez por
/// dentro**. Dos recreaciones completas del mismo flujo, seguidas, para decidir lo mismo.
///
/// Y son la misma comprobación, no dos distintas. El validador interno hace exactamente una cosa
/// cuando no cuadra: `return Err(RoundtripMismatch)` (`stream_processor.rs:145` y `:211`). No
/// corrige, no reintenta, no cambia la codificación — solo rechaza. Ese `Err` cae por el `.ok()?`
/// de la línea de abajo y devuelve `None`; nuestro `rec != cuerpo` devuelve `None` también. Mismo
/// conjunto aceptado, misma consecuencia, por dos caminos.
///
/// Ninguna de las dos herramientas sabía que la otra estaba comprobando. Esa es la juntura: cada
/// una traía su premisa de cuando vivía sola.
///
/// SE QUITA LA SUYA Y SE QUEDA LA NUESTRA, y no al revés, por tres razones: la nuestra compara
/// contra `cuerpo`, que son los bytes exactos que tendremos que reproducir; vive en NUESTRA capa,
/// así que sobrevive a cualquier cambio del código vendorizado; y va dentro del mismo
/// `catch_unwind`, que es lo que nos protege de sus pánicos.
///
/// MEDIDO sobre 958 PNG repartidos (249 MB, 947 flujos aceptados, 200,4 MiB de DEFLATE), en serie
/// y alternando A/B/A/B para descartar deriva:
///     con su verificación     58,7 s · 59,7 s     media 59,2 s
///     sin su verificación     43,7 s · 44,0 s     media 43,8 s
///     -> -25,9% del reloj del ARCHIVADOR ENTERO
/// La dispersión dentro de cada variante es 1,7% y 0,7%, o sea en el suelo de ruido (1,25%): el
/// 25,9% es quince veces el ruido.
///
/// Y lo que de verdad prueba que era un duplicado y no una red de seguridad no es el reloj:
///     · los dos .vc salen con el MISMO sha256 — idénticos byte a byte, 182.834.586 B
///     · el mismo conjunto aceptado: 947 flujos, 200,4 MiB de DEFLATE, 6,4 MiB de correcciones
///     · ida y vuelta bit a bit sobre los 958 ficheros, blake3 958/958 OK
/// Eso es determinista y no depende de la máquina. El reloj es del Mac y es indicativo.
///
/// ⚠️ EL 25,9% NO SE EXTRAPOLA AL BANCO. Este corpus es PNG puro, o sea DEFLATE de punta a punta:
/// es el caso mejor para este ahorro. En un corpus mixto baja en proporción a cuánto DEFLATE
/// lleve. La cifra sobre el banco real se mide aparte, en el servidor y con el reloj libre.
fn preflate_seguro(cuerpo: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    silencia_panicos_ajenos();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let cfg = PreflateConfig { verify_compression: false, ..PreflateConfig::default() };
        let (res, texto) = preflate_whole_deflate_stream(cuerpo, &cfg).ok()?;
        if res.compressed_size != cuerpo.len() { return None; }
        let rec = recreate_whole_deflate_stream(texto.text(), &res.corrections).ok()?;
        if rec != cuerpo { return None; } // paranoia: si no es byte-exacto, NO entra
        Some((texto.text().to_vec(), res.corrections))
    }));
    r.ok().flatten()
}

static SILENCIO: std::sync::Once = std::sync::Once::new();
/// Calla los pánicos que vienen de dentro de preflate: los gestionamos nosotros con
/// `catch_unwind` y, si no, llenarían la salida de trazas en un corpus grande.
fn silencia_panicos_ajenos() {
    SILENCIO.call_once(|| {
        let anterior = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let en_preflate = info.location().map(|l| l.file().contains("preflate")).unwrap_or(false);
            if !en_preflate { anterior(info); }
        }));
    });
}

/// Intenta desplegar los flujos DEFLATE que el fichero lleve dentro (gzip, png, zip y todo
/// lo que es un zip por dentro). true = fichero consumido, con la recreación byte a byte
/// YA comprobada aquí mismo; false = que siga el camino normal.
/// Despliega los flujos DEFLATE de un MIEMBRO que resulto ser a su vez un contenedor.
///
/// `prof` acota cuanto se puede anidar. No es paranoia de manual: un fichero con contenedores
/// encajados sin fin es trivial de fabricar y agotaria la memoria del que descomprime, que es
/// justo quien no eligio el fichero. Dos niveles cubren docx -> png -> deflate, que es el caso
/// real; el tercero no aparece en ningun corpus que hayamos medido.
fn despliega_interior(d: &[u8], prof: u8) -> Option<(Vec<u8>, Vec<Hueco>, Vec<u8>)> {
    if prof == 0 { return None; }
    let flujos = deflate::flujos(d)?;
    let mut huecos: Vec<Hueco> = Vec::with_capacity(flujos.len());
    let mut plano: Vec<u8> = Vec::new();
    let mut bytes_deflate = 0u64;
    for f in &flujos {
        let mut cuerpo = Vec::new();
        let mut fuera = false;
        for &(o, l) in f {
            let (o, l) = (o as usize, l as usize);
            if o + l > d.len() { fuera = true; break; }
            cuerpo.extend_from_slice(&d[o..o + l]);
        }
        if fuera || cuerpo.len() < 16 { continue; }
        let Some((texto, correcciones)) = preflate_seguro(&cuerpo) else { continue };
        bytes_deflate += cuerpo.len() as u64;
        plano.extend_from_slice(&texto);
        huecos.push(Hueco { trozos: f.clone(), correcciones, len_plano: texto.len() as u32,
                            interior: None, clase: HUECO_DEFLATE });
    }
    if huecos.is_empty() { return None; }
    let esqueleto = esqueleto_de(d, &huecos)?;
    // El mismo descarte que arriba: si desplegar no aporta, no se complica el archivo.
    if plano.len() as u64 <= bytes_deflate / 8 { return None; }
    Some((esqueleto, huecos, plano))
}

/// El nivel de zstd del carril MEDIA. 9 desde el 31-07; antes era un 3 clavado que nadie habia
/// medido. -2,212% de bytes sobre 71 videos reales por +0,27 s en 632 MB. Ver el comentario largo
/// donde se usa. VC_MEDIA_NIVEL lo cambia, para volver a medirlo cuando algo mas cambie: hoy la
/// ventana=frame ya invalido dos curvas -la del CDC y la de los frames- sin avisar a nadie.
fn nivel_media() -> i32 {
    std::env::var("VC_MEDIA_NIVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(9)
}

/// Todo el trabajo CARO del carril deflate, sin tocar nada compartido.
///
/// Se separo de `try_deflate` para poder correrlo en paralelo: es el 69,9% del reloj del
/// archivador y estaba en el hilo principal mientras el carril de JPEG —cuatro veces mas
/// pequeño— si se repartia.
fn prepara_deflate(d: &[u8]) -> Option<Preparado> {
    let mut recursivos: u64 = 0;
    let flujos = deflate::regiones(d)?;
    let mut huecos: Vec<Hueco> = Vec::with_capacity(flujos.len());
    let mut plano: Vec<u8> = Vec::new();
    let mut bytes_deflate = 0u64;
    /// Donde empieza cada miembro dentro de `plano`, cuanto ocupa y a que carril va.
    /// Sin esto, `plano` es un bloque opaco y no hay forma de tratar a cada miembro como
    /// lo que es.
    let mut tramos: Vec<(usize, usize, u8)> = Vec::with_capacity(flujos.len());
    // Un flujo que no se pueda desplegar NO tumba el fichero entero: se queda literal dentro
    // del esqueleto y se sigue con los demás. Antes era todo o nada, y eso hacía que el
    // carril funcionara en un .docx (diez flujos) y NUNCA en un zip de verdad: basta con que
    // uno de novecientos sea diminuto o se le atragante a preflate para perderlo todo.
    for reg in &flujos {
        let f = &reg.tramos;
        let mut cuerpo = Vec::new();
        let mut fuera = false;
        for &(o, l) in f {
            let (o, l) = (o as usize, l as usize);
            if o + l > d.len() { fuera = true; break; }
            cuerpo.extend_from_slice(&d[o..o + l]);
        }
        if fuera || cuerpo.len() < 16 { continue; }
        // Un miembro ALMACENADO no se despliega con preflate —nunca estuvo deflatado—: sus
        // bytes ya son el contenido. Solo vale la pena sacarlo del esqueleto si el contenido
        // es a su vez un contenedor; si no, moverlo no aporta y complica el archivo.
        // Un miembro ALMACENADO no se despliega con preflate —nunca estuvo deflatado—. Se
        // mira que es: si lleva DEFLATE dentro (un PNG) se saca del esqueleto tal cual y su
        // transformada interior hara el trabajo; si es un JPEG se transcodifica a JXL. Si no
        // es ninguna de las dos, moverlo del esqueleto no aporta y solo complica el archivo.
        let mut clase = if reg.crudo { HUECO_CRUDO } else { HUECO_DEFLATE };
        let (texto, correcciones) = if reg.crudo {
            if deflate::flujos(&cuerpo).is_some() {
                (cuerpo.clone(), Vec::new())
            } else if jxl::parece_jpeg(&cuerpo) {
                // `jxl_verificado` hace la vuelta y la compara ANTES de devolver nada: si el
                // JPEG no se recrea bit a bit, devuelve None y el miembro se queda donde
                // estaba. Un .jpg de un movil raro no puede costarnos el fichero entero.
                match jxl::jxl_verificado(&cuerpo, JXL_ESFUERZO) {
                    Some(j) => { clase = HUECO_JXL; (j, Vec::new()) }
                    None => continue,
                }
            } else { continue }
        } else {
            match preflate_seguro(&cuerpo) { Some(v) => v, None => continue }
        };
        bytes_deflate += cuerpo.len() as u64;
        // AQUI esta el 16,92%: el miembro vuelve a pasar por el router. Un PNG dentro de un
        // .docx es un PNG y merece su carril, no acabar como bytes opacos en un bufer.
        let (interior, trozo) = match despliega_interior(&texto, 1) {
            Some((e2, h2, p2)) => (Some(Box::new(Interior { esqueleto: e2, huecos: h2 })), p2),
            None => (None, texto),
        };
        let ini = plano.len();
        // El carril se decide MIRANDO lo que va a ir al bufer. Si el miembro se desplego, lo
        // que va es su plano interior —que ya es texto casi siempre—, no el contenedor.
        let carril = if parece_texto_bytes(&trozo) { LANE_TEXT } else { LANE_OTHER };
        let n_trozo = trozo.len();
        plano.extend_from_slice(&trozo);
        tramos.push((ini, n_trozo, carril));
        if interior.is_some() { recursivos += 1; }
        huecos.push(Hueco { trozos: f.clone(), correcciones, len_plano: n_trozo as u32,
                            interior, clase });
    }
    if huecos.is_empty() { return None; }
    let esqueleto = esqueleto_de(d, &huecos)?;
    // Si el despliegue no aporta, no complicamos el archivo.
    if plano.len() as u64 <= bytes_deflate / 8 { return None; }
    // Trocear POR MIEMBRO. Asi los limites caen donde termina un miembro y empieza otro, en
    // vez de a mitad, y dos `styles.xml` identicos de dos .docx distintos producen el MISMO
    // trozo y dedupican. Concatenados en orden siguen dando `plano` byte a byte, que es lo
    // unico que la reconstruccion necesita.
    //
    // Los miembros pequenos y seguidos del mismo carril se juntan antes de trocear: partir un
    // zip de novecientas entradas en novecientos trozos mete mas indice del que ahorra.
    Some(Preparado { plano, huecos, tramos, esqueleto, bytes_deflate, recursivos })
}

/// Mete en el archivo lo que `prepara_deflate` dejo listo. Es la parte que MUTA, y se llama
/// siempre EN EL MISMO ORDEN que el bucle secuencial: si el orden cambiara, los trozos
/// entrarian distinto en el indice y el archivo dejaria de ser byte a byte el mismo.
fn mete_deflate(a: &mut Assembler, ruta: &Path, rel: String, d: &[u8], pre: Preparado)
    -> Result<bool> {
    let Preparado { plano, huecos, tramos, esqueleto, bytes_deflate, recursivos } = pre;
    a.st.deflate_recursivos += recursivos;
    const MIN_TRAMO: usize = 4096;
    let mut ids = Vec::new();
    let mut i = 0usize;
    while i < tramos.len() {
        let (ini, mut len, carril) = tramos[i];
        i += 1;
        while i < tramos.len() && tramos[i].2 == carril && len < MIN_TRAMO {
            len += tramos[i].1;
            i += 1;
        }
        let m = &plano[ini..ini + len];
        // El PLANO desplegado lleva su propio troceado, mas fino: ver cdc_params_plano().
        for (coff, clen) in { let (a, b, c) = cdc_params_plano(); cdc::trozos(m, a, b, c) } {
            a.st.chunks_total += 1;
            ids.push(a.add_chunk(carril, &m[coff..coff + clen])?);
        }
    }
    a.st.in_bytes += d.len() as u64;
    a.st.files += 1;
    a.st.preflate_files += 1;
    a.st.preflate_deflate += bytes_deflate;
    a.st.preflate_plain += plano.len() as u64;
    a.st.preflate_corr += huecos.iter().map(|h| h.correcciones.len() as u64).sum::<u64>();
    let (mode, mtime, uid, gid, _) = atributos_de(ruta);
    let xattr = xattrs_de(ruta);
    // LO QUE ESTE FICHERO LE VA A COSTAR AL CIERRE. El esqueleto -todo lo que NO era deflate-
    // viaja EN EL INDICE, y en un PDF eso son ~125 KB por fichero. Ver `PROG_INDICE`.
    anota_indice(esqueleto.len() as u64
                 + huecos.iter().map(|h| h.correcciones.len() as u64 + h.trozos.len() as u64 * 4 + 8).sum::<u64>());
    a.idx.files.push(FileRec {
        uid, gid, xattr,
        path: rel,
        size: d.len() as u64,
        chunks: ids,
        hash: huella(d),
        transform: Some(Transform::Deflate { esqueleto, huecos }),
        mode, mtime, link_target: None, ..Default::default()
    });
    Ok(true)
}


/// Lo que `prepara_deflate` deja listo para meter. Todo son datos propios: se puede calcular en
/// un hilo y consumir en otro.
struct Preparado {
    plano: Vec<u8>,
    huecos: Vec<Hueco>,
    tramos: Vec<(usize, usize, u8)>,
    esqueleto: Vec<u8>,
    bytes_deflate: u64,
    recursivos: u64,
}

/// La union de las dos, para quien no quiera paralelismo. El comportamiento es el de siempre.
fn try_deflate(a: &mut Assembler, ruta: &Path, rel: String, d: &[u8]) -> Result<bool> {
    match prepara_deflate(d) {
        Some(pre) => mete_deflate(a, ruta, rel, d, pre),
        None => Ok(false),
    }
}

/// Prepara EN PARALELO todo el carril deflate de un lote de ficheros.
///
/// Mismo patron que `pretranscodifica_jpeg`, que lleva funcionando en el carril de JPEG: cada
/// hilo coge uno de cada N, hace el trabajo caro —que es puro— y devuelve datos propios. El
/// bucle secuencial los consume por indice, en orden, para que el archivo no cambie.
/// `saltar[i] == true` -> ese fichero ya se dio por bueno y NO hay que prepararle nada. La lista
/// llega de fuera en vez de decidirse aqui porque quien lo sabe es el bucle principal, que tiene
/// el indice de la instantanea anterior delante.
fn pre_deflate(lote: &[PathBuf], saltar: &[bool], hilos: usize) -> HashMap<usize, (Vec<u8>, Preparado)> {
    if apagado("VC_SIN_PREFLATE") { return HashMap::new(); }
    let cand: Vec<(usize, &PathBuf)> = lote.iter().enumerate()
        .filter(|(i, _)| !saltar.get(*i).copied().unwrap_or(false))
        .filter(|(_, r)| {
            let Ok(md) = r.metadata() else { return false };
            let t = md.len();
            t >= 64 && t <= PREFLATE_MAX
                && matches!(firma_de(r),
                            deflate::Formato::Gzip | deflate::Formato::Png | deflate::Formato::Zip
                            | deflate::Formato::Pdf)
        })
        .collect();
    if cand.is_empty() { return HashMap::new(); }
    let n = hilos.max(1).min(cand.len());
    let mut out = HashMap::new();
    // 🔑 AQUI ES DONDE LA BARRA SE CALLABA 17,5 s SEGUIDOS. Esta pre-pasada prepara el lote
    // ENTERO en paralelo antes de que el bucle por fichero vea uno solo, y era muda: sobre
    // codigo+pdfoffice cuesta 36,5 s de los 88 de la corrida. Nadie tocaba ningun contador
    // mientras tanto, asi que la barra se quedaba clavada hasta que el lote terminaba.
    //
    // Se apunta en un contador APARTE y el avance usa el mayor de los dos. Sumarlo al de leer
    // seria contar dos veces: estos mismos ficheros los vuelve a contar el bucle de abajo.
    PROG_PREP.store(PROG_BYTES.load(std::sync::atomic::Ordering::Relaxed),
                    std::sync::atomic::Ordering::Relaxed);
    std::thread::scope(|s| {
        let mut hs = Vec::new();
        for k in 0..n {
            let mios: Vec<(usize, &PathBuf)> = cand.iter().skip(k).step_by(n).copied().collect();
            hs.push(s.spawn(move || {
                let mut v = Vec::new();
                for (i, r) in mios {
                    let Ok(d) = std::fs::read(r) else { continue };
                    PROG_PREP.fetch_add(d.len() as u64, std::sync::atomic::Ordering::Relaxed);
                    pinta_progreso(false);
                    if let Some(pre) = prepara_deflate(&d) { v.push((i, d, pre)); }
                }
                v
            }));
        }
        for h in hs {
            if let Ok(v) = h.join() { for (i, d, pre) in v { out.insert(i, (d, pre)); } }
        }
    });
    out
}

// ---------- x86: llamadas relativas -> absolutas ----------
// En x86, CALL (E8) y JMP (E9) llevan la dirección RELATIVA a la instrucción siguiente. Eso
// significa que la MISMA llamada a la MISMA función produce bytes distintos según desde dónde
// se llame — justo lo contrario de lo que un compresor necesita. Pasándolas a absolutas, todas
// las llamadas a una función quedan idénticas y el modelo las ve como lo que son: repetición.
//
// El truco de la vuelta: la ida guarda la dirección con el byte alto PRIMERO. Así la marca
// (00 o FF, que es lo que distingue un salto plausible de un byte cualquiera) queda en la
// posición i+1 en vez de en i+4, y el camino de vuelta reconoce exactamente las mismas
// posiciones que reconoció la ida. Sin eso la transformada no sería reversible.
/// ¿Es este byte el byte alto plausible de un desplazamiento relativo? Sólo 00 y FF lo son:
/// un salto hacia delante corto empieza por 00, uno hacia atrás por FF.
#[inline]
fn alto_x86(b: u8) -> bool { b == 0 || b == 0xFF }

/// Tablas de la máscara de estado. La máscara recuerda cuáles de los tres bytes anteriores
/// eran también candidatos a E8/E9; sin esa memoria, ida y vuelta no recorren el mismo camino
/// y la transformada deja de ser reversible.
const X86_PERMITE: [bool; 8] = [true, true, true, false, true, false, false, false];
const X86_BIT: [u32; 8] = [0, 1, 2, 2, 3, 3, 3, 3];

pub fn e8e9(d: &mut [u8], adelante: bool) {
    let n = d.len();
    if n < 5 { return; }
    let limite = n - 4;
    let mut pos: usize = 0;
    let mut pos_prev: i64 = -1;
    let mut mascara: u32 = 0;
    loop {
        // saltar hasta el siguiente CALL (E8) o JMP (E9)
        while pos < limite && (d[pos] & 0xFE) != 0xE8 { pos += 1; }
        if pos >= limite { break; }
        let dist = pos as i64 - pos_prev;
        pos_prev = pos as i64;
        if dist > 3 {
            mascara = 0;
        } else {
            mascara = (mascara << (dist - 1)) & 7;
            if mascara != 0 {
                let b = d[pos + 4 - X86_BIT[mascara as usize] as usize];
                if !X86_PERMITE[mascara as usize] || alto_x86(b) {
                    mascara = ((mascara << 1) & 7) | 1;
                    pos += 1;
                    continue;
                }
            }
        }
        if alto_x86(d[pos + 4]) {
            let mut src = (d[pos + 4] as u32) << 24
                | (d[pos + 3] as u32) << 16
                | (d[pos + 2] as u32) << 8
                | d[pos + 1] as u32;
            let mut dest;
            loop {
                let p = (pos as u32).wrapping_add(5);
                dest = if adelante { src.wrapping_add(p) } else { src.wrapping_sub(p) };
                if mascara == 0 { break; }
                let i = X86_BIT[mascara as usize] * 8;
                if !alto_x86((dest >> (24 - i)) as u8) { break; }
                src = dest ^ ((1u32 << (32 - i)) - 1);
            }
            // El byte alto se reconstruye como 00 o FF según el bit 24, que es justo lo que
            // la otra dirección espera encontrar.
            d[pos + 4] = (0u8).wrapping_sub(((dest >> 24) & 1) as u8);
            d[pos + 3] = (dest >> 16) as u8;
            d[pos + 2] = (dest >> 8) as u8;
            d[pos + 1] = dest as u8;
            pos += 5;
        } else {
            mascara = ((mascara << 1) & 7) | 1;
            pos += 1;
        }
    }
}

/// ¿Merece la pena? Se cuentan las llamadas plausibles: un fichero de texto tiene alguna por
/// casualidad, un ejecutable tiene miles.
///
/// Dos cosas que costaron una medición equivocada. La primera: **no se mira el principio del
/// fichero**. Los primeros cientos de kilobytes de un ejecutable son cabecera, tablas de
/// importación y datos — el código está más adentro. Mirando el principio, `mozilla` de
/// Silesia da 25 llamadas en 256 KB y parece texto; mirando desde un tercio, da miles.
/// La segunda: el listón. `mozilla` entero tiene una llamada cada 1.459 bytes, así que un
/// umbral de "una cada 512" lo dejaba fuera aunque sea un ejecutable de manual.
// ---------------------------------------------------------------------------------------
// Carril de LOGS: el timestamp de cabecera, en delta
//
// Un log tiene una cabecera de fecha por línea y un mensaje detrás. El mensaje se repite
// muchísimo —el match model lo engancha casi gratis— pero la cabecera cambia SIEMPRE, y sus
// microsegundos son, para un modelo de contextos, ruido puro.
//
// Medido sobre 3 MB de logs reales: el fichero entero comprime a 18.698 bytes y sólo los
// timestamps a 7.406. El 40% del coste, siendo el 5% de los bytes. Pasándolos a diferencia
// con el anterior bajan a 5.818 y el total a 15.981 (-14,53%); en 10 MB, -19,35%.
//
// Se hace ESTRECHO a propósito. Sacar todos los números de la línea —la "plantilla" entera—
// se probó y sale un 12,25% PEOR: los números del mensaje pierden el contexto que los hacía
// predecibles.
// ---------------------------------------------------------------------------------------

/// `[2026-07-14T14:48:59.180146+02:00]` — 34 bytes exactos.
const TS_LARGO: usize = 34;
const M_SIN: u8 = 0x01;   // esta línea no llevaba timestamp
const M_ESC: u8 = 0x02;   // ...y además empezaba por 0x01 o 0x02 de verdad

/// Días desde 1970-01-01. Sin dependencias y sólo con enteros, como todo lo demás.
fn dias_civiles(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_de_dias(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

fn num(b: &[u8]) -> Option<i64> {
    let mut v = 0i64;
    for &c in b {
        if !c.is_ascii_digit() { return None; }
        v = v * 10 + (c - b'0') as i64;
    }
    Some(v)
}

/// Lee la cabecera de fecha si está y devuelve (microsegundos absolutos, zona horaria).
fn lee_ts(l: &[u8]) -> Option<(i64, [u8; 6])> {
    if l.len() < TS_LARGO { return None; }
    if l[0] != b'[' || l[5] != b'-' || l[8] != b'-' || l[11] != b'T'
        || l[14] != b':' || l[17] != b':' || l[20] != b'.'
        || (l[27] != b'+' && l[27] != b'-') || l[30] != b':' || l[33] != b']' { return None; }
    let (y, mo, d) = (num(&l[1..5])?, num(&l[6..8])?, num(&l[9..11])?);
    let (h, mi, sg) = (num(&l[12..14])?, num(&l[15..17])?, num(&l[18..20])?);
    let us = num(&l[21..27])?;
    num(&l[28..30])?; num(&l[31..33])?;   // la zona se valida, pero se guarda tal cual
    if mo < 1 || mo > 12 || d < 1 || d > 31 || h > 23 || mi > 59 || sg > 60 { return None; }
    let abs = (dias_civiles(y, mo, d) * 86400 + h * 3600 + mi * 60 + sg) * 1_000_000 + us;
    let mut z = [0u8; 6];
    z.copy_from_slice(&l[27..33]);
    Some((abs, z))
}

/// ¿Merece la pena? Se olfatean los primeros 256 KB: hacen falta bastantes líneas y que la
/// gran mayoría lleven cabecera de fecha.
fn parece_log_con_ts(d: &[u8]) -> bool {
    let n = d.len().min(256 * 1024);
    let mut con = 0usize;
    let mut tot = 0usize;
    for l in d[..n].split(|&c| c == b'\n') {
        tot += 1;
        if lee_ts(l).is_some() { con += 1; }
        if tot > 4096 { break; }
    }
    tot >= 64 && con * 5 >= tot * 4      // >= 80% de las líneas
}

/// Separa el fichero en tres flujos: las diferencias de tiempo, las zonas horarias, y el
/// resto de cada línea. Los dos primeros no llevan nunca un 0x00, así que sirve de separador.
pub fn log_ts(d: &[u8]) -> Vec<u8> {
    let (mut del, mut zon, mut res) = (Vec::new(), Vec::new(), Vec::new());
    let mut prev: i64 = 0;
    let mut primera = true;
    for l in d.split(|&c| c == b'\n') {
        if !primera { res.push(b'\n'); }
        primera = false;
        match lee_ts(l) {
            Some((abs, z)) => {
                if !del.is_empty() { del.push(b'\n'); }
                del.extend_from_slice((abs - prev).to_string().as_bytes());
                prev = abs;
                if !zon.is_empty() { zon.push(b'\n'); }
                zon.extend_from_slice(&z);
                let r = &l[TS_LARGO..];
                if matches!(r.first(), Some(&M_SIN) | Some(&M_ESC)) { res.push(M_ESC); }
                res.extend_from_slice(r);
            }
            None => {
                res.push(M_SIN);
                if matches!(l.first(), Some(&M_SIN) | Some(&M_ESC)) { res.push(M_ESC); }
                res.extend_from_slice(l);
            }
        }
    }
    let mut out = del;
    out.push(0);
    out.extend_from_slice(&zon);
    out.push(0);
    out.extend_from_slice(&res);
    out
}

/// La vuelta. Devuelve None si el blob no cuadra: quien llama lo trata como fallo, nunca
/// como datos.
pub fn log_ts_inv(d: &[u8]) -> Option<Vec<u8>> {
    let c1 = d.iter().position(|&c| c == 0)?;
    let c2 = c1 + 1 + d[c1 + 1..].iter().position(|&c| c == 0)?;
    let dels: Vec<&[u8]> = if c1 == 0 { Vec::new() } else { d[..c1].split(|&c| c == b'\n').collect() };
    let zons: Vec<&[u8]> = if c2 == c1 + 1 { Vec::new() } else { d[c1 + 1..c2].split(|&c| c == b'\n').collect() };
    let mut out: Vec<u8> = Vec::with_capacity(d.len() + d.len() / 4);
    let mut prev: i64 = 0;
    let mut k = 0usize;
    let mut primera = true;
    for l in d[c2 + 1..].split(|&c| c == b'\n') {
        if !primera { out.push(b'\n'); }
        primera = false;
        if l.first() == Some(&M_SIN) {
            let r = &l[1..];
            out.extend_from_slice(if r.first() == Some(&M_ESC) { &r[1..] } else { r });
            continue;
        }
        if k >= dels.len() || k >= zons.len() || zons[k].len() != 6 { return None; }
        let t = std::str::from_utf8(dels[k]).ok()?.parse::<i64>().ok()?;
        prev = prev.checked_add(t)?;
        if prev < 0 { return None; }
        let us = prev % 1_000_000;
        let seg = prev / 1_000_000;
        let (y, mo, dd) = civil_de_dias(seg / 86400);
        let t = seg % 86400;
        if !(0..10000).contains(&y) { return None; }
        out.extend_from_slice(format!(
            "[{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}", y, mo, dd, t / 3600, (t / 60) % 60, t % 60, us
        ).as_bytes());
        out.extend_from_slice(zons[k]);
        out.push(b']');
        let r = if l.first() == Some(&M_ESC) { &l[1..] } else { l };
        out.extend_from_slice(r);
        k += 1;
    }
    if k != dels.len() { return None; }
    Some(out)
}

fn try_log_ts(a: &mut Assembler, ruta: &Path, rel: String, d: &[u8]) -> Result<bool> {
    if !parece_log_con_ts(d) { return Ok(false); }
    let plano = log_ts(d);
    // La puerta de siempre: se deshace y se compara byte a byte. Si no vuelve exacto, este
    // fichero va por el camino normal. Nunca se cambia correctitud por ratio.
    match log_ts_inv(&plano) {
        Some(v) if v == d => {}
        _ => return Ok(false),
    }
    let mut ids = Vec::new();
    for (coff, clen) in { let (a,b,c) = cdc_params(); cdc::trozos(&plano, a, b, c) } {
        a.st.chunks_total += 1;
        ids.push(a.add_chunk(LANE_TEXT, &plano[coff..coff + clen])?);
    }
    a.st.in_bytes += d.len() as u64;
    a.st.files += 1;
    let (mode, mtime, uid, gid, _) = atributos_de(ruta);
    let xattr = xattrs_de(ruta);
    a.idx.files.push(FileRec {
        uid, gid, xattr,
        path: rel, size: d.len() as u64, chunks: ids, hash: huella(d),
        transform: Some(Transform::LogTs), mode, mtime, link_target: None, ..Default::default()
    });
    Ok(true)
}

fn parece_ejecutable(d: &[u8]) -> bool {
    let n = d.len();
    if n < 16 * 1024 { return false; }
    let mut c = 0usize;
    let mut i = 0usize;
    while i + 4 < n {
        if (d[i] == 0xE8 || d[i] == 0xE9) && (d[i + 4] == 0 || d[i + 4] == 0xFF) { c += 1; i += 5; } else { i += 1; }
    }
    c * 2048 > n   // ~una llamada cada 2 KB
}

/// Lee `max` bytes desde `desde`. Sirve para olfatear por el medio del fichero en vez de por
/// el principio. Comparte cuerpo con `lee_prefijo`: eran la misma funcion con un seek de
/// diferencia, el mismo bucle y el mismo defecto de reserva.
fn lee_ventana(ruta: &Path, desde: u64, max: usize) -> Option<Vec<u8>> {
    let mut f = File::open(ruta).ok()?;
    f.seek(SeekFrom::Start(desde)).ok()?;
    lee_hasta(f, max)
}

fn try_e8e9(a: &mut Assembler, ruta: &Path, rel: String, d: &[u8]) -> Result<bool> {
    if !parece_ejecutable(d) { return Ok(false); }
    let mut plano = d.to_vec();
    e8e9(&mut plano, true);
    // La puerta de siempre: se deshace y se compara byte a byte. Si no vuelve exacto, este
    // fichero va por el camino normal. Nunca se cambia correctitud por ratio.
    let mut vuelta = plano.clone();
    e8e9(&mut vuelta, false);
    if vuelta != d { return Ok(false); }
    let mut ids = Vec::new();
    for (coff, clen) in { let (a,b,c) = cdc_params(); cdc::trozos(&plano, a, b, c) } {
        a.st.chunks_total += 1;
        ids.push(a.add_chunk(LANE_OTHER, &plano[coff..coff + clen])?);
    }
    a.st.in_bytes += d.len() as u64;
    a.st.files += 1;
    let (mode, mtime, uid, gid, _) = atributos_de(ruta);
    let xattr = xattrs_de(ruta);
    a.idx.files.push(FileRec {
        uid, gid, xattr,
        path: rel, size: d.len() as u64, chunks: ids, hash: huella(d),
        transform: Some(Transform::E8e9), mode, mtime, link_target: None, ..Default::default()
    });
    Ok(true)
}

/// Lee como mucho `max` bytes del principio del fichero.
fn lee_prefijo(ruta: &Path, max: usize) -> Option<Vec<u8>> {
    lee_hasta(File::open(ruta).ok()?, max)
}

/// Lee como mucho `max` bytes de `f`, reservando por lo que HAY y no por el tope.
///
/// La version anterior hacia `vec![0u8; max]` y truncaba: para el tope de 1 MiB con el que se
/// llama desde el agrupamiento, eso es pedir —y poner a cero— un megabyte por cada fichero de
/// un arbol cuya MEDIANA son 1.972 B. Y 1 MiB pasa del umbral de mmap de glibc, asi que cada
/// fichero pagaba ademas un mmap y un munmap que no hacian falta.
///
/// La capacidad se reserva de golpe y SIN PREGUNTAR NADA: 64 KiB, que es el percentil 88 del
/// arbol real (bench/tallas.tsv). `Vec::with_capacity` no pone a cero —eso era `vec![0u8; n]`—
/// asi que apartar 64 KiB no cuesta un memset, solo sitio.
///
/// Se probo antes con un fstat sobre el descriptor para acertar la capacidad exacta. Funciona,
/// pero se paga una llamada por fichero SIEMPRE, y medido se comia la mitad de la ganancia: la
/// bajada de statx pasaba de 2,00 a 1,00 por fichero. Con la distribucion real del arbol la
/// capacidad fija sale a 2,47 llamadas por fichero contra 3,00 del fstat.
///
/// Sin esa capacidad, `read_to_end` arranca de cero y va doblando, PIDIENDO UNA LECTURA POR
/// CADA CRECIMIENTO. Medido con strace, esa version intermedia subia de 5,02 a 11,41 lecturas
/// por fichero: cambiaba dos stat por seis lecturas, o sea peor. No se vio con el reloj —seis
/// lecturas de algo que ya esta en la cache de paginas no mueven un 1,63%— sino contando.
///
/// (Y la reserva de 1 MiB no costaba mmap, como se creyo al principio: 92 mmap en total para
/// 27.162 ficheros, porque glibc sube su umbral solo. Costaba el memset: poner a cero un mega
/// por fichero son ~26 GB de escritura a lo largo del arbol.)
fn lee_hasta(f: File, max: usize) -> Option<Vec<u8>> {
    let mut v = Vec::with_capacity(max.min(64 * 1024));
    match f.take(max as u64).read_to_end(&mut v) {
        Ok(_) => Some(v),
        Err(_) => None,
    }
}

/// Reordena los ficheros dejando JUNTOS los que se parecen (ver `cdc::sketch`).
///
/// Es la palanca que zip no puede tener ni en principio: comprime cada fichero aislado, así
/// que toda la redundancia ENTRE ficheros se le escapa. Aquí, al caer los parecidos en el
/// mismo frame, el compresor la ve. El desempate por ruta mantiene juntos los del mismo
/// directorio cuando no hay señal de contenido, y hace el orden determinista.
fn agrupa_por_similitud(rutas: &mut Vec<PathBuf>, hilos: usize) {
    let n = rutas.len();
    if n < 2 { return; }
    let mut firmas: Vec<[u64; cdc::SKETCH_K]> = vec![[u64::MAX; cdc::SKETCH_K]; n];
    let por = n.div_ceil(hilos.max(1));
    {
        let ro: &[PathBuf] = rutas;
        std::thread::scope(|s| {
            for (k, trozo) in firmas.chunks_mut(por).enumerate() {
                let base = k * por;
                s.spawn(move || {
                    for (i, f) in trozo.iter_mut().enumerate() {
                        if let Some(d) = lee_prefijo(&ro[base + i], 1024 * 1024) {
                            *f = cdc::sketch(&d);
                        }
                    }
                });
            }
        });
    }
    // MEDIDO: ordenar directamente por la firma EMPEORA (código −7,7%, logs −13,5%). El
    // orden por ruta ya agrupa muy bien —mismo directorio = mismo proyecto, misma rotación
    // de logs, mismo volcado— y sustituirlo por un sketch de 4 valores lo destroza.
    //
    // Así que la similitud no sustituye al orden natural: lo REFUERZA. Se unen en grupos
    // sólo los ficheros que demostradamente comparten contenido (algún mínimo en común), y
    // cada grupo viaja junto a la posición de su primer miembro. Lo que no comparte nada
    // se queda exactamente donde estaba.
    let mut padre: Vec<usize> = (0..n).collect();
    fn raiz(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x { p[x] = p[p[x]]; x = p[x]; }
        x
    }
    let mut primero: HashMap<u64, usize> = HashMap::new();
    for i in 0..n {
        for &h in &firmas[i] {
            if h == u64::MAX { continue; }
            if let Some(&j) = primero.get(&h) {
                let (ra, rb) = (raiz(&mut padre, i), raiz(&mut padre, j));
                if ra != rb { padre[ra] = rb; }
            } else {
                primero.insert(h, i);
            }
        }
    }
    let raices: Vec<usize> = (0..n).map(|i| raiz(&mut padre, i)).collect();
    let mut ancla: HashMap<usize, usize> = HashMap::new();
    for (i, &r) in raices.iter().enumerate() { ancla.entry(r).or_insert(i); }
    let mut orden: Vec<usize> = (0..n).collect();
    orden.sort_by_key(|&i| (ancla[&raices[i]], i));
    let mut tmp: Vec<PathBuf> = Vec::with_capacity(n);
    for i in orden { tmp.push(std::mem::take(&mut rutas[i])); }
    *rutas = tmp;
}

/// Firma del fichero leyendo sólo la cabecera: barato, y no obliga a cargarlo entero
/// para descartarlo.
fn firma_de(ruta: &Path) -> deflate::Formato {
    let Ok(mut f) = File::open(ruta) else { return deflate::Formato::Otro };
    let mut cab = [0u8; 8];
    match f.read(&mut cab) {
        Ok(n) => deflate::firma(&cab[..n]),
        Err(_) => deflate::Formato::Otro,
    }
}

/// ¿Va este fichero por el carril JPEG? (extensión y tamaño dentro del rango)
fn es_candidato_jpeg(r: &Path) -> bool {
    let ext = r.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext != "jpg" && ext != "jpeg" { return false; }
    matches!(r.metadata(), Ok(m) if m.len() >= 128 && m.len() <= JXL_MAX_JPEG)
}

/// Transcodifica EN PARALELO los .jpg de un lote. El bucle de empaquetado es secuencial por
/// diseño (el Assembler no es Sync), así que el coste caro —el JXL y su verificación de
/// vuelta— se paga aquí, repartido entre hilos. Devuelve posición-en-el-lote → (jpeg original,
/// flujo JXL); lo que no reconstruye byte a byte sencillamente no aparece y cae a store.
/// Se hace por lotes para acotar la memoria: nunca hay más de JXL_LOTE ficheros en vuelo.
/// `saltar[i] == true` -> ese fichero ya se dio por bueno y no hay que transcodificarlo. Ver la
/// nota de `pre_deflate`: transcodificar un JPEG para tirarlo despues cuesta lo mismo que
/// transcodificarlo para usarlo.
fn pretranscodifica_jpeg(lote: &[PathBuf], saltar: &[bool], hilos: usize) -> HashMap<usize, (Vec<u8>, Vec<u8>)> {
    let cand: Vec<(usize, &PathBuf)> = lote.iter().enumerate()
        .filter(|(i, _)| !saltar.get(*i).copied().unwrap_or(false))
        .filter(|(_, r)| es_candidato_jpeg(r)).collect();
    if cand.is_empty() { return HashMap::new(); }
    let n = hilos.max(1).min(cand.len());
    let mut out = HashMap::new();
    std::thread::scope(|s| {
        let mut hs = Vec::new();
        for k in 0..n {
            let mios: Vec<(usize, &PathBuf)> = cand.iter().skip(k).step_by(n).copied().collect();
            hs.push(s.spawn(move || {
                let mut v = Vec::new();
                for (i, r) in mios {
                    let Ok(jpeg) = std::fs::read(r) else { continue };
                    if let Some(f) = jxl::jxl_verificado(&jpeg, JXL_ESFUERZO) { v.push((i, jpeg, f)); }
                }
                v
            }));
        }
        for h in hs {
            if let Ok(v) = h.join() { for (i, j, f) in v { out.insert(i, (j, f)); } }
        }
    });
    out
}

/// Mete por el carril media un JPEG ya transcodificado y YA verificado byte-exacto.
fn mete_jxl(a: &mut Assembler, ruta: &Path, rel: String, jpeg: &[u8], flujo: &[u8]) -> Result<()> {
    let mut ids = Vec::new();
    for (coff, clen) in { let (a,b,c) = cdc_params(); cdc::trozos(flujo, a, b, c) } {
        let seg = &flujo[coff..coff + clen];
        a.st.chunks_total += 1;
        ids.push(a.add_chunk(LANE_MEDIA, seg)?);
    }
    a.st.in_bytes += jpeg.len() as u64;
    a.st.files += 1;
    a.st.jxl_files += 1;
    a.st.jxl_orig += jpeg.len() as u64;
    a.st.jxl_out += flujo.len() as u64;
    let (mode, mtime, uid, gid, _) = atributos_de(ruta);
    let xattr = xattrs_de(ruta);
    a.idx.files.push(FileRec {
        uid, gid, xattr,
        path: rel,
        size: jpeg.len() as u64,
        chunks: ids,
        hash: huella(jpeg),
        transform: Some(Transform::JpegJxl),
        mode, mtime, link_target: None, ..Default::default()
    });
    Ok(())
}

/// `jpeg`: activa el carril JPEG→JXL. Da ~16% sobre los .jpg pero cuesta velocidad en AMBOS
/// sentidos (ver docs/f2.3-jpeg-jxl.md); apagándolo, los .jpg van por el camino normal.
/// `solido`: 0 = frames normales (navegable fino). Si no, tamaño de frame en MiB (32..256).
/// Frames grandes comprimen bastante más en ficheros grandes y repetitivos —una copia de
/// seguridad, un volcado de base de datos— porque el compresor deja de arrancar de cero cada
/// pocos MiB. Se paga en granularidad de acceso y en memoria, no en propiedades: montar y
/// extraer un fichero suelto siguen funcionando.

// ================================================================================================
// ELEGIR EL PUNTO DE TRABAJO POR DINERO  (F2.63)
// ================================================================================================

/// SOLO SE USA COMO RESPALDO. La velocidad de cada ajuste ya no se lee de aqui: la MIDE la
/// sonda sobre el dato del usuario y en su maquina. Esta lista se queda para cuando el
/// cronometro no llegue a correr, y no se toma ninguna decision con ella sin decirlo.
///
/// POR QUE DEJO DE SER LA FUENTE. Decia que -19 va a 13,58 MiB/s y -1 a 473,03: una relacion
/// de 34,8 veces. Eso es cierto PARA EL CODEC SOLO y sobre otro corpus. El archivador entero,
/// medido el 2026-07-31 sobre el arbol de 2,7 GB, va de 8,26 a 14,78 MB/s: una relacion de
/// 1,79. Los carriles cuestan lo mismo a nivel 1 que a nivel 19 y aplastan la diferencia.
///
/// Y NO SE ARREGLA CAMBIANDO UNA TABLA POR OTRA. Poner aqui las del archivador seria el error
/// simetrico: harian creer que el nivel casi no importa, cuando para el codec importa muchisimo
/// — y ademas desde hoy `--price` puede APAGAR carriles, con lo que el coste fijo que aplasta
/// la diferencia ni siquiera es constante. La unica salida honesta es medirlo cada vez.
///
/// (ajuste, MiB/s). El -1 es `--max`, que no es un nivel de zstd sino otro codec.
pub const VELOCIDAD_NIVEL: &[(i32, f64)] = &[
    (1, 473.03),   // zstd -1
    (3, 457.23),   // zstd -3
    (5, 417.08),   // zstd -5
    (7, 377.59),   // zstd -7
    (9, 327.09),   // zstd -9
    (12, 197.52),   // zstd -12
    (15, 82.09),   // zstd -15
    (17, 38.47),   // zstd -17
    (19, 13.58),   // zstd -19
    (-1, 1.00),   // --max, medido en bench/vs-restic-borg.tsv sobre el mismo arbol
];

/// Cuanto cuesta una hora de la maquina que comprime. Por defecto, la de referencia: un
/// Hetzner AX41-NVMe a 46 EUR/mes son 0,0639 EUR/hora. Se cambia con --cpu porque una maquina
/// alquilada por horas en la nube cuesta veinte veces mas, y eso cambia la respuesta.
pub const EUR_HORA_DEFECTO: f64 = 46.0 / (30.0 * 24.0);

pub struct Candidato {
    /// true si la velocidad salio del cronometro y false si salio de la tabla de respaldo. Se
    /// enseña: una cifra prestada de otra maquina no puede parecer una medida.
    pub medido: bool,
    pub nivel: i32,
    pub bytes: u64,
    pub eur_disco: f64,
    pub eur_cpu: f64,
    pub segundos: f64,
}
impl Candidato {
    pub fn total(&self) -> f64 { self.eur_disco + self.eur_cpu }
}

/// Toma una muestra del dato REAL y prueba cada ajuste sobre ella.
///
/// La muestra son los primeros `tope` bytes recorriendo el arbol en el mismo orden en que lo
/// recorre el compresor. Que sea el mismo orden importa: si la sonda mirara ficheros distintos
/// de los que se comprimen primero, estaria midiendo otra cosa.
///
/// AVISO SOBRE --max Y EL TAMAÑO DE MUESTRA. Con --max los tramos son de 32 MiB y por encima de
/// 8 MiB entra la poda de modelos. Una muestra menor de 32 MiB subestima lo que --max haria de
/// verdad; por eso se pide de 32 MiB y quien llame avisa si no llego.
/// Cuantos trozos distintos del arbol entran en la muestra. Con 64 se cubren las familias de
/// un arbol normal sin que cada trozo baje de unos cientos de KiB, que es donde zstd y el CM
/// dejan de comportarse como en un fichero de verdad.
const TROZOS_MUESTRA: usize = 64;

/// `tope_cm`: cuanto de la muestra se le da al candidato --max. Va aparte porque vc-cm a un
/// hilo va a ~0,2 MiB/s: darle los mismos 32 MiB que a zstd costaba 160 s, mas que el trabajo
/// que la sonda estaba informando. Con 0 no se sondea --max.
/// Devuelve, por ajuste: (nivel, bytes de muestra que vio, bytes que saco, MiB/s medidos).
///
/// LOS MiB/s SE MIDEN, no se leen de ninguna tabla. Ver la nota de `VELOCIDAD_NIVEL`: la que
/// habia decia que -19 es 34,8 veces mas lento que -1, que es verdad para el codec solo y sobre
/// otro corpus. Aqui se cronometra ESTE codec en ESTA maquina con ESTOS datos.
pub fn sonda_niveles(entradas: &[PathBuf], niveles: &[i32], tope: usize, tope_cm: usize)
    -> Result<Vec<(i32, u64, u64, f64)>>
{
    // A ZANCADAS por todo el arbol, no los primeros N bytes.
    //
    // Coger el principio del recorrido ordenado significa coger los ficheros alfabeticamente
    // primeros, que en un arbol real son todos de la misma familia: se sondea `codigo/` y se
    // decide para un arbol que ademas tiene logs, imagenes y volcados. Medido: la sonda por
    // el principio estimo --max en 1,94% y el trabajo real dio 2,65%.
    let mut muestra: Vec<u8> = Vec::with_capacity(tope.min(64 << 20));
    {
        // La lista de ficheros sale de TODAS las entradas: sondear solo la primera daria el
        // nivel de un fichero para un trabajo que tiene otros tres.
        let mut ficheros: Vec<PathBuf> = Vec::new();
        for input in entradas {
            if input.is_dir() {
                let mut rutas: Vec<(PathBuf, Tipo)> = Vec::new();
                recorrer(input, &mut rutas)?;
                ficheros.extend(rutas.into_iter()
                    .filter(|(_, t)| matches!(t, Tipo::Fichero))
                    .map(|(r, _)| r));
            } else { ficheros.push(input.clone()); }
        }
        if ficheros.is_empty() { bail!("la entrada no tiene ficheros que sondear"); }
        // Indices repartidos por igual sobre la lista ORDENADA: mismo arbol, misma muestra,
        // en cualquier maquina y cualquier dia. Nada de azar, que romperia el determinismo.
        let n = TROZOS_MUESTRA.min(ficheros.len());
        let por_trozo = (tope / n).max(64 << 10);
        for k in 0..n {
            if muestra.len() >= tope { break; }
            let i = k * ficheros.len() / n;
            if let Ok(d) = std::fs::read(&ficheros[i]) {
                let falta = tope - muestra.len();
                muestra.extend_from_slice(&d[..d.len().min(por_trozo).min(falta)]);
            }
        }
        // Si los ficheros elegidos eran pequenos y la muestra se quedo corta, se completa
        // recorriendo el resto: mejor una muestra llena y algo sesgada que una muestra vacia.
        if muestra.len() < tope / 2 {
            for r in &ficheros {
                if muestra.len() >= tope { break; }
                if let Ok(d) = std::fs::read(r) {
                    let falta = tope - muestra.len();
                    muestra.extend_from_slice(&d[..d.len().min(falta)]);
                }
            }
        }
    }
    if muestra.is_empty() { bail!("la entrada no tiene bytes que sondear"); }

    if std::env::var("VC_SONDA_DEBUG").is_ok() {
        eprintln!("[sonda] muestra={} B  sha={}", muestra.len(),
                  &blake3::hash(&muestra).to_hex()[..16]);
    }
    let sin_cm = std::env::var("VC_SONDA_SIN_CM").is_ok() || tope_cm == 0;
    let depurar = std::env::var("VC_SONDA_DEBUG").is_ok();
    let activos: Vec<i32> = niveles.iter().copied().filter(|&n| !(n < 0 && sin_cm)).collect();

    // EN PARALELO, y aqui si se puede. La regla de medir en serie existe para los RELOJES: dos
    // medidas de tiempo a la vez se estorban y se falsean. Esto no mide tiempo, mide BYTES —
    // y los bytes que saca un compresor de una entrada dada son los mismos lo cargada que este
    // la maquina. Cada hilo escribe en su propia ranura y el orden se conserva, asi que el
    // resultado es identico al de hacerlo en serie; solo que siete veces mas rapido.
    let mut out: Vec<(i32, u64, u64, f64)> = vec![(0, 0, 0, 0.0); activos.len()];
    let muestra_ref = &muestra;
    std::thread::scope(|es| -> Result<()> {
        let mut hilos = Vec::new();
        for (i, &n) in activos.iter().enumerate() {
            hilos.push((i, n, es.spawn(move || -> Result<(usize, usize)> {
                // El CM recibe un PREFIJO de la muestra. Como la muestra ya viene repartida
                // por todo el arbol, su prefijo tambien lo esta: se acorta el coste sin
                // volver a sesgarla.
                let trozo: &[u8] = if n < 0 { &muestra_ref[..tope_cm.min(muestra_ref.len())] }
                                   else { muestra_ref };
                let salen = if n < 0 { vc_cm::comprimir(trozo).len() }
                            else { sonda_zstd(trozo, n).context("sonda zstd")? };
                Ok((trozo.len(), salen))
            })));
        }
        for (i, n, h) in hilos {
            let (m_in, m_out) = h.join().map_err(|_| anyhow::anyhow!("la sonda del nivel {n} se cayo"))??;
            if depurar {
                eprintln!("[sonda] nivel {:>3}: muestra {} B -> {} B  ({:.3}%)",
                          n, m_in, m_out, 100.0 * m_out as f64 / m_in.max(1) as f64);
            }
            // El m_in es el del TROZO que se comprimio, no el de la muestra entera: --max usa
            // un prefijo mas corto y su ratio hay que sacarlo contra lo que de verdad vio.
            out[i] = (n, m_in as u64, m_out as u64, 0.0);
        }
        Ok(())
    })?;

    // ── Y AHORA EL RELOJ, EN SERIE Y SOBRE UN TROZO ─────────────────────────────────────────
    //
    // Los BYTES se pueden medir en paralelo —no dependen de la carga— pero el TIEMPO no: dos
    // relojes a la vez se estorban. Asi que esta pasada va de uno en uno.
    //
    // Y sobre un TROZO de la muestra, no sobre la muestra entera: la velocidad no cambia por
    // comprimir mas bytes, asi que un cuarto basta y cuesta un cuarto. La sonda no puede costar
    // mas que la decision que informa.
    let corte = (muestra.len() / 4).max(1 << 20).min(muestra.len());
    for (i, &n) in activos.iter().enumerate() {
        let trozo: &[u8] = if n < 0 { &muestra[..corte.min(tope_cm.max(1)).min(muestra.len())] }
                           else { &muestra[..corte] };
        if trozo.is_empty() { continue }
        let t = std::time::Instant::now();
        let ok = if n < 0 { let _ = vc_cm::comprimir(trozo); true }
                 else { sonda_zstd(trozo, n).is_ok() };
        let sg = t.elapsed().as_secs_f64();
        // Si no se pudo cronometrar se deja en 0 y quien llame usa el respaldo DICIENDOLO.
        if ok && sg > 0.0 { out[i].3 = trozo.len() as f64 / 1_048_576.0 / sg; }
        if depurar {
            eprintln!("[sonda] nivel {:>3}: {:.1} MiB/s medidos sobre {} B",
                      n, out[i].3, trozo.len());
        }
    }
    Ok(out)
}

/// Un carril, y lo que costo probarlo sobre una muestra de ficheros de VERDAD.
pub struct CarrilSonda {
    /// Que se apago. "completo" es la referencia contra la que se comparan los demas.
    pub nombre: &'static str,
    /// Bytes de ENTRADA que vio la muestra. Sin esto no se puede escalar a la entrada real, y
    /// escalar por el tope pedido en vez de por lo que de verdad se leyo mete un sesgo del
    /// tamaño de lo que se quedo fuera.
    pub entrada: u64,
    pub bytes: u64,
    pub segundos: f64,
}

/// Prueba la TUBERIA REAL sobre una muestra de ficheros, apagando un carril cada vez.
///
/// POR QUE NO SIRVE LA SONDA DE NIVELES PARA ESTO. Aquella junta bytes de muchos ficheros en un
/// bufer y se lo da a zstd: mide el CODEC, no el archivador. Los carriles —preflate, JXL— actuan
/// ANTES del codec y sobre FICHEROS enteros, asi que para decidirlos hay que comprimir ficheros
/// de verdad por el camino de verdad. Sin esto, `--price` no tenia con que decidir un carril, y
/// por eso decidia lo unico que la sonda vieja veia: el nivel de zstd.
///
/// LO QUE ESTA EN JUEGO, medido el 2026-07-31 sobre el arbol de 2,7 GB: preflate compra el
/// 7,073% de los bytes y cuesta el 59,2% de la CPU; JXL compra el 3,933% y cuesta el 35,4%. A
/// precio S3 los dos se pagan de sobra; a precio disco y un año de retencion los dos PIERDEN.
/// Estan siempre encendidos, asi que en disco se estaban tirando ~2.593 EUR/PB-año.
///
/// SOLO SE PRUEBAN ESOS DOS. Los otros tres carriles —e8e9, agrupar y log-ts— tienen un coste
/// POR DEBAJO DEL SUELO DE RUIDO de la maquina (2,96% medido). Decidir sobre ellos seria
/// publicar ruido como si fuera una decision, asi que se quedan como estan y se dice por que.
///
/// EN SERIE, y a proposito: aqui SI se mide tiempo, y dos relojes a la vez se estorban. La
/// sonda de niveles puede ir en paralelo porque solo mira bytes; esta no.
///
/// SOBRE LA VARIABLE DE ENTORNO. El carril preflate se apaga con `VC_SIN_PREFLATE`, que es
/// estado global del proceso. Aqui es seguro —la sonda corre antes del trabajo y en un solo
/// hilo— y ademas es justo lo que hace falta: si la sonda decide apagarlo, la variable se queda
/// puesta y el trabajo de verdad la respeta.
pub fn sonda_carriles(entradas: &[PathBuf], nivel: i32, tope: usize) -> Result<Vec<CarrilSonda>> {
    // La misma muestra a zancadas que la sonda de niveles, pero de FICHEROS ENTEROS: un carril
    // se decide por fichero, asi que media imagen no dice nada sobre el carril de imagenes.
    let mut ficheros: Vec<PathBuf> = Vec::new();
    for input in entradas {
        if input.is_dir() {
            let mut rutas: Vec<(PathBuf, Tipo)> = Vec::new();
            recorrer(input, &mut rutas)?;
            ficheros.extend(rutas.into_iter().filter(|(_, t)| matches!(t, Tipo::Fichero))
                            .map(|(r, _)| r));
        } else { ficheros.push(input.clone()); }
    }
    if ficheros.is_empty() { bail!("la entrada no tiene ficheros que sondear"); }

    let n = TROZOS_MUESTRA.min(ficheros.len());
    let mut muestra: Vec<PathBuf> = Vec::new();
    let mut bytes = 0usize;
    // Se recorre a zancadas y se da mas de una vuelta desplazando el arranque: asi, si los
    // ficheros de la primera zancada eran pequeños, la muestra se llena con OTROS repartidos y
    // no con los vecinos del primero, que serian de la misma familia.
    'llenar: for desp in 0..n {
        for k in 0..n {
            let i = (k * ficheros.len() / n + desp).min(ficheros.len() - 1);
            if muestra.contains(&ficheros[i]) { continue }
            if let Ok(md) = ficheros[i].metadata() {
                muestra.push(ficheros[i].clone());
                bytes += md.len() as usize;
                if bytes >= tope { break 'llenar }
            }
        }
    }
    if muestra.is_empty() { bail!("la muestra de carriles salio vacia"); }

    let dir = std::env::temp_dir().join(format!("vc-sonda-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let mut out = Vec::new();
    // (nombre, apagar preflate, apagar el carril PDF, hacer JXL)
    //
    // El PDF entra aqui el 2026-08-02 y entra por la puerta de siempre: NO se decide en global si
    // «compensa». Medido sobre pdfoffice (201 MB): compra el 16,62% de los bytes y cuesta 1,67
    // veces el reloj y 5,43 veces la RAM. Eso no es un veredicto, es un PUNTO del frente — y cual
    // gana depende del precio del disco y del plazo, que es justo lo que sabe el usuario y no
    // nosotros. Antes ni siquiera era elegible: el PDF iba opaco a zstd y no habia perilla.
    for (nombre, sin_preflate, sin_pdf, jpeg) in [("completo", false, false, true),
                                                  ("preflate", true, true, true),
                                                  ("pdf", false, true, true),
                                                  ("jxl", false, false, false)] {
        let destino = dir.join(format!("{nombre}.vc"));
        let _ = std::fs::remove_file(&destino);
        if sin_preflate { std::env::set_var("VC_SIN_PREFLATE", "1") }
        else { std::env::remove_var("VC_SIN_PREFLATE") }
        // ⚠️ Apagar preflate apaga el PDF TAMBIEN, y no es una eleccion de estilo: el carril de
        // PDF ES el de preflate mirando dentro de otro contenedor. Sin esto, la fila «preflate»
        // seguiria pagando el despliegue de los PDF y su coste saldria mas barato de lo que es.
        if sin_pdf { std::env::set_var("VC_SIN_PDF", "1") }
        else { std::env::remove_var("VC_SIN_PDF") }
        let t = std::time::Instant::now();
        let r = comprimir_pleno(&muestra, &destino, nivel, jpeg, true, false, 0, None, "", false);
        let seg = t.elapsed().as_secs_f64();
        std::env::remove_var("VC_SIN_PREFLATE");
        std::env::remove_var("VC_SIN_PDF");
        // Si una configuracion falla NO se inventa su fila: se devuelve lo que haya y quien
        // llame decide sin ella. Una sonda que rellena huecos decide sobre datos que no existen.
        let Ok(st) = r else { continue };
        out.push(CarrilSonda { nombre, entrada: st.in_bytes, bytes: st.out_bytes, segundos: seg });
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(out)
}

/// Con la sonda hecha, cuanto cuesta cada opcion en euros.
///
///   coste = bytes_estimados / 1e12 * precio_TB_mes * meses  +  horas_de_CPU * precio_hora
///
/// Dos terminos y ninguna sutileza: uno es un alquiler que se paga todos los meses y el otro
/// una compra que se paga una vez. Lo unico que hacia falta para poder compararlos era saber
/// cuantos meses dura el alquiler, que es justo el dato que solo tiene el usuario.
pub fn evalua_dinero(sonda: &[(i32, u64, u64, f64)], total_entrada: u64, precio_tb_mes: f64,
                     meses: f64, eur_hora: f64) -> Vec<Candidato> {
    // La velocidad MEDIDA manda. La tabla solo entra si el cronometro no llego a correr, y en
    // ese caso quien imprime la tabla lo dice: una cifra prestada de otra maquina y otro corpus
    // no puede pasar por una medida sin avisar.
    let vel = |n: i32, medida: f64| if medida > 0.0 { medida } else {
        VELOCIDAD_NIVEL.iter().find(|(k, _)| *k == n).map(|(_, v)| *v).unwrap_or(1.0) };
    sonda.iter().map(|&(n, m_in, m_out, mibs)| {
        let bytes = (total_entrada as f64 * (m_out as f64 / m_in.max(1) as f64)) as u64;
        let segundos = total_entrada as f64 / (vel(n, mibs) * 1_048_576.0);
        Candidato {
            medido: mibs > 0.0,
            nivel: n,
            bytes,
            eur_disco: bytes as f64 / 1e12 * precio_tb_mes * meses,
            eur_cpu: segundos / 3600.0 * eur_hora,
            segundos,
        }
    }).collect()
}

/// Cuanto pesa la entrada, para escalar lo que dijo la sonda.
pub fn tamano_entrada(entradas: &[PathBuf]) -> Result<u64> {
    let mut total = 0u64;
    for input in entradas { total += tamano_de_una(input)?; }
    Ok(total)
}

fn tamano_de_una(input: &Path) -> Result<u64> {
    if input.is_dir() {
        let mut rutas: Vec<(PathBuf, Tipo)> = Vec::new();
        recorrer(input, &mut rutas)?;
        Ok(rutas.iter()
            .filter(|(_, t)| matches!(t, Tipo::Fichero))
            .filter_map(|(r, _)| std::fs::metadata(r).ok())
            .map(|m| m.len())
            .sum())
    } else {
        Ok(std::fs::metadata(input)
            .with_context(|| format!("medir {}", input.display()))?
            .len())
    }
}

pub fn comprimir(input: &Path, out_path: &Path, nivel: i32, jpeg: bool, agrupar: bool, maximo: bool, solido: usize) -> Result<Stats> {
    comprimir_con_base(&[input.to_path_buf()], out_path, nivel, jpeg, agrupar, maximo, solido, None, "")
}

/// Igual que `comprimir`, pero puede CONTINUAR un archivo que ya existe.
///
/// `base`: el indice de lo que ya hay y el offset donde acaban sus frames. Con eso, los trozos
/// que ya estan no se vuelven a comprimir —se referencian— y los frames nuevos se apilan
/// detras de los viejos.
///
// ==================== NO REHACER LO QUE NO HA CAMBIADO (D12) ====================
//
// MEDIDO 2026-07-30: la segunda instantanea del MISMO arbol costaba 30,72 s contra 29,89 s la
// primera. El dedup por trozos ahorraba el 99,94% del ESPACIO y el 0% del TIEMPO, porque cada
// fichero se volvia a leer, inflar, transformar, trocear y hashear para acabar descubriendo que
// todos sus trozos ya estaban. Un respaldo diario pagaba entero el carril preflate, que es el
// 78% del reloj. Eso es la familia 1 —«no hacer el trabajo»— y no la habiamos tocado.
//
// COMO SE DECIDE QUE UN FICHERO NO HA CAMBIADO: por (talla, mtime), que es lo que hacen restic,
// borg y tar desde siempre. NO es una huella, es una apuesta, y hay que decir exactamente cual:
//
//   LO QUE SI SE COGE. Contenido distinto con talla distinta, o con la misma talla pero escrito
//   en otro segundo. Es el caso normal y es el 100% de lo que pasa sin querer.
//
//   LO QUE SE ESCAPA. Reescribir un fichero conservando EXACTAMENTE la misma talla y devolviendo
//   el mtime antiguo a mano (`touch -d`). Eso enga~na a cualquier respaldo incremental del
//   mercado, y hay una prueba que lo hace a proposito para que quede medido y no supuesto.
//
//   EL SEGUNDO EN CURSO. Nuestro mtime tiene resolucion de SEGUNDO. Un fichero escrito en el
//   mismo segundo en que lo miramos puede volver a escribirse DENTRO de ese segundo sin que el
//   mtime cambie, y entonces seria invisible para siempre, no solo esta vez. Por eso no se salta
//   nada cuyo mtime sea el segundo actual o posterior. Es la guardia que git llama «racily clean».
//
// Y NO ES SILENCIOSO: se cuentan los saltados y se dicen al terminar. `VC_SIN_INCREMENTAL=1` lo
// apaga entero y se vuelve al comportamiento de antes.

/// Los ficheros de las instantaneas anteriores, indexados para poder preguntar «¿este ya estaba,
/// igual?» sin recorrer el indice entero por cada fichero.
///
/// LO UNICO DELICADO ES LA RUTA. Una instantanea es un PREFIJO que puso `add`, asi que el mismo
/// fichero figura en el indice como `lunes/x/y.txt` y ahora llega como `x/y.txt`. Por eso la
/// clave es (talla, mtime) —que no obliga a construir ninguna cadena para preguntar— y despues
/// se COMPRUEBA que la ruta guardada termina en la que llega, en un limite de `/`. Sin esa
/// segunda comprobacion `a/notas.txt` casaria con `b/notas.txt` y se restauraria el fichero
/// equivocado.
struct Previos { tabla: HashMap<(u64, i64), Vec<u32>> }

impl Previos {
    /// Se construye UNA vez por ejecucion, no una por fichero.
    ///
    /// Los enlaces —duros y simbolicos— se quedan fuera: apuntan a otras entradas del indice y
    /// reusarlos a ciegas seria reusar una referencia que quiza ya no significa lo mismo. Son
    /// pocos y su camino normal ya es barato.
    fn de(files: &[FileRec]) -> Previos {
        let mut tabla: HashMap<(u64, i64), Vec<u32>> = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            if f.enlace_a.is_some() || f.link_target.is_some() { continue }
            tabla.entry((f.size, f.mtime)).or_default().push(i as u32);
        }
        Previos { tabla }
    }

    /// `rel0` es la ruta relativa SIN el prefijo de esta instantanea.
    fn busca(&self, files: &[FileRec], rel0: &str, size: u64, mtime: i64) -> Option<u32> {
        for &j in self.tabla.get(&(size, mtime))? {
            if coincide(&files[j as usize].path, rel0) { return Some(j) }
        }
        None
    }
}

/// ¿La ruta `guardada` es este mismo fichero `rel0`, visto desde otra instantanea?
///
/// Vale si es identica, o si termina en `/rel0` — lo de delante es el prefijo que puso `add`. El
/// limite en `/` es justo lo que impide que `notas.txt` case con `mis-notas.txt`.
fn coincide(guardada: &str, rel0: &str) -> bool {
    if guardada == rel0 { return true }
    guardada.len() > rel0.len()
        && guardada.as_bytes()[guardada.len() - rel0.len() - 1] == b'/'
        && guardada.ends_with(rel0)
}

/// `prefijo`: se antepone a la ruta de cada fichero. Es lo que separa una instantanea de otra
/// sin necesidad de inventar una estructura nueva en el indice.
#[allow(clippy::too_many_arguments)]
pub fn comprimir_con_base(entradas: &[PathBuf], out_path: &Path, nivel: i32, jpeg: bool, agrupar: bool,
                          maximo: bool, solido: usize, desde: Option<(Index, u64)>,
                          prefijo: &str) -> Result<Stats> {
    comprimir_pleno(entradas, out_path, nivel, jpeg, agrupar, maximo, solido, desde, prefijo, false)
}

/// El de verdad. `flujo`: leer de la entrada estandar en vez de recorrer un arbol.
#[allow(clippy::too_many_arguments)]
/// Los patrones a excluir, separados por '\n'. Viajan por variable de entorno a proposito: la
/// alternativa era anadir un parametro mas a `comprimir_pleno`, que ya tiene diez y lo llaman
/// cuatro envolturas. Un parametro numero once habria obligado a tocar todas para algo que solo
/// consulta el recorrido.
fn exclusiones() -> Vec<String> {
    std::env::var("VC_EXCLUDE").ok()
        .map(|v| v.split('\n').filter(|p| !p.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

// ============================ EL CAMINO DEL BYTE, EN ORDEN ============================
//
// Un perfil agrega por funcion y responde «donde se va el tiempo». La pregunta era otra:
// «¿donde se pierde velocidad POR PRIMERA VEZ?». Para eso hay que cronometrar las fases en el
// ORDEN en que las recorre un byte, y esa es la unica forma de ver que se pierde antes incluso
// de empezar a comprimir.
pub const FASES: [&str; 18] = ["recorrer", "agrupar", "stat", "enrutar", "jxl", "transformar",
                               "leer+trocear", "hash", "dedup+copiar", "esperar", "meta+xattr",
                               "comprimir", "escribir", "cerrar",
                               "  ...preflate", "  ...log-ts", "  ...E8E9", "  ...pre-deflate"];
pub const F_RECORRER: usize = 0;
pub const F_AGRUPAR: usize = 1;
pub const F_STAT: usize = 2;
pub const F_ENRUTAR: usize = 3;
pub const F_JXL: usize = 4;
pub const F_TRANSF: usize = 5;
pub const F_TROCEAR: usize = 6;
pub const F_HASH: usize = 7;
pub const F_DEDUP: usize = 8;
pub const F_ESPERAR: usize = 9;
pub const F_META: usize = 10;
// Estas tres se cronometran DENTRO del hilo que hace el trabajo, no en el principal. Sin ellas
// la piramide tendria un hueco que solo se podria rellenar restando, y restar no es medir.
pub const F_COMPRIMIR: usize = 11;
pub const F_ESCRIBIR: usize = 12;
pub const F_CERRAR: usize = 13;
// 🔑 LAS CUATRO QUE FALTABAN, Y POR QUE IMPORTAN MAS QUE PARECER DETALLE.
//
// Hasta el 02-08 las SIETE llamadas de preflate, log-ts y E8E9 compartian F_TRANSF, asi que el
// desglose directo se paraba en «transformar» y el reparto de dentro solo se podia sacar
// RESTANDO dos ablaciones. Eso es el nivel 3 de la jerarquia —restar dos totales buscando una
// parte pequena— y dio lo que da siempre: en la tanda del 30-07, E8E9 salio con coste NEGATIVO
// (sin-e8e9 2.090,7 G ciclos contra 2.049,7 G del completo) y log-ts tambien. La medida era
// correcta y la conclusion imposible: dos billones de ciclos a carga 6-10 no dejan ver una pieza
// pequena.
//
// Con estos cuatro contadores la misma pregunta pasa a nivel 1: se mide cada pieza DIRECTAMENTE,
// una pasada, sin restar nada.
//
// Se cuentan APARTE y no suman al total: son un desglose DE DENTRO de «transformar», y sumarlos
// contaria el mismo tiempo dos veces. Por eso llevan sangria en el nombre.
pub const F_PREFLATE: usize = 14;
pub const F_LOGTS: usize = 15;
pub const F_E8E9: usize = 16;
pub const F_PREDEFLATE: usize = 17;

static NANOS: [std::sync::atomic::AtomicU64; FASES.len()] =
    [const { std::sync::atomic::AtomicU64::new(0) }; FASES.len()];
static VECES: [std::sync::atomic::AtomicU64; FASES.len()] =
    [const { std::sync::atomic::AtomicU64::new(0) }; FASES.len()];
static FASES_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn pon_fases(si: bool) {
    FASES_ON.store(si, std::sync::atomic::Ordering::Relaxed);
}

/// Cronometra una fase. Apagado cuesta leer un bool atomico, que no se nota.
#[inline]
fn cron<T>(fase: usize, op: impl FnOnce() -> T) -> T {
    use std::sync::atomic::Ordering::Relaxed;
    if !FASES_ON.load(Relaxed) { return op(); }
    let t = std::time::Instant::now();
    let r = op();
    NANOS[fase].fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
    VECES[fase].fetch_add(1, Relaxed);
    r
}

/// Las fases, en orden, con lo que cuesta cada una y CUANTAS VECES se paga.
///
/// Las veces importan tanto como los segundos: una fase que cuesta poco pero se paga una vez por
/// fichero es lo que hace que un arbol de 43.000 ficheros no se parezca en nada a uno de 656,
/// y es justo lo que no se veia midiendo sobre el pequeño.
pub fn pinta_fases(bytes: u64, ficheros: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    if !FASES_ON.load(Relaxed) { return; }
    let tot: u64 = NANOS.iter().map(|a| a.load(Relaxed)).sum();
    if tot == 0 { return; }
    eprintln!();
    eprintln!("EL CAMINO DEL BYTE — {} ficheros, {:.1} MiB", ficheros, bytes as f64 / 1048576.0);
    eprintln!("{:<14}{:>10}{:>8}{:>12}{:>12}", "fase", "seg", "%", "veces", "us/vez");
    let mut acum = 0.0;
    for (i, n) in FASES.iter().enumerate() {
        let ns = NANOS[i].load(Relaxed) as f64;
        let v = VECES[i].load(Relaxed);
        let pct = 100.0 * ns / tot as f64;
        acum += pct;
        eprintln!("{:<14}{:>10.2}{:>7.1}%{:>12}{:>12.1}",
                  n, ns / 1e9, pct, v, if v > 0 { ns / 1000.0 / v as f64 } else { 0.0 });
    }
    eprintln!("{:<14}{:>10.2}{:>7.1}%", "TOTAL (hilo principal)", tot as f64 / 1e9, acum);
    eprintln!("  Las tres ultimas ocurren en OTROS hilos: su tiempo se solapa con el del principal,");
    eprintln!("  asi que el TOTAL de esta tabla no es el reloj, es la suma del trabajo hecho.");
    eprintln!("  «esperar» es lo que el hilo principal espera a los de compresion. Si es casi cero,");
    eprintln!("  los hilos de compresion NO son el cuello: no se les llega a dar trabajo.");
}

/// ¿Esta apagado este carril de transformada? Solo para ABLACION del banco: se apaga la pieza y
/// se miden los ciclos, que es la unica forma de saber lo que cuesta.
///
/// Por entorno y no por bandera a proposito: apagar preflate EMPEORA el archivo -es lo que nos
/// da el 39,8% sobre 7-Zip en PNG- y ofrecerlo en --help seria invitar a dispararse en el pie.
fn apagado(que: &str) -> bool {
    std::env::var(que).map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
}

/// Cuantos ficheros hay que procesar y cuantos van. Solo para pintar el progreso.
static PROG_HECHOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PROG_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PROG_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Bytes que la PRE-PASADA del carril deflate lleva preparados. Ver la nota larga en
/// `pre_deflate`: es la fase que se comia 17,5 s de barra callada.
static PROG_PREP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 🚨 EL AVANCE MEDIA LO QUE SE LEE, Y LEER ES VEINTE VECES MAS RAPIDO QUE COMPRIMIR.
///
/// Medido el 03-08 sobre 458 MiB en el Mac: las 61 lineas de avance salen TODAS en los primeros
/// **1,23 s** y llegan al 100%… mientras la corrida entera tarda **26,0 s**. O sea que la barra
/// se planta en el 100% al 5% del trabajo y ahi se queda veinticinco segundos. Lo destapo probar
/// el tiempo estimado de la app: la estimacion no llegaba a dispararse NUNCA, y el motivo no era
/// la formula sino que a los 2 s ya no quedaba nada que estimar.
///
/// La causa: `PROG_BYTES` se suma cuando el fichero se LEE (el bucle del lote), y comprimir pasa
/// despues, en el pool de trabajadores. El bucle de lectura corre por delante y termina enseguida.
///
/// Estos dos cuentan el trabajo DE VERDAD: el plano que entra al pool y el que sale comprimido.
/// El denominador crece mientras se lee -o sea, durante ese primer segundo el porcentaje da
/// saltos- y a partir de ahi es firme. Es infinitamente mejor que un 100% que dura el 95% del
/// tiempo.
///
/// ⚠️ NO se puede usar el tamaño de los ficheros como denominador: el carril deflate EXPANDE
/// (645 MiB de DEFLATE se vuelven 10.230 de plano), la dedup QUITA, y lo que el salto incremental
/// reutiliza no pasa por aqui. El unico denominador honesto es el plano que de verdad se encola.
static PROG_PLANO_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PROG_PLANO_HECHO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 🔑 EL CIERRE ES EL 13,6% DE LA CORRIDA Y LA BARRA NO LO CONTABA: EL INDICE, EN UN SOLO HILO.
///
/// Comprimir el indice es la ultima cosa que pasa y no es gratis: zstd-19 (o el CM con --max)
/// sobre todas las rutas, todos los trozos Y —lo que no se veia— **los esqueletos**. Un fichero
/// que pasa por el carril DEFLATE guarda en el indice todo lo que NO era deflate (`Transform::
/// Deflate { esqueleto, .. }`), y en un PDF eso es casi todo el fichero: medido el 03-08, 271
/// PDF anaden 34 MB de indice, 125 KB cada uno. Ahi salen los 11,3 s de 82,7 en los que la
/// barra decia 100% sin haber terminado.
///
/// ⚠️ NO SE PUEDE PREVER CON UNA FORMULA. Se probo con (ficheros, bytes) y la misma constante
/// falla por dos ordenes de magnitud segun el arbol: el indice va del 0,0046% del plano en
/// video al 3,69% en codigo, y los esqueletos del PDF lo disparan aun mas. Asi que no se
/// estima: se CUENTA segun se construye, en los dos sitios donde nace (un registro de trozo y
/// un esqueleto), y al final se cambia por el numero exacto.
static PROG_INDICE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// ¿La cuenta del indice ya es el numero de verdad? Desde ese momento no se suma nada mas.
static PROG_INDICE_FIRME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// ¿El indice ya esta comprimido? Entonces —y solo entonces— su peso cuenta como hecho.
static PROG_INDICE_LISTO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Cuanto pesa en la barra un byte de indice: por defecto, el numero de hilos.
///
/// No es un capricho: el indice lo comprime UN hilo mientras los otros once miran, asi que por
/// segundo de reloj un byte de indice cuesta `hilos` veces lo que cuesta uno de frame. MEDIDO
/// sobre codigo+pdfoffice, error medio de la barra contra el reloj: con peso 1, 18,3 puntos;
/// con peso = hilos, 15,6. `VC_PESO_INDICE` lo cambia sin recompilar, que es lo minimo que se
/// le puede pedir a una constante que decide lo que se le promete al usuario.
static PESO_INDICE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// 🔑 LOS FRAMES QUE SE ESTAN COMPRIMIENDO AHORA MISMO, para que la barra se mueva DENTRO de uno.
///
/// El pool solo puede mover la barra cuando un frame ENTERO termina, y en la corrida medida son
/// 28 frames de ~6 s cada uno: entre los 44,2 s en que se acaba de leer y los 75,5 s en que
/// termina el pool, la barra dio TRES saltos y se callo 17,2 s seguidos. Para un tiempo estimado
/// eso es no tener datos.
///
/// ⚠️ Esto es una ESTIMACION, y la unica del avance: el frame que lleva `t` segundos en el horno
/// se cuenta como `t x ritmo` bytes hechos, donde el ritmo sale de los frames YA terminados
/// (bytes por milisegundo y por hilo). Nunca pasa del tamaño del frame, asi que no puede
/// prometer mas de lo que hay. zstd no tiene forma de decir por donde va, y trocear la llamada
/// para preguntarselo cambiaria los bytes del archivo: eso si que no.
const VUELO_MAX: usize = 128;
static VUELO_BYTES: [std::sync::atomic::AtomicU64; VUELO_MAX] =
    [const { std::sync::atomic::AtomicU64::new(0) }; VUELO_MAX];
static VUELO_MS: [std::sync::atomic::AtomicU64; VUELO_MAX] =
    [const { std::sync::atomic::AtomicU64::new(0) }; VUELO_MAX];
/// Milisegundos de hilo gastados en frames ya terminados. Con `PROG_PLANO_HECHO` da el ritmo.
static PROG_MS_HILO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Cuando empezo a comprimirse el indice (ms desde el arranque), o 0 si todavia no. El cierre es
/// otro frame en el horno: ver `cuanto_lleva`.
static VUELO_INDICE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// QUE FRACCION DE UN BLOQUE LLEVA HECHA UNO QUE LLEVA `t` MILISEGUNDOS Y ESPERABA TARDAR `e`.
///
/// La primera version era `min(1, t/e)` y MEDIDO se comporta fatal: el ritmo medio es el de los
/// frames faciles, los ultimos son los dificiles, y la cuenta llegaba al tope enseguida. La
/// barra se quedo clavada en el 76% durante 32 s y luego dio un salto. Con
///     1 - e^(-t/e)
/// se llega al 63% en el tiempo esperado, al 86% en el doble y al 95% en el triple, sin tocar
/// nunca el 100%: si tarda mas de lo previsto la barra SIGUE moviendose, cada vez mas despacio,
/// y el 100% solo lo pone quien de verdad termina. Es la unica estimacion del avance y no puede
/// prometer que algo esta hecho.
fn cuanto_lleva(t: f64, esperado: f64) -> f64 {
    if esperado <= 0.0 { return 0.0 }
    1.0 - (-t / esperado).exp()
}
/// ¿Se acabo todo? Lo lee el hilo del latido para irse a dormir. Ver `late_mientras_trabaja`.
static PROG_FIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// EL LATIDO: alguien que pinta cada poco AUNQUE no termine nada.
///
/// Los tres emisores del avance —el que enruta, el pool y el cierre— solo pintan cuando terminan
/// algo, y hay fases que no terminan nada durante mucho rato: un PDF grande en la pre-pasada,
/// un frame de 6 s, el indice. Medido: 17,2 s sin una sola linea. Con el latido, la barra se
/// refresca cada 300 ms y ademas recoge lo que llevan los frames en el horno (`VUELO_BYTES`),
/// que es lo que hace que el numero se MUEVA y no solo se repita.
///
/// Se para sola al soltarse: si `comprimir` se va por un error, no queda un hilo pintando encima
/// del mensaje.
struct Latido(Option<std::thread::JoinHandle<()>>);
impl Latido {
    fn arranca() -> Latido {
        PROG_FIN.store(false, std::sync::atomic::Ordering::Relaxed);
        Latido(Some(std::thread::spawn(|| {
            while !PROG_FIN.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
                pinta_progreso(false);
            }
        })))
    }
    fn para(&mut self) {
        PROG_FIN.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.0.take() { let _ = h.join(); }
    }
}
impl Drop for Latido {
    fn drop(&mut self) { self.para(); }
}

/// DONDE SE CALLA LA BARRA. `VC_TRAZA=1` sella con el reloj el paso por cada fase del lote, que
/// es la unica forma de saber si un silencio de trece segundos es una fase muda o un cuelgue.
fn traza(que: &str) {
    // El valor se lee UNA vez: esto se llama tres veces por lote y hay miles de lotes.
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("VC_TRAZA").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)) {
        let t = PROG_ARRANQUE.get_or_init(std::time::Instant::now).elapsed();
        eprintln!("[traza {:7.2}s] {}", t.as_secs_f64(), que);
    }
}

/// Anota lo que un trozo de indice recien nacido va a costar. Barato: una suma atomica.
fn anota_indice(bytes: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    if !PROG_INDICE_FIRME.load(Relaxed) { PROG_INDICE.fetch_add(bytes, Relaxed); }
}

/// Cambia la cuenta por el numero REAL, ya conocido. A partir de aqui el denominador es exacto.
fn prevision_indice_firme(bytes: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    PROG_INDICE.store(bytes, Relaxed);
    PROG_INDICE_FIRME.store(true, Relaxed);
}
/// Cuando se pinto por ultima vez (ms desde el arranque del proceso). Los N trabajadores pintan
/// a la vez y sin esto la salida se entrelaza y se inunda: un frame de 8 MiB se comprime en
/// decimas y son doce hilos.
static PROG_ULT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PROG_ARRANQUE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
/// ¿Esto es un `compress`, o sea con pool de compresion detras? Entonces el trabajo son las tres
/// partes -enrutar, comprimir, cerrar- y el porcentaje sale del modelo de `pinta_progreso`.
///
/// Apagado -listar, verificar, extraer- no hay plano ni indice que contar y manda lo LEIDO, que
/// ahi si describe el trabajo. Antes esta bandera servia ademas para callar al bucle de lectura,
/// porque los dos emisores calculaban cosas distintas y el numero RETROCEDIA al cambiar de
/// emisor («34 MiB, 69 MiB, 0 MiB…»). Ya no hace falta callar a nadie: ahora los dos calculan lo
/// mismo, y el que enruta es el unico que puede mover la barra durante la primera fase.
static PROG_POOL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Cuantos bytes hay EN TOTAL. El porcentaje sale de aqui y no del numero de ficheros: cuatro
/// videos de 40 MB y un README no son cinco quintos del trabajo.
static PROG_TOTAL_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// EL PLANO QUE SE ESPERA DE TODO EL ARBOL, previsto POR TIPO antes de leer nada.
///
/// 🚨 POR QUE. El denominador de la barra se DESCUBRIA durante la corrida: contaba el plano
/// que ya habia aparecido mas `lo que queda por leer x 1.0`, o sea apostaba a que cada byte
/// sin leer daria UN byte de plano. Medido el 03-08 sobre un arbol mixto de 799 MB, eso NO
/// es cierto ni de lejos:
///
/// ```text
///     pdfoffice 7,58    png 1,70 a 15,72    codigo 1,05    jpg 0,75    csvjson 0,54
/// ```
///
/// Cuando entraba el lote de PDF el denominador pegaba un salto y el porcentaje se caia:
/// **del 93% al 70% en seis segundos**. Y la app de macOS calcula con eso lo que falta, asi
/// que decia «unos segundos» cuando faltaban 55 — error mediano del 60%.
///
/// ⚠️ NO ES EXTRAPOLAR LA PROPORCION OBSERVADA. Eso ya se probo y esta arriba como negativo:
/// al entrar el PDF la proporcion se disparaba y se aplicaba TAMBIEN al codigo que faltaba,
/// y la barra se caia del 84% al 46%. Aqui cada byte cuenta por lo suyo DESDE EL ARRANQUE,
/// asi que el lote de PDF no trae ninguna sorpresa: ya estaba contado por 7,58.
static PROG_PREVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// De la prevision de arriba, la parte cuyos ficheros YA se han enrutado. Lo que queda
/// pendiente es la resta, y esa es la unica parte del denominador que todavia es promesa.
static PROG_PREVISION_HECHA: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Cuanto plano da cada byte de entrada, por tipo, en 1/256.
///
/// En enteros y no en coma flotante porque esto se llama una vez por fichero en el camino
/// del progreso. Los valores salen de medir el corpus (`bench/ratios.sh`), no de estimarlos.
///
/// ⚠️ LA EXTENSION NO SIEMPRE BASTA, Y SE SABE: el mismo `.png` dio 1,70 en un corpus de
/// iconos y 15,72 en uno de fotos grandes — un PNG chico casi no tiene IDAT que inflar. Se
/// pone un valor intermedio y se acepta el error, porque **cualquier valor de esa horquilla
/// es mejor que el 1,00 que habia**. Lo que de verdad lo arreglaria es aprender la
/// proporcion POR EXTENSION sobre la marcha; esto es el escalon anterior, y se mide antes de
/// decidir si hace falta el siguiente.
fn plano_por_byte_256(ruta: &std::path::Path) -> u64 {
    let ext = ruta.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        // Contenedores con DEFLATE dentro: es lo que de verdad rompia la barra.
        "pdf" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub"
        | "zip" | "jar" | "apk" | "gz" | "tgz" => 1940,   // 7,58 medido
        "png" => 2048,                                        // 8,0: mitad de la horquilla
        "jpg" | "jpeg" => 192,                                // 0,75 medido
        "json" | "csv" | "lock" | "ndjson" => 138,            // 0,54 medido
        _ => 256,                                              // 1,00, como antes
    }
}
/// --progress: pintar el avance aunque la salida no sea un terminal.
static PROG_FORZAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enciende el avance para quien NO es un terminal — una interfaz grafica que lee por tuberia.
///
/// Existe porque la app leia unas lineas de avance que nunca salian: el emisor estaba detras de
/// un `isatty`, y una tuberia no es un terminal. La barra se quedaba a cero y en «preparando...»
/// toda la ejecucion, que para un trabajo de varios minutos es indistinguible de estar colgado.
pub fn pon_progreso(si: bool) {
    PROG_FORZAR.store(si, std::sync::atomic::Ordering::Relaxed);
}

/// Pinta el avance, pero SOLO si la salida de error es un terminal.
///
/// En un cron, o con la salida redirigida a un fichero, una barra que se repinta cada pocos
/// ficheros llena el log de basura y hace ilegible el error que de verdad importaba. Y va por
/// la salida de ERROR para que `vcompresor list | grep` siga funcionando.
fn pinta_progreso(forzar: bool) {
    use std::sync::atomic::Ordering::Relaxed;
    let a_tuberia = PROG_FORZAR.load(Relaxed);
    if !a_tuberia && unsafe { libc::isatty(2) } == 0 { return; }
    let hechos = PROG_HECHOS.load(Relaxed);
    let total = PROG_TOTAL.load(Relaxed);
    if total == 0 { return; }
    // Uno de cada 64 ficheros esta bien para un arbol de 43.000 y es INUTIL para uno de cuatro:
    // no llega a pintar ni una sola vez y la barra se queda a cero de principio a fin. El paso
    // sale del tamaño del trabajo, con tope de 64 para no inundar un arbol enorme.
    let paso = (total / 100).clamp(1, 64);
    // EL FRENO DE LOS TRABAJADORES. Los llamantes son dos: el bucle de lectura, que viene con su
    // `paso` por ficheros, y los N hilos del pool, que no tienen ficheros que contar. Para estos
    // el freno es el RELOJ: como mucho una linea cada 200 ms, o doce hilos entrelazan la salida.
    let arranque = *PROG_ARRANQUE.get_or_init(std::time::Instant::now);
    let plano_total = PROG_PLANO_TOTAL.load(Relaxed);
    let plano_hecho = PROG_PLANO_HECHO.load(Relaxed);
    if !forzar {
        // El paso por ficheros es del bucle de lectura; los otros dos emisores -la pre-pasada y
        // el pool- no tienen ficheros que contar y solo les frena el reloj.
        if hechos % paso != 0 && plano_hecho == 0 && PROG_PREP.load(Relaxed) == 0 { return; }
        let ahora_ms = arranque.elapsed().as_millis() as u64;
        let ult = PROG_ULT_MS.load(Relaxed);
        if ahora_ms < ult + 200 { return; }
        PROG_ULT_MS.store(ahora_ms, Relaxed);
    }
    let bytes = PROG_BYTES.load(Relaxed);
    let total_b = PROG_TOTAL_BYTES.load(Relaxed);
    // 🚨 DIVIDIR EL PLANO HECHO ENTRE EL PLANO ENCOLADO HACIA QUE LA BARRA FUERA HACIA ATRAS.
    //
    // Medido el 03-08 sobre 478 MiB (codigo + pdfoffice, 83,0 s): 25,7 s MUDA -el 31% de la
    // corrida sin una sola linea- y luego 83% → 64% → 92%. El numerador y el denominador crecen
    // los DOS, y cuando el que crece es el de abajo el porcentaje retrocede. Una barra que
    // retrocede es peor que una que no se mueve: la de la app calcula el ritmo con la derivada.
    //
    // EL MODELO: cada byte de entrada se paga DOS VECES, una al enrutarlo -leer, firmar, inflar
    // el DEFLATE, trocear, hashear- y otra al comprimir el plano que produce. Asi que
    //     trabajo = enrutar (bytes de entrada) + comprimir (plano) + cerrar (el indice)
    // y las tres partes se miden en la misma unidad, bytes, que es lo unico que se puede sumar.
    //
    // Lo que no se ha leido todavia no se sabe cuanto plano dara, asi que se PREDICE con la
    // proporcion observada hasta ahora. Al terminar de leer, la prediccion vale cero y el
    // denominador es exacto: la barra llega al 100% justo cuando acaba, no antes ni despues.
    //
    // ⚠️ Es una PREVISION, y por eso puede corregirse a la baja: si la segunda mitad del arbol
    // resulta ser PDF -que infla 16 a 1- el denominador crece y el porcentaje baja un poco. Lo
    // que ya no puede es empezar en cero y crecer sin freno, que era el fallo.
    // El MAYOR de los dos, no la suma: la pre-pasada del carril deflate va por delante del bucle
    // por fichero y luego el bucle la alcanza. Sumarlos contaria dos veces los mismos ficheros.
    let enrutado = bytes.max(PROG_PREP.load(Relaxed)).min(total_b);
    let queda_por_leer = total_b.saturating_sub(enrutado);
    // ❌ NEGATIVO MEDIDO (03-08): CONTAR EL TRABAJO EN SEGUNDOS EN VEZ DE EN BYTES. NO LO REPITAS.
    //
    // El reparto en bytes tiene un defecto real: un byte de enrutar y uno de comprimir no cuestan
    // lo mismo, y la proporcion la pone el arbol. Sobre codigo+pdfoffice el 55% del reloj es
    // enrutar (preflate); sobre 622 MB de video, el 80% es comprimir — y ahi la barra llegaba al
    // 70% a los 10,8 s con 43 s por delante. Asi que se probo a medir las dos velocidades en vivo
    // y a hacer la cuenta en segundos: `queda = max(lo que falta de enrutar, lo que falta de
    // comprimir) + el indice`, con el porcentaje como `transcurrido / (transcurrido + queda)`.
    //
    // Con la velocidad MEDIA de toda la corrida: la barra se queda entre el 52% y el 65% durante
    // 37 s y el motor dice «faltan 25 s» cuando faltan 5 — la media mezcla la fase rapida con la
    // lenta y no es de ninguna de las dos.
    //
    // Con una VENTANA MOVIL de 4-8 s, que es el arreglo de libro para eso: peor todavia. Los
    // frames tardan ~6 s, asi que hay ventanas enteras en las que no termina NADA, la velocidad
    // se va casi a cero y lo que queda se dispara — llego a decir **«faltan 1.791.420 s»** y la
    // barra bajo del 70% al 0%. Con sucesos de seis segundos no hay ventana buena: corta, la
    // sacude cada frame; larga, no se entera de que ha cambiado la fase.
    //
    // Lo de los bytes no es exacto, pero es MONOTONO y no puede explotar. Un porcentaje que se
    // queda corto es un defecto; uno que dice 1.791.420 segundos es una averia.

    // ⚠️ Y LO QUE FALTA POR LEER SE CUENTA UNO A UNO, NO EXTRAPOLANDO LA PROPORCION OBSERVADA.
    //
    // Extrapolar parecia lo listo -«si hasta ahora cada byte ha dado 1,8 de plano, los que
    // quedan daran lo mismo»- y MEDIDO fue peor: cuando el lote de PDF entra, la proporcion se
    // dispara, se aplica a TODO lo que queda por leer y el denominador pega un salto. La barra
    // se cayo del 84% al 46% en dos segundos. Con la proporcion fija en 1 el denominador solo
    // crece con el plano que de verdad aparece, que es un retroceso pequeño y honesto.
    // ⚠️ `ratio` MURIO: era el 1.0 a ciegas que hacia saltar el denominador. Lo que queda por
    // leer ya viene previsto POR TIPO desde el arranque (ver `PROG_PREVISION`), asi que aqui
    // solo hay que restar lo ya consumido.
    // 🚨 LA PREVISION SE USA COMO SUELO, NO COMO RESTA. Y esto no es un detalle: restandola,
    // la barra seguia cayendo del 93% al 67% y el error mediano del tiempo se quedo en el 62%
    // -contra el 60% de antes-, o sea que la prevision por tipo NO ARREGLO NADA. Medido.
    //
    // El porque lo dio separar las dos causas posibles en vez de probar otra cosa: la prevision
    // ACIERTA (1.857 MiB previstos contra 1.831 reales, +1,4% sobre un arbol mixto de 629 MiB).
    // Lo que fallaba era CUANDO se consume. `PROG_PREVISION_HECHA` sube al LEER el fichero, y
    // `PROG_PLANO_TOTAL` sube al CERRAR EL FRAME, que es mucho despues. En la ventana entre las
    // dos, la prevision ya salio del denominador y el plano todavia no ha entrado: el
    // denominador se hunde, el porcentaje sube de mas, y cuando el frame por fin se cierra pega
    // el salto hacia atras. Restar dos cosas que no ocurren a la vez es fabricar un diente de
    // sierra.
    //
    // Con el maximo, el denominador de lo ya leido es `max(lo real, lo previsto)`: mientras el
    // frame esta en el horno manda la prevision, y cuando sale, si la realidad la supera, manda
    // la realidad. Sumado a lo que queda por leer, el total se queda CLAVADO en la prevision
    // mientras la prevision acierte — que es justo lo que se le pide a un denominador.
    let prev_total = PROG_PREVISION.load(Relaxed);
    let prev_hecha = PROG_PREVISION_HECHA.load(Relaxed);
    let prevision = prev_total.saturating_sub(prev_hecha);
    let indice = PROG_INDICE.load(Relaxed) * PESO_INDICE.load(Relaxed);
    // Lo que llevan hechos los frames que estan EN EL HORNO. Ver `VUELO_BYTES`: es la unica
    // estimacion de todo esto, y esta acotada por el tamaño del frame.
    let ms_hilo = PROG_MS_HILO.load(Relaxed);
    let mut vuelo = 0.0f64;
    if ms_hilo > 0 && plano_hecho > 0 {
        let ritmo = plano_hecho as f64 / ms_hilo as f64;   // bytes por ms y por hilo
        let ahora_ms = arranque.elapsed().as_millis() as u64;
        for k in 0..VUELO_MAX {
            let b = VUELO_BYTES[k].load(Relaxed);
            if b == 0 { continue }
            let t = ahora_ms.saturating_sub(VUELO_MS[k].load(Relaxed)) as f64;
            vuelo += b as f64 * cuanto_lleva(t, b as f64 / ritmo);
        }
        // El indice es otro frame en el horno, solo que pesa `PESO_INDICE` por byte. Sin esto la
        // barra se quedaba clavada 11 s en el mismo numero mientras lo comprimia.
        let t_idx = VUELO_INDICE_MS.load(Relaxed);
        if t_idx > 0 && !PROG_INDICE_LISTO.load(Relaxed) {
            let bytes_idx = PROG_INDICE.load(Relaxed) as f64;
            vuelo += indice as f64
                     * cuanto_lleva(ahora_ms.saturating_sub(t_idx) as f64, bytes_idx / ritmo);
        }
    }
    let trabajo_total = total_b as f64 + plano_total.max(prev_hecha) as f64
                        + prevision as f64 + indice as f64;
    let trabajo_hecho = enrutado as f64 + plano_hecho as f64 + vuelo
                        + if PROG_INDICE_LISTO.load(Relaxed) { indice as f64 } else { 0.0 };
    // Sin pool detras -listar, verificar, o un arbol que cabe entero en un frame- el plano no
    // dice nada y manda lo leido, que ahi SI describe el trabajo.
    let pct = if PROG_POOL.load(Relaxed) && trabajo_total > 0.0 {
        100.0 * trabajo_hecho / trabajo_total
    } else if total_b > 0 { 100.0 * bytes as f64 / total_b as f64 }
      else { 100.0 * hechos as f64 / total as f64 };
    // ⚠️ EL 100% SOLO LO PONE EL FINAL. Con el 99 de tope, «100%» pasa a significar «terminado» y
    // no «me sale 100 de redondear». MEDIDO el 03-08 sobre 622 MB de video+PDF: sin esto la barra
    // ponia 100% con 5,4 s de trabajo por delante, porque los frames del horno se cuentan casi
    // enteros justo antes de salir. La linea final va forzada, y es la unica que puede decir 100.
    let pct = if forzar { pct.min(100.0) } else { pct.min(99.0) };
    // Y los MiB tambien: es de lo que la app saca el ritmo para decir cuanto falta, asi que si
    // aqui fueran solo los leidos volveria a estimar sobre la fase rapida.
    let mb = if PROG_POOL.load(Relaxed) { trabajo_hecho / 1_048_576.0 }
             else { bytes as f64 / 1_048_576.0 };
    if a_tuberia {
        // A una tuberia, LINEAS ENTERAS. Quien lee al otro lado parte por saltos de linea, y un
        // \r deja la linea a medias en el buffer hasta la siguiente.
        // UN DECIMAL, Y NO ES COSMETICA. La app calcula lo que falta como
        // `transcurrido * (100 - pct) / pct` -medido: 48% de error mediano, contra el
        // 334% de estimar el ritmo- y con el porcentaje en enteros, al 1% el resultado
        // salta de golpe entre valores muy separados. Un decimal cuesta un caracter y
        // multiplica por diez la resolucion justo donde mas falta hace.
        eprintln!("  {hechos}/{total} ficheros ({pct:.1}%), {mb:.0} MiB");
    } else {
        eprint!("\r  {hechos}/{total} ficheros ({pct:.1}%), {mb:.0} MiB\x1b[K");
        if forzar { eprintln!(); }
    }
}

/// De donde cuelgan las rutas que se guardan DENTRO del archivo.
///
/// UNA carpeta significa «guarda lo que hay dentro»: la base es ella misma y las rutas salen sin
/// su nombre, que es lo que uno espera al guardar una carpeta.
///
/// VARIAS cosas significan «guarda ESTAS cosas»: la base es la carpeta que las contiene a todas,
/// asi que soltar cuatro videos de Descargas guarda los cuatro videos — y no Descargas entera,
/// que es exactamente lo que hacia antes. Si vienen de sitios distintos, la base sube hasta el
/// ancestro comun y las rutas conservan de donde salio cada una.
fn base_comun(entradas: &[PathBuf]) -> PathBuf {
    let padre = |e: &Path| e.parent().unwrap_or(Path::new(".")).to_path_buf();
    match entradas.len() {
        0 => PathBuf::from("."),
        1 => { let e = &entradas[0]; if e.is_dir() { e.clone() } else { padre(e) } }
        _ => {
            let mut base = padre(&entradas[0]);
            for e in &entradas[1..] {
                let p = padre(e);
                while !p.starts_with(&base) {
                    match base.parent() {
                        Some(x) => base = x.to_path_buf(),
                        None => return base,
                    }
                }
            }
            base
        }
    }
}

pub fn comprimir_pleno(entradas_in: &[PathBuf], out_path: &Path, nivel: i32, jpeg: bool, agrupar: bool,
                       maximo: bool, solido: usize, desde: Option<(Index, u64)>,
                       prefijo: &str, flujo: bool) -> Result<Stats> {
    // El cronometro de fases se lee AQUI y no en el CLI: asi vale igual desde el CLI, desde la
    // app y desde el fuzz, sin tener que acordarse de pasarlo en tres sitios.
    pon_fases(apagado("VC_FASES"));
    let excluir = exclusiones();
    let (idx_base, pos_base) = match desde {
        Some((i, p)) => (i, p),
        None => (Index { nivel, ..Default::default() }, 0),
    };
    let continuando = pos_base > 0;
    let f = if continuando {
        // Colocarse justo donde acaban los frames: el indice y el pie de la version anterior
        // quedan pisados, que es exactamente lo que queremos.
        let fh = std::fs::OpenOptions::new().write(true).open(out_path)
            .with_context(|| format!("abrir para anadir {}", out_path.display()))?;
        // TRUNCAR, no solo colocarse. El indice y el pie de la version anterior viven DESPUES
        // de los frames: si lo nuevo es mas corto que lo viejo, esa cola sobrevive y al leer
        // el pie desde el final del fichero aparece el ANTIGUO. Paso exactamente eso en la
        // primera prueba: una segunda copia sin cambios apenas escribia nada y el archivo
        // quedaba ilegible. El seguro lo detecto y no reemplazo el original, que es para lo
        // que estaba, pero el fallo era este.
        fh.set_len(pos_base).context("truncar el archivo donde acaban los frames")?;
        let mut fh = fh;
        fh.seek(SeekFrom::Start(pos_base))?;
        fh
    } else {
        File::create(out_path).with_context(|| format!("crear {}", out_path.display()))?
    };
    let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    // El tope de 10 nunca se ha medido: es un numero puesto a ojo. VC_WORKERS existe para poder
    // barrerlo y averiguar cual conviene, que sin la palanca no hay forma de preguntarselo.
    // EL TOPE ES 4, Y ESTA MEDIDO. De 4 a 12 hilos se gana un 6% por triplicarlos, y la CPU
    // nunca pasa de 4,6 nucleos ocupados: los demas esperan al hilo principal, que es lo que
    // dice la piramide con «esperar» = 0,00 s. Ademas cada hilo con --max carga su propio
    // modelo de 1,15 GB, asi que los que sobran cuestan memoria de verdad.
    let mut nworkers = std::env::var("VC_WORKERS").ok().and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| ncpu.saturating_sub(2).clamp(1, 4));
    // GUARDIA DE MEMORIA. Cada hilo carga su propia instancia del modelo mas el frame entero
    // en RAM; medido, ~2,3 GB por hilo con --max y frames de 32 MiB. Con --solid=256 y diez
    // hilos serian ~25 GB, o sea tumbar la maquina por un archivo. El pool se limita por lo
    // que cada hilo GASTA DE VERDAD, no por cuantos nucleos hay.
    if maximo || solido > 0 {
        // Con --max cada hilo carga su propio modelo -483 MiB de tablas mas sus estructuras- y
        // el frame entero. Medido: ~1,15 GB por hilo con frames de 32 MiB, o sea 11,5 GB con
        // diez. El guardia SOLO se aplicaba con --solid, asi que un --max normal no tenia
        // ninguno.
        let por_hilo_mb = if maximo {
            if solido > 0 { 2048 + solido } else { 1152 }
        } else { solido * 2 };
        // El techo sale de la memoria que HAY, no de un numero inventado. 11,5 GB en una
        // maquina de 62 no es un defecto: es lo que cuestan diez modelos y la maquina los
        // tiene. El defecto era que en una de 8 GB iba a intentar exactamente lo mismo.
        let techo_mb: usize = std::env::var("VC_RAM_MB").ok().and_then(|v| v.parse().ok())
            .unwrap_or_else(|| (memoria_fisica_mb() * 6 / 10).clamp(1024, 24 * 1024));
        nworkers = nworkers.min((techo_mb / por_hilo_mb.max(1)).max(1));
    }

    let nframes_base = idx_base.frames.len() as u32;
    let (tx_work, rx_work) = mpsc::sync_channel::<PendingFrame>(16);
    let (tx_done, rx_done) = mpsc::channel::<DoneFrame>();
    let rx_work = Arc::new(Mutex::new(rx_work));
    set_nivel(nivel);
    set_cm(maximo);
    set_solido(solido);

    let mut pool = Vec::new();
    for k in 0..nworkers {
        let rxw = Arc::clone(&rx_work);
        let txd = tx_done.clone();
        // La casilla de este hilo en el tablero de «frames en el horno». Ver `VUELO_BYTES`.
        let casilla = k.min(VUELO_MAX - 1);
        pool.push(std::thread::spawn(move || -> Result<()> {
            loop {
                let job = { let g = rxw.lock().unwrap(); g.recv() };
                let Ok(pf) = job else { break };
                let rsize = pf.data.len() as u64;
                // PERILLA DE BANCO (F6·Q3) · `VC_BANCO_FRAMES=<dir>`: el CONTENIDO CRUDO de cada
                // frame, tal y como el archivador lo arma, antes de comprimirlo. No mueve ni un
                // byte de la salida —solo escribe una copia— y es la unica forma de medir un
                // frame REAL con `vc-cm` suelto en vez de un `head -c` que se le parece. Fuera
                // del banco la variable no esta y esto es una lectura de entorno por frame.
                if let Some(d) = banco_frames() {
                    let _ = std::fs::create_dir_all(&d);
                    let _ = std::fs::write(format!("{d}/carril{}-frame{:05}.bin", pf.lane, pf.id),
                                           &pf.data);
                }
                let t_frame = std::time::Instant::now();
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    let arranque = *PROG_ARRANQUE.get_or_init(std::time::Instant::now);
                    VUELO_MS[casilla].store(arranque.elapsed().as_millis() as u64, Relaxed);
                    VUELO_BYTES[casilla].store(rsize, Relaxed);
                }
                // Carril texto en modo máximo: nuestro compresor de mezcla de contextos.
                // En media/otros no se usa: son datos ya comprimidos donde el CM no gana y
                // sí cuesta muchísimo tiempo.
                // Con --max el modelo de contextos entra tambien en el carril de OTROS.
                // Solo estaba en el de texto, y eso costaba carisimo: en Silesia la mitad del
                // corpus son ejecutables y datos numericos que caen en "otros", asi que
                // ibamos a la pelea con zstd mientras zpaq usaba su modelo en todo. El carril
                // de MEDIA se queda fuera a proposito: ahi los datos ya vienen comprimidos y
                // el CM no gana nada a cambio de tardar muchisimo. COMPROBADO 31-07: sobre 106 MB
                // de video, --max da 77.187.945 B contra 77.188.169 sin el. Cero.
                //
                // PERO ESO NO IMPLICABA EL NIVEL. De «el CM no gana nada» no se sigue «zstd
                // tampoco»: son dos compresores distintos, y el 3 llevaba clavado ahi sin que
                // nadie lo hubiera probado. Sobre 71 videos reales (632 MB), rondas 2-3:
                //     zstd -3    287.147.660 B        -      1,40 s
                //     zstd -9    280.796.870 B  -2,212%      1,67 s   +18,93%
                //     zstd -12   278.820.035 B  -2,900%      2,89 s  +106,43%
                // El 9 y no el 12: el 12 da 0,69 puntos mas por 5,5 veces el reloj extra. El 9
                // cuesta +0,27 s sobre 632 MB y compra 6,35 MB, que en dinero no tiene discusion.
                //
                // ⚠️ Y LA CURVA NO ES MONOTONA: el nivel 6 sale PEOR que el 3 (+0,802% en el video
                // de pantalla). Subir «un poco» a ojo habria empeorado el resultado.
                let (cod, comp) = cron(F_COMPRIMIR, || -> Result<(u8, Vec<u8>)> {
                    Ok(if usar_cm() && (pf.lane == LANE_TEXT || pf.lane == LANE_OTHER) {
                        (CODEC_CM, vc_cm::comprimir(&pf.data))
                    } else {
                        let lvl = if pf.lane == LANE_MEDIA { nivel_media() } else { pf_nivel() };
                        (CODEC_ZSTD, comprime_zstd(&pf.data, lvl).context("zstd compress")?)
                    })
                })?;
                let (codec, bytes) = if (comp.len() as u128) * 100 >= (rsize as u128) * STORE_PCT { (CODEC_STORE, pf.data) } else { (cod, comp) };
                // EL FRAME YA ESTA COMPRIMIDO: esto SI es trabajo hecho. Ver `PROG_PLANO_TOTAL`.
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    VUELO_BYTES[casilla].store(0, Relaxed);   // sale del horno
                    PROG_MS_HILO.fetch_add(t_frame.elapsed().as_millis() as u64, Relaxed);
                    PROG_PLANO_HECHO.fetch_add(rsize, Relaxed);
                }
                pinta_progreso(false);
                if txd.send(DoneFrame { id: pf.id, lane: pf.lane, codec, rsize, bytes }).is_err() { break; }
            }
            Ok(())
        }));
    }
    drop(tx_done);

    let writer = std::thread::spawn(move || -> Result<(File, u64, Vec<(u32, FrameRec)>, u64)> {
        let mut bw = BufWriter::new(f);
        // Continuando, el MAGIC ya esta escrito y la posicion viene de donde acabaron los
        // frames anteriores. Volver a escribirlo aqui corromperia el archivo.
        let mut pos: u64 = if continuando { pos_base } else {
            // Cabecera fija de 40 B, cifre o no. Ver la nota de F2.72: un offset de primer
            // frame que dependiera del modo es un fallo esperando a pasar.
            let mut cab = [0u8; CABECERA];
            cab[0..8].copy_from_slice(MAGIC);
            if let Some(_k) = clave() {
                cab[8] = 1;
                let sal = SAL_EN_USO.read().unwrap().clone();
                cab[9..25].copy_from_slice(&sal);
                let p = cripto::Params::default();
                cab[25..29].copy_from_slice(&p.mib.to_le_bytes());
                cab[29..33].copy_from_slice(&p.pasadas.to_le_bytes());
                cab[33..37].copy_from_slice(&p.carriles.to_le_bytes());
            }
            bw.write_all(&cab)?;
            CABECERA as u64
        };
        let mut recs: Vec<(u32, FrameRec)> = Vec::new();
        let mut nstore = 0u64;
        // Buffer de reordenación: los workers terminan en cualquier orden, pero los frames se
        // escriben SIEMPRE por id. Sin esto, dos empaquetados del mismo árbol dan ficheros
        // distintos byte a byte (mismo contenido, distinta colocación). La memoria está
        // acotada sola: los workers no pueden adelantarse más allá del canal (16) + los suyos.
        let mut pendientes: HashMap<u32, DoneFrame> = HashMap::new();
        // Los ids de frame siguen donde lo dejo el archivo anterior, no en cero.
        let mut siguiente: u32 = nframes_base;
        for df in rx_done {
            pendientes.insert(df.id, df);
            while let Some(d) = pendientes.remove(&siguiente) {
                // CADA FRAME lleva su propio nonce dentro. Ver F2.72: `forget` renumera los
                // frames al compactar, asi que un nonce ligado al numero se reutilizaria con la
                // misma clave sobre datos distintos.
                let bytes = match clave() {
                    Some(k) => cripto::cifra(&k, &d.bytes)?,
                    None => d.bytes,
                };
                cron(F_ESCRIBIR, || bw.write_all(&bytes))?;
                // csize es lo que ocupa EN DISCO, o sea el bloque ya cifrado. Con el tamano en
                // claro se pedirian 28 bytes de menos al leer y el descifrado fallaria en TODOS
                // los frames — pero solo con cifrado activado, que es como se cuela un fallo
                // que las pruebas de siempre no ven.
                let n = bytes.len() as u64;
                recs.push((d.id, FrameRec { offset: pos, csize: n, rsize: d.rsize, codec: d.codec, lane: d.lane }));
                if d.codec == 0 { nstore += 1; }
                pos += n;
                siguiente += 1;
            }
        }
        if !pendientes.is_empty() { bail!("{} frames quedaron sin drenar (hueco en la numeración)", pendientes.len()); }
        bw.flush()?;
        Ok((bw.into_inner().map_err(|e| anyhow::anyhow!("flush: {e}"))?, pos, recs, nstore))
    });

    // El mapa de deduplicacion se siembra con las huellas que el indice ya trae (F2.60). Esto
    // es TODO lo incremental: un trozo que ya estaba no se vuelve a comprimir, se referencia.
    // Sin las huellas guardadas habria que descomprimir el archivo entero para saberlo.
    let mut dedup: HashMap<[u8; 32], u64> = HashMap::new();
    for (i, c) in idx_base.chunks.iter().enumerate() {
        if c.hash != [0u8; 16] {
            let mut k = [0u8; 32];
            k[..16].copy_from_slice(&c.hash);
            dedup.insert(k, i as u64);
        }
    }
    let mut a = Assembler {
        next_frame: idx_base.frames.len() as u32,
        idx: idx_base,
        lanes: (0..LANES).map(|l| Vec::with_capacity(frame_target(l) + cdc_params().2)).collect(),
        pendientes: (0..LANES).map(|_| Vec::new()).collect(),
        dedup,
        st: Stats { hilos: nworkers, ..Default::default() },
        tx: tx_work,
    };

    // La tabla de «esto ya estaba igual». Solo tiene sentido si venimos de un archivo previo:
    // en un `compress` desde cero no hay nada contra lo que comparar y esto queda en None, con
    // lo que el camino de siempre no cambia ni en un syscall.
    let previos = if a.idx.files.is_empty() || apagado("VC_SIN_INCREMENTAL") { None }
                  else { Some(Previos::de(&a.idx.files)) };
    // El segundo en que arranca esta ejecucion, para la guardia del mtime de arriba. Si el reloj
    // no se puede leer se usa i64::MIN, que hace que no se salte NADA: ante la duda, trabajar de
    // mas es recuperable y saltarse un fichero cambiado no lo es.
    let ahora_s: i64 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64).unwrap_or(i64::MIN);

    let base = if flujo { PathBuf::from(".") } else { base_comun(entradas_in) };
    let mut rutas: Vec<PathBuf> = Vec::new();
    // (dispositivo, inodo) -> la PRIMERA ruta que lo trajo, y la lista de las demas. Viven en
    // el ambito de la funcion, no en el del recorrido: se rellenan al recorrer y se resuelven
    // al final, cuando ya se sabe el indice de cada fichero.
    let mut vistos_inodo: HashMap<(u64, u64), PathBuf> = HashMap::new();
    let mut duros: Vec<(PathBuf, PathBuf)> = Vec::new();

    if flujo {
        // Un flujo es UN fichero cuyo contenido llega a trozos y cuyo tamano no se sabe hasta
        // que se acaba. El troceador ya sabe trabajar asi; lo unico que cambia es de donde
        // salen los bytes.
        let nombre = if prefijo.is_empty() { "stdin".to_string() } else { prefijo.to_string() };
        let entrada = std::io::stdin();
        let mut fl = { let (a,b,c) = cdc_params(); cdc::TrozosFlujo::new(entrada.lock(), a, b, c) };
        let mut hasher = blake3::Hasher::new();
        let mut ids: Vec<u64> = Vec::new();
        let mut total = 0u64;
        // El carril se decide con el PRIMER trozo. Es la unica informacion que hay en el
        // momento de decidir: a diferencia de un fichero, aqui no se puede mirar la extension
        // ni volver atras a leer la cabecera.
        let mut carril = LANE_TEXT;
        let mut primero = true;
        while let Some(trozo) = fl.siguiente().context("leer de la entrada estandar")? {
            if primero {
                carril = if parece_texto_bytes(&trozo) { LANE_TEXT } else { LANE_OTHER };
                primero = false;
            }
            hasher.update(&trozo);
            total += trozo.len() as u64;
            a.st.chunks_total += 1;
            ids.push(a.add_chunk(carril, &trozo)?);
        }
        a.st.in_bytes += total;
        a.st.files += 1;
        let ruta = if prefijo.is_empty() { nombre } else { format!("{prefijo}/stdin") };
        a.idx.files.push(FileRec {
            path: ruta, size: total, chunks: ids, hash: recorta(hasher.finalize()),
            transform: None,
            // Un flujo no trae permisos ni fecha. 0644 y 0: inventarse un mtime seria mentir
            // sobre el dato, y un archivador no miente sobre lo que guarda.
            mode: 0o644, mtime: 0, link_target: None, ..Default::default()
        });
    } else {
        // Se juntan las entradas de TODAS las cosas que se han pedido guardar. Una carpeta
        // aporta su arbol entero; un fichero suelto se aporta a si mismo.
        let mut entradas = Vec::new();
        for e in entradas_in {
            if e.is_dir() { cron(F_RECORRER, || recorrer(e, &mut entradas))?; }
            else {
                // Un fichero suelto pasa por el MISMO sitio que los del recorrido, no por un
                // atajo aparte: asi le llegan igual las exclusiones, los enlaces duros y los
                // ficheros especiales. Antes se empujaba directo a `rutas` y se saltaba las tres.
                let md = std::fs::symlink_metadata(e)
                    .with_context(|| format!("stat {}", e.display()))?;
                let t = if md.is_symlink() { Tipo::Symlink }
                        else if md.is_file() { Tipo::Fichero }
                        else { Tipo::Especial };
                entradas.push((e.clone(), t));
            }
        }
        for (p, tipo) in entradas {
            // La exclusion se mira sobre la ruta RELATIVA, que es lo que el usuario escribe:
            // nadie pone `--exclude /tmp/loquesea/node_modules`, pone `node_modules`.
            if !excluir.is_empty() {
                let rel = p.strip_prefix(&base).unwrap_or(&p).to_string_lossy().into_owned();
                if excluir.iter().any(|pat| casa_patron(&rel, pat)) {
                    a.st.excluidos += 1;
                    continue;
                }
            }
            match tipo {
                Tipo::Symlink => {
                    // symlink: guardamos su destino (no seguimos el enlace). Reconstruible byte a byte.
                    let rel = p.strip_prefix(&base).unwrap_or(&p).to_string_lossy().into_owned();
                    let target = std::fs::read_link(&p).with_context(|| format!("read_link {}", p.display()))?.to_string_lossy().into_owned();
                    let (mode, mtime, uid, gid, _) = atributos_de(&p);
                    a.st.files += 1;
                    a.idx.files.push(FileRec { uid, gid, path: rel, size: target.len() as u64, chunks: vec![], hash: huella(target.as_bytes()), transform: None, mode, mtime, link_target: Some(target), ..Default::default() });
                }
                Tipo::DirVacio => {
                    // Sin contenido que trocear: sólo la ruta y sus metadatos. El tipo va en
                    // los bits altos del modo, igual que en el sistema de ficheros.
                    let rel = p.strip_prefix(&base).unwrap_or(&p).to_string_lossy().into_owned();
                    let (mode, mtime, uid, gid, _) = atributos_de(&p);
                    let xattr = xattrs_de(&p);
                    a.st.files += 1;
                    a.idx.files.push(FileRec { path: rel, size: 0, chunks: vec![], hash: huella(b""), transform: None, mode, mtime, link_target: None, uid, gid, xattr, ..Default::default() });
                }
                Tipo::Especial => {
                    // Fifos y dispositivos: no tienen contenido que trocear, solo identidad.
                    // Antes se saltaban SIN DECIR NADA, que es la peor forma de no soportar algo.
                    let rel = p.strip_prefix(&base).unwrap_or(&p).to_string_lossy().into_owned();
                    let (mode, mtime, uid, gid, rdev) = atributos_de(&p);
                    a.st.files += 1;
                    a.idx.files.push(FileRec { path: rel, size: 0, chunks: vec![], hash: huella(b""),
                        transform: None, mode, mtime, link_target: None, uid, gid, rdev,
                        ..Default::default() });
                }
                Tipo::Fichero => {
                    // ENLACES DUROS. Dos rutas con el mismo (dispositivo, inodo) son EL MISMO
                    // fichero. La primera se comprime; las demas solo guardan a quien apuntan.
                    match inodo_de(&p) {
                        Some(k) => match vistos_inodo.get(&(k.0, k.1)) {
                            Some(primero) => duros.push((p.clone(), primero.clone())),
                            None => { vistos_inodo.insert((k.0, k.1), p.clone()); rutas.push(p); }
                        },
                        None => rutas.push(p),
                    }
                }
            }
        }
    }

    {
        use std::sync::atomic::Ordering::Relaxed;
        PROG_TOTAL.store(rutas.len() as u64, Relaxed);
        PROG_HECHOS.store(0, Relaxed);
        PROG_BYTES.store(0, Relaxed);
        PROG_PREP.store(0, Relaxed);
        PROG_PLANO_TOTAL.store(0, Relaxed);
        PROG_PLANO_HECHO.store(0, Relaxed);
        // El indice arranca con lo que cuesta un registro por fichero: la ruta y su metadato.
        // Medido sobre seis arboles, ~80 B de media, que es la misma estimacion que usa
        // `formato::escribir` para reservar el Vec.
        PROG_INDICE.store(rutas.len() as u64 * 80, Relaxed);
        PROG_INDICE_FIRME.store(false, Relaxed);
        PROG_INDICE_LISTO.store(false, Relaxed);
        PROG_MS_HILO.store(0, Relaxed);
        VUELO_INDICE_MS.store(0, Relaxed);
        for k in 0..VUELO_MAX { VUELO_BYTES[k].store(0, Relaxed); }
        PESO_INDICE.store(std::env::var("VC_PESO_INDICE").ok().and_then(|v| v.parse().ok())
                          .unwrap_or(nworkers as u64).max(1), Relaxed);
        PROG_ULT_MS.store(0, Relaxed);
        PROG_POOL.store(true, Relaxed);
        // Cuantos bytes hay que leer, para que el porcentaje sea de TRABAJO y no de ficheros.
        // Los stat vienen calientes del recorrido, asi que cuesta milisegundos.
        // Y de paso la PREVISION DE PLANO por tipo: el mismo stat, una multiplicacion mas.
        // Hacerlo aqui y no en otra pasada importa: los stat vienen calientes del recorrido.
        let (mut tot, mut prev) = (0u64, 0u64);
        for r in rutas.iter() {
            if let Ok(m) = std::fs::metadata(r) {
                tot += m.len();
                prev += m.len() / 256 * plano_por_byte_256(r);
            }
        }
        PROG_TOTAL_BYTES.store(tot, Relaxed);
        PROG_PREVISION.store(prev, Relaxed);
        PROG_PREVISION_HECHA.store(0, Relaxed);
    }
    let mut latido = Latido::arranca();
    if agrupar { cron(F_AGRUPAR, || agrupa_por_similitud(&mut rutas, nworkers)); }

    for lote in rutas.chunks(JXL_LOTE) {
        // ¿A QUIEN DE ESTE LOTE HAY QUE TRABAJARLE ALGO? Se decide AQUI, antes de las dos
        // pasadas caras de abajo, y no dentro del bucle de ficheros.
        //
        // MEDIDO 2026-07-31, y es la razon de que esto exista. Con la comprobacion puesta
        // DESPUES —que es donde la puse primero— el trabajo POR FICHERO desaparecia entero
        // (leer+trocear, hash, comprimir y escribir a 0,00 s sobre 43.298 ficheros) y aun asi
        // el reloj solo bajaba un 14%: seguian pagandose 134,6 s de 136,2 transcodificando a
        // JXL y preparando el DEFLATE de ficheros que un instante despues se daban por buenos
        // sin mirarlos. Saltarse el trabajo DESPUES de haberlo preparado no es saltarselo.
        //
        // Los atributos se piden UNA vez aqui y se reusan abajo, asi que esto no anade ningun
        // stat: es el mismo que el camino normal hacia al final de todas formas.
        let mut salto: Vec<Option<u32>> = vec![None; lote.len()];
        let mut attrs: Vec<Option<(u32, i64, u32, u32, u64)>> = vec![None; lote.len()];
        if let Some(p) = previos.as_ref() {
            cron(F_STAT, || {
                for (i, ruta) in lote.iter().enumerate() {
                    let at = atributos_de(ruta);
                    attrs[i] = Some(at);
                    // El segundo en curso no se fia: ver la nota larga de `Previos`.
                    if at.1 >= ahora_s { continue }
                    // Si el stat falla, NO se salta: que lo trate el camino normal, que ya sabe
                    // dar el error con su ruta en vez de tragarselo aqui.
                    let Ok(md) = ruta.metadata() else { continue };
                    let rel0 = ruta.strip_prefix(&base).unwrap_or(ruta).to_string_lossy();
                    salto[i] = p.busca(&a.idx.files, &rel0, md.len(), at.1);
                }
            });
        }
        let saltar: Vec<bool> = salto.iter().map(|x| x.is_some()).collect();

        traza("lote: empieza la pre-pasada JXL");
        let pre = if jpeg { cron(F_JXL, || pretranscodifica_jpeg(lote, &saltar, nworkers)) } else { HashMap::new() };
        traza("lote: empieza la pre-pasada deflate");
        // El carril deflate se prepara EN PARALELO, igual que el de JPEG. Es el 69,9% del reloj
        // y estaba corriendo en el hilo principal mientras los demas esperaban («esperar» eran
        // 0,14 s de 377). Lo que se reparte es solo el trabajo CARO Y PURO; meterlo en el
        // archivo sigue siendo secuencial y en el mismo orden, que es de lo que depende que el
        // archivo salga byte a byte identico.
        let mut pred = cron(F_TRANSF, || cron(F_PREDEFLATE, || pre_deflate(lote, &saltar, nworkers)));
        traza("lote: empieza el bucle por fichero");
        for (i, ruta) in lote.iter().enumerate() {
            {
                use std::sync::atomic::Ordering::Relaxed;
                PROG_HECHOS.fetch_add(1, Relaxed);
            }
            let ruta = ruta.as_path();
            // UN solo stat por fichero en esta iteracion. Antes eran dos: uno aqui para la
            // barra de progreso y otro cinco lineas mas abajo para `size`, pidiendo el MISMO
            // dato a la misma ruta. La barra usa ahora el tamaño que ya hace falta de todas
            // formas, y de paso el porcentaje deja de poder discrepar del trabajo real.
            let size = cron(F_STAT, || ruta.metadata())?.len();
            {
                use std::sync::atomic::Ordering::Relaxed;
                PROG_BYTES.fetch_add(size, Relaxed);
                // Este fichero deja de ser promesa: su prevision sale del pendiente y su
                // plano real entrara por `PROG_PLANO_TOTAL` cuando el frame se cierre.
                PROG_PREVISION_HECHA.fetch_add(size / 256 * plano_por_byte_256(ruta), Relaxed);
                // ENRUTAR TAMBIEN ES TRABAJO, y este bucle es el unico que lo ve. Estuvo callado
                // media hora -«con pool detras el avance lo pinta el pool»- y el resultado fue
                // que la barra no decia NADA durante los primeros 25,7 s de 83. Ahora los dos
                // emisores calculan LO MISMO (ver `pinta_progreso`), asi que ya no se pisan.
                pinta_progreso(false);
            }
            let rel0 = ruta.strip_prefix(&base).unwrap_or(ruta).to_string_lossy().into_owned();
            let rel = if prefijo.is_empty() { rel0 } else { format!("{prefijo}/{rel0}") };
            // ¿Ya estaba, igual, en una instantanea anterior? Entonces no se toca: ni leer, ni
            // inflar, ni transformar, ni trocear, ni hashear. Se anota que esta ruta apunta a
            // los MISMOS trozos, que siguen en el archivo, y a por el siguiente.
            //
            // Los atributos se piden AQUI y se reusan abajo, asi que no se paga ningun stat de
            // mas: es el mismo que hacia falta al final de todas formas. Se piden con
            // `symlink_metadata`, igual que el camino normal, para que el mtime que se compara
            // sea EL MISMO que el que se guardo.
            let attr = attrs[i];
            // Ya estaba, igual, en una instantanea anterior: no se toca. Ni leer, ni inflar,
            // ni transformar, ni trocear, ni hashear. Se anota que esta ruta apunta a los MISMOS
            // trozos, que siguen dentro del archivo, y a por el siguiente.
            if let Some(j) = salto[i] {
                let (mode, mtime, uid, gid, _) = attr.expect("salto[i] implica attrs[i]");
                let v = &a.idx.files[j as usize];
                let (chunks, hash, transform) = (v.chunks.clone(), v.hash, v.transform.clone());
                // Permisos y dueno se cogen de AHORA, no del registro viejo: un `chmod` no toca
                // el mtime, asi que copiarlos del viejo los perderia en silencio. Salen del stat
                // que ya se hizo arriba, o sea que son gratis.
                let xattr = cron(F_META, || xattrs_de(ruta));
                a.st.in_bytes += size;
                a.st.files += 1;
                a.st.saltados_iguales += 1;
                a.st.bytes_saltados += size;
                a.idx.files.push(FileRec { path: rel, size, chunks, hash, transform, mode, mtime,
                                           link_target: None, uid, gid, xattr, ..Default::default() });
                continue;
            }
            // El router mira la FIRMA del contenido, no la extensión: en el corpus real hay
            // 58 PNG llamados .jpg, y con la extensión irían al carril equivocado.
            if size >= 64 && size <= PREFLATE_MAX && !apagado("VC_SIN_PREFLATE") {
                // Si el pre-pase paralelo ya lo preparo, se usa y no se vuelve a hacer nada caro.
                if let Some((d, prep)) = pred.remove(&i) {
                    match cron(F_TRANSF, || cron(F_PREFLATE, || mete_deflate(&mut a, ruta, rel.clone(), &d, prep))) {
                        Ok(true) => continue,
                        Ok(false) => { a.st.deflate_rechazados += 1; }
                        Err(e) => { eprintln!("aviso: deflate {} fallo ({e}); sigue por el camino normal", ruta.display()); }
                    }
                }
                let f = cron(F_ENRUTAR, || firma_de(ruta));
                if matches!(f, deflate::Formato::Gzip | deflate::Formato::Png | deflate::Formato::Zip
                              | deflate::Formato::Pdf) {
                    let d = cron(F_TRANSF, || std::fs::read(ruta)).with_context(|| format!("leer {}", ruta.display()))?;
                    match cron(F_TRANSF, || cron(F_PREFLATE, || try_deflate(&mut a, ruta, rel.clone(), &d))) {
                        Ok(true) => continue,
                        Ok(false) => { a.st.deflate_rechazados += 1; }
                        Err(e) => { eprintln!("aviso: deflate {} falló ({e}); sigue por el camino normal", ruta.display()); }
                    }
                }
            }
            // Logs: mismo criterio que con los ejecutables — sólo se lee entero si el
            // principio ya tiene pinta de llevar cabecera de fecha en casi todas las líneas.
            if size >= 64 * 1024 && lane_for(ruta) == LANE_TEXT && !apagado("VC_SIN_LOGTS") {
                if let Some(cab) = cron(F_ENRUTAR, || lee_prefijo(ruta, 256 * 1024)) {
                    if parece_log_con_ts(&cab) {
                        let dd = cron(F_TRANSF, || std::fs::read(ruta)).with_context(|| format!("leer {}", ruta.display()))?;
                        match cron(F_TRANSF, || cron(F_LOGTS, || try_log_ts(&mut a, ruta, rel.clone(), &dd))) {
                            Ok(true) => continue,
                            Ok(false) => {}
                            Err(e) => { eprintln!("aviso: log-ts {} falló ({e}); sigue por el camino normal", ruta.display()); }
                        }
                    }
                }
            }
            // Ejecutables: sólo se leen enteros si la pinta ya lo sugiere, para no cargar en
            // memoria cualquier binario grande por si acaso.
            if size >= 4096 && size <= PREFLATE_MAX && lane_for(ruta) == LANE_OTHER
               && !apagado("VC_SIN_E8E9") {
                if let Some(cab) = cron(F_ENRUTAR, || lee_ventana(ruta, size / 3, 512 * 1024)) {
                    if parece_ejecutable(&cab) {
                        let dd = cron(F_TRANSF, || std::fs::read(ruta)).with_context(|| format!("leer {}", ruta.display()))?;
                        match cron(F_TRANSF, || cron(F_E8E9, || try_e8e9(&mut a, ruta, rel.clone(), &dd))) {
                            Ok(true) => continue,
                            Ok(false) => {}
                            Err(e) => { eprintln!("aviso: e8e9 {} falló ({e}); sigue por el camino normal", ruta.display()); }
                        }
                    }
                }
            }
            if let Some((jpeg, flujo)) = pre.get(&i) {
                // ⚠️ ESTA LLAMADA NO SE CRONOMETRABA: el carril JXL no estaba atribuido en la
                // piramide. Se envuelve en F_JXL, que es donde ya se contaba su pre-pasada.
                cron(F_JXL, || mete_jxl(&mut a, ruta, rel.clone(), jpeg, flujo))?;
                continue;
            }
            if jpeg && es_candidato_jpeg(ruta) { a.st.jxl_rechazados += 1; } // no fue byte-exacto: a store
            let lane = lane_for(ruta);
            let fh = File::open(ruta).with_context(|| format!("abrir {}", ruta.display()))?;
            let mut hasher = blake3::Hasher::new();
            let mut ids: Vec<u64> = Vec::new();
            if size > 0 {
                let mut flujo = { let (a,b,c) = cdc_params(); cdc::TrozosFlujo::new(fh, a, b, c) };
                while let Some(trozo) = cron(F_TROCEAR, || flujo.siguiente()).context("cdc")? {
                    cron(F_HASH, || hasher.update(&trozo));
                    a.st.chunks_total += 1;
                    ids.push(a.add_chunk(lane, &trozo)?);
                }
            }
            a.st.in_bytes += size;
            a.st.files += 1;
            let (mode, mtime, uid, gid, _) = match attr {
                Some(t) => t,                                      // ya pedido para el atajo
                None => cron(F_META, || atributos_de(ruta)),
            };
            let xattr = cron(F_META, || xattrs_de(ruta));
            a.idx.files.push(FileRec { path: rel, size, chunks: ids, hash: recorta(hasher.finalize()), transform: None, mode, mtime, link_target: None, uid, gid, xattr, ..Default::default() });
        }
    }
    // Aqui se ACABA DE LEER, no de trabajar: los frames siguen en el pool y falta el indice. La
    // linea final se pinta al final del todo; esta se deja al freno de siempre.
    pinta_progreso(false);
    traza("se acabo de LEER; quedan los frames en el pool y el indice");
    let (_fb, _ff) = (a.st.in_bytes, a.st.files);
    for l in 0..LANES { a.flush_lane(l)?; }
    drop(a.tx);

    traza("esperando a que el pool termine los frames que quedan");
    for h in pool { h.join().map_err(|_| anyhow::anyhow!("worker panic"))??; }
    traza("pool terminado");
    let (file, pos, mut recs, nstore) = writer.join().map_err(|_| anyhow::anyhow!("writer panic"))??;
    // Los enlaces duros se resuelven AHORA, no durante el recorrido: `agrupa_por_similitud`
    // reordena los ficheros, asi que el indice de uno no se conoce hasta que estan todos.
    if !duros.is_empty() {
        let pos: HashMap<&str, u32> = a.idx.files.iter().enumerate()
            .map(|(i, f)| (f.path.as_str(), i as u32)).collect();
        let mut nuevos = Vec::new();
        for (enlace, origen) in &duros {
            let rel_o = origen.strip_prefix(&base).unwrap_or(origen).to_string_lossy().into_owned();
            let rel_o = if prefijo.is_empty() { rel_o } else { format!("{prefijo}/{rel_o}") };
            let Some(&j) = pos.get(rel_o.as_str()) else { continue };
            let rel_e = enlace.strip_prefix(&base).unwrap_or(enlace).to_string_lossy().into_owned();
            let rel_e = if prefijo.is_empty() { rel_e } else { format!("{prefijo}/{rel_e}") };
            let (mode, mtime, uid, gid, _) = atributos_de(enlace);
            nuevos.push(FileRec {
                path: rel_e, size: a.idx.files[j as usize].size, chunks: vec![],
                hash: a.idx.files[j as usize].hash, transform: None,
                mode, mtime, link_target: None, uid, gid, rdev: 0, enlace_a: Some(j),
                xattr: xattrs_de(enlace),
            });
        }
        a.st.files += nuevos.len() as u64;
        a.idx.files.extend(nuevos);
    }
    recs.sort_by_key(|(id, _)| *id);
    if recs.len() as u32 + nframes_base != a.next_frame {
        bail!("frames perdidos: {} nuevos + {} de base, se esperaban {}",
              recs.len(), nframes_base, a.next_frame);
    }
    // Los frames de la copia anterior siguen donde estaban: solo se anaden los nuevos detras.
    a.idx.frames.truncate(nframes_base as usize);
    a.idx.frames.extend(recs.into_iter().map(|(_, r)| r));
    a.st.frames_store = nstore;

    let mut bw = BufWriter::new(file);
    // 🔑 EL CIERRE ES TRABAJO, Y NO ESTABA NI CRONOMETRADO NI CONTADO EN LA BARRA.
    //
    // `F_ESCRIBIR` y `F_CERRAR` llevaban declaradas desde el principio y no las usaba NADIE: el
    // desglose de fases se paraba justo donde acaba el pool. Medido el 03-08 sobre 478 MiB, esa
    // parte muda son 11,3 s de 83 —el 13,6% de la corrida— con la barra clavada en el 100%. El
    // hueco de la piramide y el silencio de la barra eran el MISMO agujero visto por dos sitios.
    traza("empieza el CIERRE: serializar el indice");
    let idx_raw = cron(F_CERRAR, || formato::escribir(&a.idx));
    traza("indice serializado; empieza a comprimirse");
    // El índice es lo que mejor le va a nuestro modelo: 27.000 rutas ordenadas, con prefijos
    // compartidos y estructura de tabla. Bajo --max se comprime con vc-cm; si no, con zstd,
    // porque descomprimirlo con CM cuesta y el índice se lee SIEMPRE que se abre el archivo.
    // El códec va en el primer byte del bloque, así que el lector no necesita adivinarlo.
    //
    // Y ENTRA EN LA CUENTA DEL AVANCE COMO UN FRAME MAS, porque es exactamente eso: un bloque de
    // plano por el mismo compresor y al mismo nivel. Ver `PROG_INDICE_PREVISTO`, que es lo que
    // reserva su sitio ANTES de conocer su tamaño para que la barra no se caiga del 100% al 97%.
    let contado = PROG_INDICE.load(std::sync::atomic::Ordering::Relaxed);
    prevision_indice_firme(idx_raw.len() as u64);
    // El indice entra al horno: a partir de aqui la barra lo cuenta como un frame mas en curso.
    VUELO_INDICE_MS.store(PROG_ARRANQUE.get_or_init(std::time::Instant::now)
                          .elapsed().as_millis().max(1) as u64,
                          std::sync::atomic::Ordering::Relaxed);
    let mut idx_comp: Vec<u8> = Vec::new();
    if usar_cm() {
        idx_comp.push(CODEC_CM);
        idx_comp.extend_from_slice(&cron(F_CERRAR, || vc_cm::comprimir(&idx_raw)));
    } else {
        idx_comp.push(CODEC_ZSTD);
        idx_comp.extend_from_slice(&cron(F_CERRAR, || comprime_zstd(&idx_raw, 19))?);
    }
    PROG_INDICE_LISTO.store(true, std::sync::atomic::Ordering::Relaxed);
    // El indice tambien va cifrado: lleva las RUTAS de todos los ficheros, que dicen casi tanto
    // como el contenido. Un respaldo cifrado que filtre el arbol de directorios no esta cifrado.
    let idx_comp = match clave() { Some(k) => cripto::cifra(&k, &idx_comp)?, None => idx_comp };
    cron(F_ESCRIBIR, || -> Result<()> {
        bw.write_all(&idx_comp)?;
        bw.write_all(&pos.to_le_bytes())?;
        bw.write_all(&(idx_comp.len() as u64).to_le_bytes())?;
        bw.write_all(&(idx_raw.len() as u64).to_le_bytes())?;
        bw.write_all(blake3::hash(&idx_raw).as_bytes())?;
        bw.write_all(MAGIC)?;
        bw.flush()?;
        Ok(())
    })?;
    a.st.out_bytes = pos + idx_comp.len() as u64 + 64;
    // AHORA SI ESTA TODO HECHO: la ultima linea la pinta el final, no el ultimo frame que tuvo
    // suerte con el freno de 200 ms. Forzada, o sea que se salta el freno. Y el latido para
    // ANTES, para que no le caiga una linea encima despues del 100%.
    latido.para();
    pinta_progreso(true);
    // Y se apaga el modelo de tres partes: en la MISMA ejecucion puede venir detras un `verify`,
    // que no tiene pool ni indice que comprimir y donde manda lo leido.
    PROG_POOL.store(false, std::sync::atomic::Ordering::Relaxed);
    pinta_fases(_fb, _ff);
    // EL TAMAÑO DEL INDICE, PARA PODER RE-CALIBRAR `prevision_indice`. Una constante que se usa
    // para prometerle algo al usuario tiene que poder comprobarse sin recompilar.
    if std::env::var("VC_FASES").map(|v| v != "0" && !v.is_empty()).unwrap_or(false) {
        let plano = PROG_PLANO_TOTAL.load(std::sync::atomic::Ordering::Relaxed);
        eprintln!("indice        {:>10.2} MB en claro   (la barra iba contando {:.2} MB, {:+.1}%)   {} ficheros, {} trozos, plano {:.0} MiB",
                  idx_raw.len() as f64 / 1e6, contado as f64 / 1e6,
                  100.0 * (contado as f64 - idx_raw.len() as f64) / idx_raw.len().max(1) as f64,
                  _ff, a.idx.chunks.len(), plano as f64 / 1048576.0);
    }
    Ok(a.st)
}

/// La clave en vigor para esta operacion, si la hay.
///
/// Va en un estatico como el nivel: la alternativa era un parametro mas en `comprimir_pleno`
/// —que ya tiene once— y en `extraer`, `abrir`, `leer_frame` y todo lo que cuelga. El CLI la
/// fija una vez al arrancar.
static CLAVE: std::sync::RwLock<Option<[u8; 32]>> = std::sync::RwLock::new(None);
/// La sal con la que se derivo la clave, para poder escribirla en la cabecera.
static SAL_EN_USO: std::sync::RwLock<Vec<u8>> = std::sync::RwLock::new(Vec::new());
/// Fija la sal que ira a la cabecera del archivo nuevo.
pub fn pon_sal(s: Vec<u8>) { *SAL_EN_USO.write().unwrap() = s; }

/// Fija la clave derivada de la contrasena. Sin llamarla, todo funciona como siempre.
pub fn pon_clave(k: Option<[u8; 32]>) { *CLAVE.write().unwrap() = k; }
fn clave() -> Option<[u8; 32]> { *CLAVE.read().unwrap() }

/// Lee la cabecera de cifrado de un archivo ya abierto. Devuelve (cifrado, sal, parametros).
pub fn cabecera_cripto(path: &Path) -> Result<(bool, Vec<u8>, cripto::Params)> {
    let mut f = File::open(path).with_context(|| format!("abrir {}", path.display()))?;
    let mut cab = [0u8; CABECERA];
    if f.read_exact(&mut cab).is_err() { return Ok((false, vec![], cripto::Params::default())); }
    if &cab[0..8] != MAGIC { return Ok((false, vec![], cripto::Params::default())); }
    let cifrado = cab[8] == 1;
    let sal = cab[9..25].to_vec();
    let p = cripto::Params {
        mib: u32::from_le_bytes(cab[25..29].try_into().unwrap()),
        pasadas: u32::from_le_bytes(cab[29..33].try_into().unwrap()),
        carriles: u32::from_le_bytes(cab[33..37].try_into().unwrap()),
    };
    Ok((cifrado, sal, p))
}

use std::sync::atomic::{AtomicI32, Ordering};
static NIVEL: AtomicI32 = AtomicI32::new(19);
fn set_nivel(n: i32) { NIVEL.store(n, Ordering::Relaxed); }
fn pf_nivel() -> i32 { NIVEL.load(Ordering::Relaxed) }
static CM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn set_cm(v: bool) { CM.store(v, Ordering::Relaxed); }
fn usar_cm() -> bool { CM.load(Ordering::Relaxed) }

/// Modo sólido: tamaño de frame en MiB, o 0 si está apagado.
static SOLIDO: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// LA VENTANA ES EL FRAME. La juntura mas cara que hemos encontrado, y no vivia en ninguna pieza.
///
/// Lo que habia aqui era `zstd::bulk::compress(&pf.data, lvl)`, que NO le pasa ni un parametro.
/// zstd usa entonces la ventana que trae por defecto para ese nivel — y esa ventana es FIJA POR
/// NIVEL, no crece con la entrada. Preguntado a la libreria y no a la documentacion:
/// `ZSTD_getCParams` devuelve windowLog=22 (4 MiB) para el nivel 9 con entradas de 8, 32, 64 y
/// 256 MiB, y windowLog=23 (8 MiB) para el 19. Le estabamos dando frames de 64 MiB a un compresor
/// que mira 4 MiB hacia atras: 16 a 1.
///
/// POR QUE NADIE LO VIO. El archivador elige el tamaño de frame y el codec elige la ventana. Las
/// dos decisiones son sensatas por separado, las dos piezas miden bien por separado, y el numero
/// que las descuadra no aparece en ninguna de las dos basculas. Solo se ve mirando la juntura.
///
/// MEDIDO (bench/ventana.py, 8 trozos de 64 MiB repartidos por todo el corpus, no los primeros):
///   ventana al frame          -1,365% de bytes y 0,932x de CPU  ← mas pequeño Y mas rapido
///   LDM con la ventana de hoy  +0,013% de bytes y 1,035x de CPU  ← el LDM sin alcance no sirve
///   las dos                   -1,617% de bytes y 1,015x de CPU
/// Se coge la primera: es la unica que gana en los dos ejes a la vez. El LDM pasa a ser una
/// decision de dinero, que es de quien es -0,26% mas de bytes por un 8,8% mas de CPU.
///
/// EL TOPE DE 27 NO ES UN CAPRICHO: es `ZSTD_WINDOWLOG_LIMIT_DEFAULT`, lo maximo que acepta un
/// descompresor SIN configurar. Por encima de ahi los archivos dejan de abrirse, y un archivo
/// mas pequeño que no vuelve no es una mejora. La sonda hace la ida y vuelta con el descompresor
/// tal cual viene justo para que ese techo salga en la medida y no en un cliente.
fn comprime_zstd(data: &[u8], lvl: i32) -> Result<Vec<u8>> {
    // El windowLog que cubre EXACTAMENTE este frame. Pedir mas gastaria memoria sin alcanzar
    // nada: no hay nada mas atras que mirar.
    let bits = usize::BITS - (data.len().max(2) - 1).leading_zeros();
    let wlog = bits.clamp(10, 27);
    let mut c = zstd::bulk::Compressor::new(lvl).context("crear compresor zstd")?;
    c.set_parameter(zstd::zstd_safe::CParameter::WindowLog(wlog))
        .context("fijar la ventana al tamaño del frame")?;
    c.compress(data).context("comprimir")
}

/// COMPRIMIR COMO COMPRIME EL PRODUCTO. La sonda de `--price` mide sobre una muestra suelta,
/// pero el producto no comprime muestras: comprime FRAMES. Y desde que la ventana es el frame,
/// comprimir la muestra entera de una vez le daria a zstd un alcance que en el producto no va a
/// tener nunca, o sea una prediccion optimista sobre la que se decide el precio.
///
/// Asi que se trocea igual y se suma. Cuesta lo mismo —son los mismos bytes— y ademas cierra la
/// juntura por construccion: si mañana cambia el tamaño de frame, la sonda cambia con el sin que
/// nadie se acuerde de tocarla.
///
/// Se usa el frame del carril TEXTO porque es el mas grande y es donde la decision de comprimir
/// de verdad se toma; el carril MEDIA va a nivel 3 pase lo que pase y el suyo no mueve el precio.
fn sonda_zstd(datos: &[u8], lvl: i32) -> Result<usize> {
    let fr = frame_target(LANE_TEXT as usize).max(1);
    let mut n = 0usize;
    for t in datos.chunks(fr) { n += comprime_zstd(t, lvl)?.len(); }
    Ok(n)
}

fn set_solido(m: usize) { SOLIDO.store(m, Ordering::Relaxed); }
fn solido() -> usize { SOLIDO.load(Ordering::Relaxed) }

/// PERILLA DE BANCO (F6·Q3): a donde volcar el contenido crudo de cada frame. `None` fuera del
/// banco, que es siempre salvo cuando se esta midiendo el arranque en frio.
fn banco_frames() -> Option<String> {
    static D: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    D.get_or_init(|| match std::env::var("VC_BANCO_FRAMES") {
        Ok(v) if !v.is_empty() => Some(v), _ => None }).clone()
}

/// Igual que `abrir`, pero devuelve tambien donde ACABAN los frames — que es donde empieza el
/// indice. Es el punto por el que hay que continuar para anadir una instantanea.
pub fn abrir_con_fin(path: &Path) -> Result<(Index, File, u64)> {
    let (idx, mut f) = abrir(path)?;
    f.seek(SeekFrom::End(-64))?;
    let mut foot = [0u8; 64]; f.read_exact(&mut foot)?;
    Ok((idx, f, u64::from_le_bytes(foot[0..8].try_into().unwrap())))
}

/// Anade una instantanea a un archivo que ya existe, reutilizando todo lo que ya estuviera.
///
/// LO QUE HACE ESPECIAL A ESTO. Los trozos cuyo contenido ya esta en el archivo NO se vuelven a
/// comprimir: se referencian. Un respaldo diario de un arbol que apenas cambia pasa de costar
/// lo mismo que el primero a costar solo lo nuevo, que es la diferencia entre ser un
/// archivador y ser una herramienta de copias de seguridad.
///
/// SOBRE LA SEGURIDAD DE ESCRIBIR ENCIMA. Se trabaja sobre una COPIA y solo se reemplaza el
/// original cuando ha salido bien. Escribir directamente seria mas rapido y dejaria el archivo
/// destruido si algo falla a mitad: frames nuevos, indice viejo pisado, y el original ya no
/// esta. Para un archivador ese riesgo no se compensa con nada.
/// Candado sobre un archivo mientras se escribe. Se suelta al soltarse esto.
///
/// El candado vive en el descriptor abierto, asi que lo limpia el NUCLEO cuando el proceso
/// termina — incluso si muere de golpe. Un fichero .lock aparte se habria quedado huerfano tras
/// un corte, y habria que explicarle al usuario como borrarlo a mano.
pub struct Candado(File);

impl Candado {
    /// Coge el candado o falla EN EL ACTO. No espera: si otro proceso esta escribiendo, lo que
    /// el usuario necesita es enterarse, no quedarse colgado sin saber por que.
    pub fn coge(archivo: &Path) -> Result<Candado> {
        use std::os::unix::io::AsRawFd;
        let f = std::fs::OpenOptions::new().read(true).open(archivo)
            .with_context(|| format!("abrir {} para bloquearlo", archivo.display()))?;
        let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if r != 0 {
            bail!("otro proceso esta escribiendo en {}. Espera a que acabe y vuelve a \
                   intentarlo; si estas seguro de que no hay ninguno, es que ha quedado un \
                   proceso colgado.", archivo.display());
        }
        Ok(Candado(f))
    }
}

pub fn anadir(archivo: &Path, entradas: &[PathBuf], nombre: &str, nivel: i32, jpeg: bool,
              agrupar: bool, maximo: bool, solido: usize) -> Result<Stats> {
    // EL CANDADO, lo PRIMERO. Antes de leer el indice: si se cogiera despues, dos procesos
    // podrian leer el mismo indice de partida y el segundo perderia la instantanea del primero
    // al renombrar encima. La carrera no esta en la escritura, esta en el hueco entre leer y
    // escribir.
    let _candado = Candado::coge(archivo)?;
    let (idx, _f, fin) = abrir_con_fin(archivo)?;
    if idx.chunks.iter().all(|c| c.hash == [0u8; 16]) && !idx.chunks.is_empty() {
        bail!("este archivo se escribio con una version anterior que no guardaba las huellas                de los trozos, asi que no se puede deduplicar contra el. Vuelve a crearlo con                esta version y a partir de ahi ya se le pueden anadir instantaneas.");
    }
    let tmp = archivo.with_extension("vc.parcial");
    std::fs::copy(archivo, &tmp).with_context(|| format!("copiar {} para trabajar sobre ella",
                                                         archivo.display()))?;
    let r = comprimir_con_base(entradas, &tmp, nivel, jpeg, agrupar, maximo, solido,
                               Some((idx, fin)), nombre);
    match r {
        Ok(st) => {
            // Antes de reemplazar nada: que el archivo nuevo se pueda ABRIR. Un indice
            // corrupto no se nota hasta que hace falta, y para entonces el original ya no
            // esta.
            abrir(&tmp).context("el archivo resultante no se puede abrir; NO se reemplaza el original")?;
            std::fs::rename(&tmp, archivo)?;
            Ok(st)
        }
        Err(e) => { let _ = std::fs::remove_file(&tmp); Err(e) }
    }
}

/// Borra una instantanea y compacta el archivo. Devuelve (bytes antes, bytes despues).
///
/// Una instantanea es el prefijo de ruta que le puso `add`, asi que borrarla es quitar los
/// ficheros que cuelgan de ese prefijo y tirar lo que deje de estar referenciado.
pub fn olvidar(archivo: &Path, nombre: &str) -> Result<(u64, u64)> {
    use std::io::{Seek, SeekFrom, Write};
    let _candado = Candado::coge(archivo)?;
    let antes = std::fs::metadata(archivo)?.len();
    let (idx, mut f) = abrir(archivo)?;

    // 1. Que ficheros se quedan.
    let pref = format!("{}/", nombre.trim_end_matches('/'));
    let quedan: Vec<&FileRec> = idx.files.iter().filter(|fr| !fr.path.starts_with(&pref)).collect();
    let fuera = idx.files.len() - quedan.len();
    if fuera == 0 {
        bail!("no hay ninguna instantánea «{nombre}» en este archivo. Mira las que hay con \
               `vcompresor list`.");
    }
    if quedan.is_empty() {
        bail!("«{nombre}» es todo lo que hay en el archivo: borrarla lo dejaría vacío. Si es lo \
               que quieres, borra el fichero .vc.");
    }

    // 2. Que trozos siguen haciendo falta, y por tanto que frames.
    let mut trozos_vivos: Vec<bool> = vec![false; idx.chunks.len()];
    for fr in &quedan {
        for &c in &fr.chunks {
            if (c as usize) < trozos_vivos.len() { trozos_vivos[c as usize] = true; }
        }
    }
    let mut frames_vivos: Vec<bool> = vec![false; idx.frames.len()];
    for (i, vivo) in trozos_vivos.iter().enumerate() {
        if *vivo {
            let fi = idx.chunks[i].frame as usize;
            if fi < frames_vivos.len() { frames_vivos[fi] = true; }
        }
    }

    // 3. Renumeracion. Los frames y trozos que se quedan cambian de numero al cerrarse los
    // huecos, y TODA referencia hay que traducirla: si se olvida una, el archivo queda
    // sintacticamente valido y apuntando a datos ajenos — corrupcion silenciosa.
    let mut mapa_frame: Vec<u32> = vec![u32::MAX; idx.frames.len()];
    let mut n = 0u32;
    for (i, v) in frames_vivos.iter().enumerate() { if *v { mapa_frame[i] = n; n += 1; } }
    let mut mapa_trozo: Vec<u64> = vec![u64::MAX; idx.chunks.len()];
    let mut m = 0u64;
    for (i, v) in trozos_vivos.iter().enumerate() { if *v { mapa_trozo[i] = m; m += 1; } }

    // 4. El archivo nuevo: los frames se copian TAL CUAL, sin descomprimir.
    let tmp = archivo.with_extension("vc.compactando");
    let resultado = (|| -> Result<()> {
        let salida = File::create(&tmp).with_context(|| format!("crear {}", tmp.display()))?;
        let mut bw = BufWriter::new(salida);
        bw.write_all(MAGIC)?;
        let mut pos: u64 = 8;
        let mut frames_nuevos: Vec<FrameRec> = Vec::with_capacity(n as usize);
        for (i, v) in frames_vivos.iter().enumerate() {
            if !*v { continue; }
            let fr = &idx.frames[i];
            f.seek(SeekFrom::Start(fr.offset))?;
            let mut buf = vec![0u8; fr.csize as usize];
            f.read_exact(&mut buf).with_context(|| format!("leer frame {i}"))?;
            bw.write_all(&buf)?;
            frames_nuevos.push(FrameRec { offset: pos, csize: fr.csize, rsize: fr.rsize,
                                          codec: fr.codec, lane: fr.lane });
            pos += fr.csize;
        }

        let mut idx2 = Index { nivel: idx.nivel, ..Default::default() };
        idx2.frames = frames_nuevos;
        for (i, v) in trozos_vivos.iter().enumerate() {
            if !*v { continue; }
            let c = &idx.chunks[i];
            idx2.chunks.push(ChunkRec { frame: mapa_frame[c.frame as usize], off: c.off,
                                        len: c.len, hash: c.hash });
        }
        // Los enlaces duros apuntan por INDICE de fichero, asi que tambien hay que traducirlos.
        let mut mapa_fich: HashMap<&str, u32> = HashMap::new();
        for (nuevo, fr) in quedan.iter().enumerate() { mapa_fich.insert(fr.path.as_str(), nuevo as u32); }
        for fr in &quedan {
            let mut nf = FileRec {
                path: fr.path.clone(), size: fr.size,
                chunks: fr.chunks.iter().map(|&c| mapa_trozo[c as usize]).collect(),
                hash: fr.hash, transform: fr.transform.clone(),
                mode: fr.mode, mtime: fr.mtime, link_target: fr.link_target.clone(),
                uid: fr.uid, gid: fr.gid, rdev: fr.rdev, enlace_a: None,
                // Al compactar se CONSERVAN: no se releen del disco, que ya no existe.
                xattr: fr.xattr.clone(),
            };
            if let Some(j) = fr.enlace_a {
                // Si el fichero al que apuntaba se ha ido con la instantanea borrada, este deja
                // de ser un enlace y se queda sin contenido. Es un caso que no deberia pasar
                // -los enlaces viven dentro de su instantanea- pero si pasa hay que decirlo, no
                // escribir una referencia a la nada.
                match mapa_fich.get(idx.files[j as usize].path.as_str()) {
                    Some(&nj) => nf.enlace_a = Some(nj),
                    None => bail!("el enlace duro {} apuntaba a un fichero de la instantánea \
                                   borrada; no se compacta para no dejarlo roto", fr.path),
                }
            }
            idx2.files.push(nf);
        }

        let crudo = formato::escribir(&idx2);
        // El indice va precedido de su BYTE DE CODEC, y el hash del pie cubre el conjunto. La
        // primera version escribio el zstd a pelo: el lector tomo su primer byte como codec y
        // dijo "codec de indice desconocido: 40". Se copia la forma del escritor de verdad en
        // vez de deducirla.
        let mut comp = vec![CODEC_ZSTD];
        comp.extend_from_slice(&comprime_zstd(&crudo, 19)?);
        bw.write_all(&comp)?;
        // El hash del pie cubre el indice EN CRUDO, no el comprimido. Lo tome sobre el
        // comprimido y salio "blake3 no coincide". Van tres veces deduciendo la forma del pie en
        // vez de leer el escritor de verdad, y las tres mal: la disposicion de un formato no se
        // adivina, se copia de quien lo escribe.
        let hash = blake3::hash(&crudo);
        let mut pie = [0u8; 64];
        pie[0..8].copy_from_slice(&pos.to_le_bytes());
        pie[8..16].copy_from_slice(&(comp.len() as u64).to_le_bytes());
        pie[16..24].copy_from_slice(&(crudo.len() as u64).to_le_bytes());
        pie[24..56].copy_from_slice(hash.as_bytes());
        pie[56..64].copy_from_slice(MAGIC);
        bw.write_all(&pie)?;
        bw.flush()?;
        Ok(())
    })();

    match resultado {
        Ok(()) => {
            // Que se pueda ABRIR antes de reemplazar nada. Borrar es la operacion en la que
            // menos se puede permitir un fallo a mitad.
            abrir(&tmp).context("el archivo compactado no se puede abrir; NO se reemplaza el original")?;
            let despues = std::fs::metadata(&tmp)?.len();
            std::fs::rename(&tmp, archivo)?;
            Ok((antes, despues))
        }
        Err(e) => { let _ = std::fs::remove_file(&tmp); Err(e) }
    }
}

pub fn abrir(path: &Path) -> Result<(Index, File)> {
    let mut f = File::open(path).with_context(|| format!("abrir {}", path.display()))?;
    let total = f.metadata()?.len();
    if total < 72 { bail!("archivo demasiado pequeño o corrupto"); }
    let mut head = [0u8; 8]; f.read_exact(&mut head)?;
    if !CONTENEDORES_LEIBLES.iter().any(|m| *m == &head) {
        bail!("formato de contenedor desconocido ({}): ¿archivo de una versión posterior? \
               Actualiza vcompresor.", String::from_utf8_lossy(&head));
    }
    f.seek(SeekFrom::End(-64))?;
    let mut foot = [0u8; 64]; f.read_exact(&mut foot)?;
    // el pie tiene que coincidir con la MISMA versión que la cabecera: si no, el archivo se
    // escribió a medias entre dos versiones y no es de fiar.
    if foot[56..64] != head { bail!("pie inválido: archivo truncado o corrupto"); }
    let idx_off = u64::from_le_bytes(foot[0..8].try_into().unwrap());
    let idx_csize = u64::from_le_bytes(foot[8..16].try_into().unwrap());
    let idx_rsize = u64::from_le_bytes(foot[16..24].try_into().unwrap());
    let hash: [u8; 32] = foot[24..56].try_into().unwrap();
    f.seek(SeekFrom::Start(idx_off))?;
    let mut comp = vec![0u8; idx_csize as usize];
    f.read_exact(&mut comp)?;
    if comp.is_empty() { bail!("índice vacío: archivo corrupto"); }
    // Descifrar el indice ANTES de mirar su byte de codec: cifrado, ese byte es ruido.
    let comp = match clave() { Some(k) => cripto::descifra(&k, &comp)?, None => comp };
    if comp.is_empty() { bail!("índice vacío tras descifrar: archivo corrupto"); }
    let raw = match comp[0] {
        CODEC_CM => vc_cm::descomprimir(&comp[1..]).map_err(|e| anyhow::anyhow!(e))?,
        CODEC_ZSTD => zstd::bulk::decompress(&comp[1..], idx_rsize as usize)?,
        c => bail!("códec de índice desconocido: {c}"),
    };
    if *blake3::hash(&raw).as_bytes() != hash { bail!("índice corrupto: blake3 no coincide"); }
    // Las huellas existen desde VCMP0013; los metadatos extendidos desde VCMP0014. Comparar
    // contra MAGIC a secas era correcto mientras MAGIC fuera 0013, y dejo de serlo al subirlo:
    // un archivo 0013 se habria leido como si no trajera huellas, y lo incremental habria
    // dejado de funcionar en silencio sobre todo lo ya escrito.
    // Se compara la VERSION, no se enumeran magias. Van dos veces que subo el MAGIC y olvido
    // actualizar estas dos lineas: la primera (0013->0014) habria hecho perder las huellas de
    // todo lo ya escrito, y la segunda (0014->0015) desincronizo el lector del indice y rompio
    // la extraccion INCLUSO SIN CIFRAR. Una lista que hay que acordarse de ampliar es una lista
    // que se olvida.
    let ver: u32 = std::str::from_utf8(&head[4..8]).ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let con_huellas = ver >= 13;
    let con_meta = ver >= 14;
    let con_xattr = ver >= 16;
    Ok((formato::leer_v(&raw, con_huellas, con_meta, con_xattr)?, f))
}

fn leer_frame(f: &mut File, fr: &FrameRec) -> Result<Vec<u8>> {
    f.seek(SeekFrom::Start(fr.offset))?;
    let mut comp = vec![0u8; fr.csize as usize];
    f.read_exact(&mut comp)?;
    // Descifrar ANTES de descodificar: lo que hay en disco es el bloque cifrado entero, y el
    // codec de dentro no se puede ni mirar sin la clave.
    let comp = match clave() { Some(k) => cripto::descifra(&k, &comp)?, None => comp };
    descodifica_frame(comp, fr)
}

/// Un frame puede venir en store, en zstd o en nuestro CM. El índice sale de un fichero que
/// puede estar corrupto, así que el códec desconocido se rechaza y el tamaño se comprueba.
pub fn descodifica_frame(comp: Vec<u8>, fr: &FrameRec) -> Result<Vec<u8>> {
    let v = match fr.codec {
        CODEC_STORE => comp,
        CODEC_ZSTD => zstd::bulk::decompress(&comp, fr.rsize as usize)?,
        CODEC_CM => {
            if comp.len() < 10 { bail!("frame CM truncado"); }
            vc_cm::descomprimir(&comp).map_err(|e| anyhow::anyhow!(e))?
        }
        c => bail!("códec de frame desconocido ({c}): ¿archivo de una versión posterior?"),
    };
    if v.len() as u64 != fr.rsize { bail!("frame descomprimido mide {} y el índice dice {}", v.len(), fr.rsize); }
    Ok(v)
}

/// Escribe el fichero SALTANDO los bloques que son todo ceros, en vez de escribirlos.
///
/// POR QUE. Un fichero disperso —una imagen de maquina virtual, una base de datos con huecos—
/// declara 100 MB y ocupa casi cero en disco. Al restaurarlo escribiendo byte a byte, esos
/// huecos se convertian en 100 MB de ceros REALES: medido, y suficiente para llenar el disco
/// al restaurar algo que cabia de sobra.
///
/// No hace falta guardar un mapa de huecos en el formato: basta con no escribir los ceros. Un
/// hueco leido devuelve ceros, asi que el contenido es identico byte a byte y `verify` sigue
/// siendo la misma comprobacion de siempre.
///
/// EFECTO LATERAL, asumido y dicho: un fichero que NO era disperso pero esta lleno de ceros
/// volvera disperso. Ocupa menos y su contenido es el mismo; para un archivador eso es correcto.
fn escribe_quiza_disperso(dest: &Path, datos: &[u8]) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    const BLOQUE: usize = 64 * 1024;
    let mut f = File::create(dest).with_context(|| format!("crear {}", dest.display()))?;
    let mut i = 0usize;
    let mut saltados = 0usize;
    while i < datos.len() {
        let fin = (i + BLOQUE).min(datos.len());
        let trozo = &datos[i..fin];
        if trozo.iter().all(|&b| b == 0) {
            saltados += trozo.len();
        } else {
            if saltados > 0 { f.seek(SeekFrom::Current(saltados as i64))?; saltados = 0; }
            f.write_all(trozo)?;
        }
        i = fin;
    }
    // El ultimo hueco no se escribe: se declara el tamano. Sin esto, un fichero que acaba en
    // ceros saldria mas corto de lo que era.
    f.set_len(datos.len() as u64)?;
    Ok(())
}

/// Fija atime y mtime al mismo segundo. `utimensat` directo: la alternativa era una
/// dependencia entera para envolver una llamada al sistema.
fn poner_mtime(dest: &Path, secs: i64, es_symlink: bool) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(dest.as_os_str().as_bytes()) else { return };
    let ts = libc::timespec { tv_sec: secs as libc::time_t, tv_nsec: 0 };
    let times = [ts, ts];
    let flags = if es_symlink { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
    unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), flags) };
}

/// Aplica permisos (solo ficheros) y mtime (ficheros y enlaces, sin seguir el enlace).
fn aplicar_meta(dest: &Path, fr: &FileRec, es_symlink: bool) {
    use std::os::unix::fs::PermissionsExt;
    // DUENO Y GRUPO. Solo funciona con privilegios; sin ellos, chown falla y NO se trata como
    // error: restaurar tus propios ficheros como usuario normal es un caso legitimo y el mas
    // frecuente. Lo que no puede volver a pasar es lo de antes, que era ni siquiera guardarlos.
    if fr.uid != 0 || fr.gid != 0 {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        if let Ok(c) = CString::new(dest.as_os_str().as_bytes()) {
            unsafe { libc::lchown(c.as_ptr(), fr.uid, fr.gid) };
        }
    }
    if !es_symlink && fr.mode != 0 {
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(fr.mode & 0o7777));
    }
    if !fr.xattr.is_empty() {
        pon_xattrs(dest, &fr.xattr);
    }
    if fr.mtime != 0 {
        poner_mtime(dest, fr.mtime, es_symlink);
    }
}

/// Recompone y verifica un tramo CONTIGUO del índice. Contiguo y no repartido a la redonda
/// a propósito: los ficheros vecinos comparten frame, así cada worker aprovecha su caché en
/// vez de que todos se peleen por los mismos frames.
/// Caché de frames COMPARTIDA entre los hilos de extracción. Sin ella, varios workers
/// descomprimen el mismo frame por su cuenta: con zstd apenas se nota, pero con el códec CM
/// —que va a ~0,9 MB/s— multiplica el tiempo de extracción por el número de hilos que se
/// solapan. Los frames son inmutables, así que compartirlos es gratis salvo el bloqueo.
struct EstadoCache {
    orden: std::collections::VecDeque<u32>,
    datos: HashMap<u32, Arc<Vec<u8>>>,
    /// Frames que alguien esta descodificando AHORA MISMO. Sin esto, doce hilos que fallan la
    /// caché a la vez descodifican los doce el mismo frame, cada uno con su modelo de 483 MiB.
    en_curso: std::collections::HashSet<u32>,
}
pub struct CacheFrames {
    m: Mutex<EstadoCache>,
    /// Se despierta cuando un frame termina de descodificarse (bien o mal).
    listo: std::sync::Condvar,
}
impl CacheFrames {
    fn nueva() -> Self {
        CacheFrames {
            m: Mutex::new(EstadoCache {
                orden: std::collections::VecDeque::new(),
                datos: HashMap::new(),
                en_curso: std::collections::HashSet::new(),
            }),
            listo: std::sync::Condvar::new(),
        }
    }
}
const CACHE_FRAMES: usize = 24;

/// Devuelve el frame descodificado, garantizando que **solo un hilo lo descodifica**.
///
/// La versión anterior miraba la caché, soltaba el candado y solo entonces descodificaba. Con
/// zstd apenas se notaba; con el códec CM —que arranca un modelo de 483 MiB por frame— hacía
/// que doce hilos construyeran doce modelos para el mismo bloque. Medido: 7,2 GB y 508% de CPU
/// para extraer un árbol de 5 MB que se había comprimido con 655 MB.
fn frame_cacheado(f: &mut File, idx: &Index, id: u32, c: &CacheFrames) -> Result<Arc<Vec<u8>>> {
    let mut g = c.m.lock().unwrap();
    loop {
        if let Some(v) = g.datos.get(&id) { return Ok(Arc::clone(v)); }
        // Nadie lo tiene y nadie lo está haciendo: me toca a mí. La marca se pone CON el
        // candado cogido, que es lo que hace que solo uno pase por aquí.
        if g.en_curso.insert(id) { break; }
        // Ya hay alguien; esperar a que avise en vez de hacer el mismo trabajo.
        g = c.listo.wait(g).unwrap();
    }
    drop(g); // descodificar FUERA del candado: es lo lento y no debe bloquear a nadie

    let res = leer_frame(f, &idx.frames[id as usize]);

    // Pase lo que pase —sobre todo si falló— hay que quitar la marca y despertar a los que
    // esperan. Un hilo esperando para siempre es peor que un error: el error se lee.
    let mut g = c.m.lock().unwrap();
    g.en_curso.remove(&id);
    let salida = match res {
        Ok(d) => {
            let datos = Arc::new(d);
            if !g.datos.contains_key(&id) {
                g.orden.push_back(id);
                g.datos.insert(id, Arc::clone(&datos));
                while g.orden.len() > CACHE_FRAMES {
                    if let Some(v) = g.orden.pop_front() { g.datos.remove(&v); }
                }
            }
            Ok(datos)
        }
        Err(e) => Err(e),
    };
    drop(g);
    c.listo.notify_all();
    salida
}

/// ¿Entra esta ruta en lo que ha pedido el usuario?
///
/// Se admiten tres formas, que son las que la gente escribe de verdad: la ruta exacta, un
/// directorio -y entonces entra todo lo que cuelga-, y un comodin como `*.log`. No se arrastra
/// una biblioteca de globs entera para esto.
pub fn casa_patron(ruta: &str, patron: &str) -> bool {
    if patron.is_empty() { return true; }
    let est = "*".chars().next().unwrap();
    if !patron.contains(est) {
        // exacta, o dentro del directorio pedido, o el nombre suelto del fichero
        return ruta == patron
            || ruta.starts_with(&format!("{}/", patron.trim_end_matches('/')))
            || ruta.rsplit('/').next() == Some(patron);
    }
    // Con comodin: se parte por el asterisco y se consumen los trozos en orden.
    let partes: Vec<&str> = patron.split(est).collect();
    let mut resto = ruta;
    for (i, parte) in partes.iter().enumerate() {
        if parte.is_empty() { continue; }
        match resto.find(parte) {
            Some(j) => {
                // Si el patron no empieza por comodin, el primer trozo tiene que ir al principio.
                if i == 0 && j != 0 { return false; }
                resto = &resto[j + parte.len()..];
            }
            None => return false,
        }
    }
    partes.last().map_or(true, |u| u.is_empty()) || resto.is_empty()
}

/// Reensambla una lista de trozos cualquiera. Existe porque el enlace duro sin destino necesita
/// el contenido de OTRO fichero, y el reensamblador de dentro del bucle solo sabe del suyo.
fn junta_lista(f: &mut File, idx: &Index, lista: &[u64], cache: &CacheFrames) -> Result<Vec<u8>> {
    let total: usize = lista.iter().map(|&c| idx.chunks[c as usize].len as usize).sum();
    let mut v = Vec::with_capacity(total);
    let mut ultimo: Option<(u32, Arc<Vec<u8>>)> = None;
    for &cid in lista {
        let c = &idx.chunks[cid as usize];
        let datos = match &ultimo {
            Some((id, d)) if *id == c.frame => Arc::clone(d),
            _ => {
                let d = frame_cacheado(f, idx, c.frame, cache)?;
                ultimo = Some((c.frame, Arc::clone(&d)));
                d
            }
        };
        v.extend_from_slice(&datos[c.off as usize..(c.off + c.len) as usize]);
    }
    Ok(v)
}

fn extraer_rango(arch: &Path, idx: &Index, rango: std::ops::Range<usize>, outdir: Option<&Path>, compartida: &CacheFrames, patron: Option<&str>) -> Result<(u64, u64, Vec<String>)> {
    let mut f = File::open(arch).with_context(|| format!("abrir {}", arch.display()))?;
    let (mut nf, mut nb) = (0u64, 0u64);
    let mut listar_fallos: Vec<String> = Vec::new();
    for fr in &idx.files[rango] {
        // EL FILTRO VA PRIMERO, antes de tocar un solo frame. Ese es todo el ahorro: sacar un
        // fichero de 4 KB de un archivo de 62 GB solo descomprime los frames que lo contienen,
        // porque los demas ficheros ni se miran. Aplicarlo despues de descomprimir daria la
        // misma salida y no ahorraria nada.
        if let Some(pat) = patron {
            if !casa_patron(&fr.path, pat) { continue; }
        }
        if let Some(target) = &fr.link_target {
            if huella(target.as_bytes()) != fr.hash {
                listar_fallos.push(format!("ROUND-TRIP FAIL: {} (destino de symlink no coincide)", fr.path));
            } else if let Some(d) = outdir {
                let dest = d.join(&fr.path);
                if let Some(p) = dest.parent() { std::fs::create_dir_all(p)?; }
                let _ = std::fs::remove_file(&dest);
                std::os::unix::fs::symlink(target, &dest)?;
                aplicar_meta(&dest, fr, true);
            }
            nb += target.len() as u64;
            nf += 1;
            continue;
        }
        // ENLACE DURO: no tiene contenido propio, comparte el del fichero al que apunta. Se
        // crea un enlace de verdad; escribir una copia seria restaurar un arbol distinto del
        // que se guardo, y ademas ocupando el doble.
        // ENLACE DURO. Normalmente aqui no se crea: la extraccion va por rangos EN PARALELO y
        // el fichero al que apunta puede no estar escrito todavia, asi que se hacen en una
        // segunda pasada cuando ya estan todos.
        //
        // PERO si hay filtro y el destino NO entra en el, no va a existir NADIE a quien
        // enlazar. Antes eso fallaba con "copiar a ...: No such file or directory" y no sacaba
        // nada: pedir un enlace duro suelto no devolvia el fichero. Y para el usuario los dos
        // nombres son EL MISMO fichero, asi que lo que quiere es su contenido, no una
        // explicacion sobre inodos. Se materializa aqui, en la ruta que pidio.
        if let Some(j) = fr.enlace_a {
            let destino_entra = patron.map_or(true, |p| casa_patron(&idx.files[j as usize].path, p));
            if destino_entra {
                nf += 1;
                continue;   // lo hara la segunda pasada, enlazando de verdad
            }
            if let Some(d) = outdir {
                let contenido = junta_lista(&mut f, idx, &idx.files[j as usize].chunks, compartida)?;
                let dest = d.join(&fr.path);
                if let Some(p) = dest.parent() { std::fs::create_dir_all(p)?; }
                escribe_quiza_disperso(&dest, &contenido)?;
                aplicar_meta(&dest, fr, false);
                nb += contenido.len() as u64;
            }
            nf += 1;
            continue;
        }
        // FIFOS Y DISPOSITIVOS. Antes ni se guardaban; ahora se recrean con mknod. Un socket
        // NO se recrea: no significa nada fuera del proceso que lo abrio.
        let tipo_fs = fr.mode & 0o170000;
        if matches!(tipo_fs, 0o010000 | 0o020000 | 0o060000) {
            if let Some(d) = outdir {
                use std::ffi::CString;
                use std::os::unix::ffi::OsStrExt;
                let dest = d.join(&fr.path);
                if let Some(p) = dest.parent() { std::fs::create_dir_all(p)?; }
                let _ = std::fs::remove_file(&dest);
                if let Ok(c) = CString::new(dest.as_os_str().as_bytes()) {
                    let r = unsafe { libc::mknod(c.as_ptr(), fr.mode as libc::mode_t, fr.rdev as libc::dev_t) };
                    if r != 0 {
                        // mknod de un dispositivo exige privilegios. Sin ellos no se puede, y
                        // hay que decirlo en vez de dejar un hueco silencioso en el arbol.
                        listar_fallos.push(format!(
                            "AVISO: {} es un fichero especial y no se pudo recrear (¿hacen falta privilegios?)",
                            fr.path));
                    } else {
                        aplicar_meta(&dest, fr, false);
                    }
                }
            }
            nf += 1;
            continue;
        }
        // Directorio vacío: no hay contenido que verificar, sólo hay que volver a crearlo.
        if fr.mode & 0o170000 == 0o040000 {
            if let Some(d) = outdir {
                let dest = d.join(&fr.path);
                std::fs::create_dir_all(&dest)?;
                aplicar_meta(&dest, fr, false);
            }
            nf += 1;
            continue;
        }
        let junta = |f: &mut File| -> Result<Vec<u8>> {
            let total: usize = fr.chunks.iter().map(|&c| idx.chunks[c as usize].len as usize).sum();
            let mut v = Vec::with_capacity(total);
            let mut ultimo: Option<(u32, Arc<Vec<u8>>)> = None;
            for &cid in &fr.chunks {
                let c = &idx.chunks[cid as usize];
                let datos = match &ultimo {
                    Some((id, d)) if *id == c.frame => Arc::clone(d),
                    _ => {
                        let d = frame_cacheado(f, idx, c.frame, compartida)?;
                        ultimo = Some((c.frame, Arc::clone(&d)));
                        d
                    }
                };
                v.extend_from_slice(&datos[c.off as usize..(c.off + c.len) as usize]);
            }
            Ok(v)
        };
        let salida: Vec<u8> = match &fr.transform {
            None => junta(&mut f)?,
            Some(Transform::Deflate { esqueleto, huecos }) => {
                let plano = junta(&mut f)?;
                match rehace_deflate(esqueleto, huecos, &plano) {
                    Ok(v) => v,
                    Err(e) => {
                        listar_fallos.push(format!("ROUND-TRIP FAIL: {} ({e})", fr.path));
                        nf += 1;
                        continue;
                    }
                }
            }
            Some(Transform::E8e9) => {
                let mut v = junta(&mut f)?;
                e8e9(&mut v, false);
                v
            }
            Some(Transform::LogTs) => {
                let plano = junta(&mut f)?;
                match log_ts_inv(&plano) {
                    Some(v) => v,
                    None => {
                        listar_fallos.push(format!("ROUND-TRIP FAIL: {} (reconstrucción log-ts fallida)", fr.path));
                        nf += 1;
                        continue;
                    }
                }
            }
            Some(Transform::JpegJxl) => {
                let flujo = junta(&mut f)?;
                match jxl::jxl_a_jpeg(&flujo, fr.size as usize) {
                    Some(v) => v,
                    None => {
                        listar_fallos.push(format!("ROUND-TRIP FAIL: {} (reconstrucción JXL fallida)", fr.path));
                        nf += 1;
                        continue;
                    }
                }
            }
        };
        if huella(&salida) != fr.hash {
            listar_fallos.push(format!("ROUND-TRIP FAIL: {} (blake3 no coincide)", fr.path));
        } else if let Some(d) = outdir {
            let dest = d.join(&fr.path);
            if let Some(p) = dest.parent() { std::fs::create_dir_all(p)?; }
            escribe_quiza_disperso(&dest, &salida)?;
            aplicar_meta(&dest, fr, false);
        }
        nb += salida.len() as u64;
        nf += 1;
    }
    Ok((nf, nb, listar_fallos))
}

/// outdir None = verificación pura. Devuelve (ficheros, bytes_originales, fallos).
/// Reparte los ficheros en tramos contiguos entre hilos; cada uno abre su propio descriptor.
pub fn extraer(arch: &Path, outdir: Option<&Path>, listar_fallos: &mut Vec<String>) -> Result<(u64, u64, u64)> {
    extraer_patron(arch, outdir, listar_fallos, None)
}

/// Igual que `extraer`, pero solo lo que case con `patron`.
///
/// Descomprime UNICAMENTE los frames que contienen esos ficheros: los demas ni se miran. Es lo
/// que convierte "recuperar un fichero" en una operacion de segundos en vez de desplegar el
/// archivo entero.
pub fn extraer_patron(arch: &Path, outdir: Option<&Path>, listar_fallos: &mut Vec<String>, patron: Option<&str>) -> Result<(u64, u64, u64)> {
    let (idx, _f) = abrir(arch)?;
    // El chequeo de rutas va ANTES de repartir: si el índice es hostil, no se toca el disco.
    //
    // 🚨 ESTABA MAL EN LAS DOS DIRECCIONES A LA VEZ. Era `if fr.path.contains("..")`, y eso:
    //
    //   - RECHAZABA NOMBRES LEGITIMOS. Un fichero llamado «version..2026.tar» dispara el bail, y
    //     este bucle recorre TODOS los ficheros ANTES de tocar el disco: un solo nombre asi deja
    //     el archivo ENTERO inextraible, y no hay manera de sacar lo demas.
    //
    //   - Y NO CAZABA EL CASO PELIGROSO. `Path::join` con una ruta ABSOLUTA reemplaza la base, y
    //     extraer_rango hace `outdir.join(&fr.path)`: un indice con «/etc/passwd» escribia FUERA
    //     del directorio de salida, y `contains("..")` no lo veia.
    //
    // Lo que hay que rechazar es el COMPONENTE `..` —no dos puntos seguidos en cualquier sitio—
    // y cualquier ruta que no sea relativa. `Path::components()` da exactamente eso, y ademas
    // entiende los prefijos de Windows (C:\), que un `starts_with("/")` dejaria pasar.
    for fr in &idx.files {
        let malo = Path::new(&fr.path).components().any(|c| matches!(c,
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_)));
        if malo { bail!("ruta insegura en índice: {}", fr.path); }
    }
    let n = idx.files.len();
    if n == 0 { return Ok((0, 0, 0)); }
    let ncpu = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(4);
    let mut nw = ncpu.clamp(1, 12).min(n);

    // Con vuelo único ya no se descodifica un frame dos veces, pero cuando hay MUCHOS frames
    // distintos siguen pudiendo estar varios modelos vivos a la vez. Comprimir tiene una
    // guardia de memoria desde hace tiempo; descomprimir no tenía ninguna, y esa asimetría es
    // la que permite escribir un archivo en una máquina de 64 GB que no abre en una de 8.
    let frames_cm = idx.frames.iter().filter(|f| f.codec == CODEC_CM).count();
    if frames_cm > 0 {
        nw = nw.min(frames_cm.max(1));
        let por_hilo_mb = 2048usize;
        let techo_mb: usize = std::env::var("VC_RAM_MB").ok().and_then(|v| v.parse().ok())
            .unwrap_or(24 * 1024);
        nw = nw.min((techo_mb / por_hilo_mb).max(1));
    }
    let por = n.div_ceil(nw);
    let idxr = &idx;
    let cache = CacheFrames::nueva();
    let cref = &cache;
    let partes = std::thread::scope(|s| -> Result<Vec<(u64, u64, Vec<String>)>> {
        let mut hs = Vec::new();
        let mut ini = 0usize;
        while ini < n {
            let fin = (ini + por).min(n);
            hs.push(s.spawn(move || extraer_rango(arch, idxr, ini..fin, outdir, cref, patron)));
            ini = fin;
        }
        let mut v = Vec::with_capacity(hs.len());
        for h in hs { v.push(h.join().map_err(|_| anyhow::anyhow!("worker de extracción con panic"))??); }
        Ok(v)
    })?;
    // SEGUNDA PASADA: los enlaces duros, ahora que TODOS los ficheros estan escritos. En serie
    // y con el arbol completo delante, un enlace no puede fallar por llegar antes que su
    // destino — que es justo lo que pasaba al hacerlos dentro de los rangos paralelos: el
    // enlace fallaba, el `?` abortaba el rango, y se perdian ficheros que no tenian nada que
    // ver. La prueba aislada no lo veia porque los dos caian en el mismo rango y en orden.
    if let Some(d) = outdir {
        for fr in &idx.files {
            let Some(j) = fr.enlace_a else { continue };
            if let Some(pat) = patron {
                if !casa_patron(&fr.path, pat) { continue; }
                // Si el DESTINO no entra en el filtro, la primera pasada ya materializo el
                // contenido en la ruta pedida y aqui no hay nada que enlazar. Sin este guardia
                // se intentaba enlazar a un fichero que no existe, fallaba la copia de respaldo
                // y reventaba la extraccion ENTERA — que es como el arreglo de la primera
                // pasada parecia no funcionar.
                if !casa_patron(&idx.files[j as usize].path, pat) { continue; }
            }
            let origen = d.join(&idx.files[j as usize].path);
            let dest = d.join(&fr.path);
            if let Some(p) = dest.parent() { std::fs::create_dir_all(p)?; }
            let _ = std::fs::remove_file(&dest);
            if let Err(e) = std::fs::hard_link(&origen, &dest) {
                // Si de verdad no se puede enlazar -otro sistema de ficheros- se copia: un
                // arbol correcto en contenido vale mas que un fallo. Pero se dice, porque deja
                // de ser el mismo arbol que se guardo.
                listar_fallos.push(format!("AVISO: {} no se pudo enlazar a {} ({e}); se copia",
                                           fr.path, idx.files[j as usize].path));
                std::fs::copy(&origen, &dest).with_context(|| format!("copiar a {}", dest.display()))?;
            }
            aplicar_meta(&dest, fr, false);
        }
    }
    // Se recomponen en orden de tramo, así el listado de fallos es siempre el mismo.
    let (mut nf, mut nb, mut nfail) = (0u64, 0u64, 0u64);
    for (a, b, fs) in partes {
        nf += a;
        nb += b;
        nfail += fs.len() as u64;
        listar_fallos.extend(fs);
    }
    Ok((nf, nb, nfail))
}

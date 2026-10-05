// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! LA FASE 3 DE VDEFLATE: estimar los parametros del codificador original en vez de describir su
//! salida. Esto mide si se puede Y SI SALE A CUENTA, que no es lo mismo.
//!
//! DE DONDE VIENE. vdeflate funciona en las dos direcciones sobre los 1.973 flujos del banco
//! pero guarda el 104,51% del DEFLATE contra el 1,70% de preflate. El 98,8% de ese coste es
//! describir las copias. La fase 3 es la idea de preflate: esas copias no son informacion
//! arbitraria, son lo que un buscador ENCONTRO — asi que si corremos un buscador parecido sobre
//! el mismo plano deberia encontrar las mismas, y solo hay que guardar donde nos equivocamos.
//!
//! ================ EL DINERO MANDA, Y POR ESO ESTO CUENTA LAS DOS COSAS ================
//!
//! «Mejor que preflate» NO es «menos bytes». Es menos EUROS, y los euros son dos sumandos:
//!
//!     coste = bytes_guardados x precio_del_disco  +  segundos_de_CPU x precio_de_la_maquina
//!
//! preflate tarda 172,97 s sobre este banco (inflar + estimar + re-deflate de verificacion).
//! Nuestro inflate tarda 10,32 s. Si la fase 3 nos deja 17 veces mas rapidos, unos puntos mas
//! de bytes pueden salir a favor. O no. Se calcula, no se opina — y por eso la salida de este
//! programa termina en euros por PB y no en porcentajes.
//!
//! ================ EL PRECOMPUTO, Y POR QUE HACIA FALTA ================
//!
//! La primera version costaba 2,55 s por flujo: 84 minutos el banco entero, contra los 172,97 s
//! de preflate. Un estimador que cuesta 29 veces mas que aquello a lo que sustituye no es una
//! mejora aunque acierte mas. El motivo eran NUEVE pasadas — nueve construcciones de la cadena
//! de hash sobre el mismo plano y nueve recorridos por posicion.
//!
//! Las tres reglas de la norma de precomputar se cumplen aqui de manual:
//!   1. el dominio es pequeño   -> por posicion hacen falta 9 pares (longitud, distancia)
//!   2. las entradas se repiten -> las nueve configuraciones preguntan por LA MISMA posicion
//!   3. el calculo cuesta mas   -> recorrer la cadena son hasta 4096 pasos; consultar, uno
//! Y la cuarta, la que decide si merece la pena: de 9 pasadas a 1.
//!
//! COMO SE CONTESTA A LAS NUEVE CON UN SOLO RECORRIDO. Las configuraciones solo se diferencian
//! en CUANDO paran: por `max_chain` -cuantos candidatos miran- o por `nice_length` -cuando se
//! dan por satisfechas-. Y la mejor longitud SOLO PUEDE CRECER segun se avanza por la cadena.
//! Asi que un unico recorrido hasta el tope mayor, guardando una foto al pasar por cada umbral
//! de cadena y otra al alcanzar cada umbral de `nice`, las contesta todas.
//!
//! COMO SE PREDICE, sin desincronizarse. La reconstruccion va guiada por el flujo ORIGINAL: en
//! cada simbolo se pregunta «¿que habria hecho el codificador AQUI?», se compara, y se avanza
//! segun el original y no segun la prediccion. Asi una divergencia no arrastra a las siguientes.
//!
//!   uso:  vc-fase3 [directorio-del-banco] [max-flujos]

use std::path::PathBuf;
use vc_core::inflate::{descomprime_con_bloques, Bloque, Token};

const VENTANA: usize = 32 * 1024;

/// EL ALCANCE DE VERDAD DE zlib, QUE NO ES LA VENTANA ENTERA.
///
/// `deflate.h:293-298`:
///     #define MIN_LOOKAHEAD (MAX_MATCH+MIN_MATCH+1)      // 258 + 3 + 1 = 262
///     #define MAX_DIST(s)   ((s)->w_size-MIN_LOOKAHEAD)  // 32768 - 262 = 32506
///
/// zlib se reserva 262 bytes al final de la ventana porque necesita tener por delante una
/// coincidencia maxima entera (258) mas los tres bytes que insertar despues. Nosotros
/// aceptabamos hasta 32.768, o sea que predeciamos coincidencias a distancias que el
/// codificador original NO PODIA producir — y eso sale contado como «otra distancia».
///
/// ⚠️ Y EL PRIMER CANDIDATO NO TIENE EL MISMO TOPE QUE LOS DEMAS. En zlib son dos sitios
/// distintos: la cabeza se comprueba en deflate_fast/deflate_slow con
///     s->strstart - hash_head <= MAX_DIST(s)          -> distancia <= 32506
/// y el recorrido de la cadena, dentro de longest_match, para con
///     } while ((cur_match = prev[...]) > limit && ...) con limit = strstart - MAX_DIST
/// que al ser ESTRICTO deja fuera la distancia justa: los saltos 1 y siguientes llegan a 32505.
/// preflate modela exactamente esa diferencia de uno (`cur_max_dist_hop0` contra
/// `cur_max_dist_hop1_plus`), que es la pista de que no es un detalle: si no, no la habrian
/// separado en dos variables.
const MIN_MIRADA: usize = MAX_COPIA + MIN_COPIA + 1;
const MAX_DIST: usize = VENTANA - MIN_MIRADA;

/// `VC_MAXDIST=0` vuelve al alcance de la ventana entera, que es lo que habia, para medir.
static ALCANCE_ZLIB: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_MAXDIST").map(|v| v != "0").unwrap_or(true));
const MASCARA: usize = VENTANA - 1;
// LA TABLA ES DE 16 BITS Y CADA HASH USA LOS SUYOS. Estaba en 15 —los de zlib— y eso recortaba
// el hash de libdeflate, que da 16: dos cubos suyos caian en uno nuestro y su fidelidad salia
// peor de lo que es. La tabla la dimensiona el hash MAS ANCHO; cada uno se enmascara con lo que
// le toca, que es lo que hacen ellos de verdad (el censo lee `Zlib{hash_mask:32767}`).
/// LOS CUBOS DE LA TABLA DE HASH. La tabla de constantes lo tenia «A OJO»: *«1<<16, un bit mas
/// que la tabla de zlib (1<<15). Deliberado para dar sitio al hash de libdeflate, que da 16 bits.
/// NUNCA MEDIDO si sobra o falta»*. `VC_CUBOS_BITS=n` lo mueve para medirlo.
static CUBOS: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| {
    let b: u32 = std::env::var("VC_CUBOS_BITS").ok().and_then(|v| v.parse().ok()).unwrap_or(16);
    1usize << b.clamp(10, 24)
});
/// zlib enmascara a 15 bits, no a los 16 de la tabla. Es SU mascara real, la que el censo lee
/// en cada flujo (`hash_mask: 32767`), y usarla mas ancha seria simular un zlib que no existe.
const MASCARA_ZLIB: usize = (1 << 15) - 1;

/// La politica de diccionario: 0 = AddAll (lo de siempre), n = AddFirst(n).
///
/// El censo (bin/vc-censo-codificadores) dice que el 30,6% de los flujos NO usa AddAll: 512
/// AddFirst(n), 48 AddFirstAndLast(n), 26 la variante de miniz y 17 la de zlib-ng. Nosotros
/// hacemos AddAll SIEMPRE, asi que en esos flujos nuestra cadena tiene candidatos que la del
/// codificador original NO TENIA.
static POL_ADD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
/// LA PERILLA, que ahora ANULA la politica que toca por nivel en vez de ser la unica que hay.
///
/// Devuelve `None` cuando nadie la ha puesto, y entonces manda `POL_DE_NIVEL` — que es lo que
/// hace zlib de verdad. `VC_ADD=n` la fuerza para todas las configuraciones (asi se midio el
/// +27,50 a mano), y el `--oraculo` la fuerza por flujo con lo que dice el censo.
fn perilla_add() -> Option<usize> {
    let v = POL_ADD.load(std::sync::atomic::Ordering::Relaxed);
    if v != usize::MAX { return Some(v) }
    match std::env::var("VC_ADD").ok().and_then(|x| x.parse().ok()) {
        Some(d) => { POL_ADD.store(d, std::sync::atomic::Ordering::Relaxed); Some(d) }
        None => None,
    }
}

/// ¿Se usa el hash de libdeflate en vez del de zlib? `VC_HASH=libdeflate` lo enciende.
///
/// Existe para UNA pregunta, la mas barata de las tres que abre el censo: ¿mueve el HASH la
/// aguja en los flujos de libdeflate, o lo que nos limita son los parametros y la politica de
/// diccionario? Se contesta corriendo solo sobre esos 609 flujos —el 10% de los simbolos— en
/// vez de re-medir los 1.973, que cuesta 75 minutos.
///
/// ⚠️ ERA UN BOOLEANO Y AHORA SON CINCO, Y NO ES generalidad gratuita: lo pidio el cruce del
/// 02-08 (bench/cruza-f3.py). Sobre el banco nuevo, los tres hashes que NO simulabamos se llevan
/// el 41,8% de todo lo que falla desde el 2,9% de los flujos:
///
///     hash            flujos     simbolos   acierto   % del hueco
///     Crc32cHash          28    8.924.930    65,26%      32,1%     <- 28 flujos, un TERCIO
///     MiniZFast           13    1.460.989    54,59%       6,9%
///     ZlibNG              31    1.411.674    81,04%       2,8%
///
/// Y en PNG —que es el 94,4% del hueco— el par Crc32c x Lazy(4,4) son 27 flujos con el 34,0%.
/// Veintisiete de dos mil quinientos veintidos.
pub const H_ZLIB: u8 = 0;
pub const H_LIBDEFLATE: u8 = 1;
pub const H_MINIZ: u8 = 2;
pub const H_ZLIBNG: u8 = 3;
pub const H_CRC32C: u8 = 4;
const H_SIN_DECIDIR: u8 = 255;
static HASH_LD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(H_SIN_DECIDIR);
fn hash_actual() -> u8 {
    let v = HASH_LD.load(std::sync::atomic::Ordering::Relaxed);
    if v != H_SIN_DECIDIR { return v }
    let d = match std::env::var("VC_HASH").as_deref() {
        Ok("libdeflate") => H_LIBDEFLATE,
        Ok("miniz") => H_MINIZ,
        Ok("zlibng") => H_ZLIBNG,
        Ok("crc32c") => H_CRC32C,
        _ => H_ZLIB,
    };
    HASH_LD.store(d, std::sync::atomic::Ordering::Relaxed);
    d
}
/// `VC_TOOFAR=1` vuelve a aplicar el descarte de TOO_FAR a las NUEVE configuraciones, que es lo
/// que se hacia antes de leer deflate.c. Existe solo para poder medir la diferencia.
///
/// ⚠️ Va en un LazyLock y no en un `env::var` a pelo, y no es estilo: la primera version leia la
/// variable DENTRO de `mejor_todos`, que corre una vez por posicion del flujo. Para el banco
/// entero son 257 millones de consultas al entorno —cada una con su cerrojo y su recorrido
/// lineal de environ— para leer un booleano que no cambia. El acierto no se falseaba (las dos
/// ramas pagaban igual), pero el reloj de esa medida no significaba nada.
/// EL HASH SE ESTIMA. Es lo UNICO que queda de estimacion de verdad en la fase 3.
///
/// La politica de diccionario resulto NO ser estimacion —la determina el nivel, deflate.h:189—
/// pero el hash si: lo decide QUE BIBLIOTECA escribio el flujo, y eso no se deduce de nada que
/// tengamos. El censo dice que un tercio del banco es libdeflate, y cambiar solo el hash sobre
/// esos flujos vale +6,16 puntos (0f32bdb).
///
/// ⚠️ LA FORMA OBVIA ERA LA CARA, Y POR ESO NO SE HACE ASI. Lo natural seria duplicar las nueve
/// configuraciones —cada nivel con hash de zlib y con hash de libdeflate— y dejar que gane la
/// mejor. Pero el hash cambia la CADENA, o sea el estado, igual que la politica: dieciocho
/// configuraciones son OCHO pasadas en vez de cuatro, y las cuatro nuevas incluyen las
/// perezosas, que son las caras (max_chain 128-4096). El reloj casi se dobla para ganar unas
/// seis decimas sobre el total. Eso es matar moscas a canonazos.
///
/// Lo barato es preguntarselo AL ESTIMADOR, que ya esta escrito y ya lo usamos para el censo:
/// preflate enumera los algoritmos de hash reales y devuelve el que explica el flujo con el
/// recorrido de cadena mas corto. Una pasada suya cuesta una fraccion de una nuestra.
///
/// O sea que cada cosa la decide quien sabe: el NIVEL sale de puntuar el prefijo, la POLITICA
/// del nivel, y el HASH del estimador.
///
/// `VC_ESTIMA_HASH=0` lo apaga (todo con el hash de zlib, que es lo que habia). `--oraculo` sigue
/// mandando por encima: es la referencia contra la que se mide si el estimador acierta.
static ESTIMA_HASH: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_ESTIMA_HASH").map(|v| v != "0").unwrap_or(true));

thread_local! {
    /// Se levanta MIENTRAS corre el estimador, y solo entonces se callan los panicos.
    static DENTRO_DEL_ESTIMADOR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// ¿Usa este flujo el hash de libdeflate? Lo dice el estimador de preflate.
///
/// ⚠️ SE ENVUELVE EN catch_unwind PORQUE PREFLATE REVIENTA. No es defensa por si acaso: esta
/// medido que entra en panico en el 8,7% de los flujos reales del banco, y un panico suyo mataria
/// el proceso entero a mitad de una medida de una hora.
///
/// ⚠️ Y EL HOOK NO SE SILENCIA A LO BRUTO, que es lo que hace vc-censo-codificadores. Ahi da
/// igual porque el censo no hace nada mas; aqui callar TODOS los panicos taparia uno nuestro, que
/// es justo el que hay que ver. El aviso solo se traga si el panico llega mientras estamos dentro
/// del estimador.
///
/// ⚠️ Y SE MIRA EL ENUM, NO SU `Debug`. La primera version hacia
/// `format!("{:?}", q.hash_algorithm).starts_with("Libdeflate")`. Funcionaba, pero ataba nuestra
/// decision al TEXTO que preflate imprime al depurar: si algun dia renombran una variante o le
/// cambian el derive, esto empieza a contestar «zlib» a todo y no lo caza ni un test ni el
/// compilador. Con el enum, renombrar una variante es un error de compilacion.
/// TODO LO QUE EL ESTIMADOR SABE DEL FLUJO — Y NOSOTROS TIRABAMOS CINCO SEPTIMOS.
///
/// 🔑 Llamabamos a `parametros_estimados` en CADA flujo y nos quedabamos con dos campos: el hash
/// y el minimo. Los otros cinco —cadena, `nice`, `good_length`, `max_lazy` y politica— describen
/// exactamente los cinco campos de nuestro `Cfg`. Los tirabamos para luego elegir a ciegas entre
/// nueve presets fijos.
///
/// Y preflate NO ADIVINA EL NIVEL: `complevel_estimator.rs:96` hace
///
///     let max_chain = candidate.max_chain_found() + 1;
///
/// o sea MIDE cuanto bajo por la cadena el codificador original y usa eso. Por construccion su
/// simulacion nunca busca mas hondo que el original, asi que hay una clase entera de fallos que
/// no puede tener: encontrar coincidencias que el otro no podia ver. El censo lo confirma —las
/// cadenas estimadas son 123, 124, 125, 126, 127, 129, 74, 75, 37, 33...— y nuestra tabla de
/// nueve solo sabe decir 4, 8, 16, 32, 128, 256, 1024 y 4096.
#[derive(Clone, Copy)]
struct Est {
    hash: u8,
    min: usize,
    cadena: usize,
    perezoso: bool,
    buena: usize,
    lazy: usize,
    nice: usize,
    /// LA DISTANCIA MAXIMA A LA QUE ESTE CODIFICADOR EMITE COPIAS DE TRES BYTES.
    ///
    /// 🔑 Nosotros teniamos `DEMASIADO_LEJOS = 4096` a fuego — la constante TOO_FAR de zlib— y
    /// preflate lo lleva como PARAMETRO ajustado por flujo (`max_dist_3_matches`): la mayor
    /// distancia a la que el flujo REALMENTE trae una copia de tres. Su nivel 1 de prueba lo
    /// tiene en 32.488, ocho veces la constante de zlib.
    ///
    /// Y es medible que duele: el canal «lit?» —predecir literal donde habia copia— nos cuesta
    /// 0,516% contra su 0,083%, o sea que nos equivocamos 7,3 veces mas. Cada copia de tres a
    /// mas de 4 KiB que ellos emiten y nosotros descartamos es exactamente un fallo de ese canal.
    ///
    /// ⚠️ Un 0 aqui NO significa «no lo se»: significa que el flujo no trae NINGUNA copia de tres,
    /// y entonces descartarlas todas es lo correcto. Es su semantica y se respeta.
    dist3: usize,
    pol: usize,
    /// La politica estimada NO se puede expresar con nuestro `pol` (AddFirstAndLast y las dos
    /// variantes de frontera de miniz/zlib-ng). Se aproxima y se CUENTA.
    pol_aprox: bool,
    /// El estimador revento y esto es el respaldo, no una estimacion.
    fallo: bool,
}

impl Default for Est {
    fn default() -> Self {
        // Si el estimador revienta —lo hace en el 8,7% de los flujos— se cae al nivel 6, que es
        // el que mas veces gana. Se CUENTAN aparte: sin eso, un modo que solo sabe funcionar en
        // el 91% de los flujos parece que funciona en todos.
        Est { hash: H_ZLIB, min: MIN_COPIA, cadena: usize::MAX,
              perezoso: CFGS[5].perezoso, buena: CFGS[5].buena, lazy: CFGS[5].lazy,
              nice: CFGS[5].nice, dist3: DEMASIADO_LEJOS, pol: 0, pol_aprox: false, fallo: true }
    }
}

/// SUS parametros traducidos a nuestro `Est`. Vive aparte porque lo usan DOS caminos —el de
/// siempre y el de un solo parseo— y dos copias de un mapeo acaban divergiendo justo en la
/// variante que se anadio despues.
fn est_de(q: preflate_rs::TokenPredictorParameters) -> Est {
    use preflate_rs::HashAlgorithm as HA;
    use preflate_rs::MatchingType as MT;
    use preflate_rs::DictionaryAddPolicy as DAP;
    let (perezoso, buena, lazy) = match q.matching_type {
        // En voraz no hay ni umbral de «buena» ni mirada perezosa: se pone `buena` al
        // infinito para que la reduccion de zlib (`c >>= 2`) no se dispare nunca.
        MT::Greedy => (false, usize::MAX, 0),
        MT::Lazy { good_length, max_lazy } => (true, good_length as usize, max_lazy as usize),
    };
    // `AddFirst(n)`: si la coincidencia pasa de n, solo la primera posicion. Es
    // exactamente nuestro `pol = n`. ⚠️ `AddFirst(0)` chocaria con nuestro «0 = AddAll»;
    // no aparece en el banco, y si algun dia aparece se cuenta como aproximada.
    let (pol, pol_aprox) = match q.add_policy {
        DAP::AddAll => (0usize, false),
        DAP::AddFirst(n) if n > 0 => (n as usize, false),
        DAP::AddFirst(_) => (1usize, true),
        DAP::AddFirstAndLast(n) => ((n as usize).max(1), true),
        // Las dos de frontera son AddFirst(0) mas una regla de bloque que no sabemos
        // expresar. Se aproximan a «solo la primera» y se cuentan.
        _ => (1usize, true),
    };
    Est {
        hash: match q.hash_algorithm {
            HA::Libdeflate4 | HA::Libdeflate4Fast => H_LIBDEFLATE,
            HA::MiniZFast => H_MINIZ,
            HA::ZlibNG => H_ZLIBNG,
            HA::Crc32cHash => H_CRC32C,
            // Zlib{..}, RandomVector y None van al de zlib. RandomVector no es un descarte:
            // son 184 flujos que ya sacan el 98,02% con el de zlib, o sea que simular el suyo
            // no compraria casi nada y costaria otra rama en el bucle mas caliente.
            _ => H_ZLIB,
        },
        // El minimo que declara el estimador, acotado a lo que el formato permite. Un 0
        // significa «no lo se» y entonces vale el del formato.
        min: (q.min_len as usize).clamp(MIN_COPIA, MAX_COPIA),
        cadena: (q.max_chain as usize).max(1),
        nice: (q.nice_length as usize).max(MIN_COPIA),
        dist3: q.max_dist_3_matches as usize,
        perezoso, buena, lazy, pol, pol_aprox, fallo: false,
    }
}

fn hash_estimado(d: &[u8]) -> Est {
    DENTRO_DEL_ESTIMADOR.with(|f| f.set(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
        preflate_rs::parametros_estimados(d).map(est_de).unwrap_or_default()));
    DENTRO_DEL_ESTIMADOR.with(|f| f.set(false));
    // Si revienta, el respaldo: hash de zlib, minimo del formato y el nivel 6.
    r.unwrap_or_default()
}

/// `VC_UN_PARSEO=1` INFLA EL FLUJO UNA SOLA VEZ: los tokens salen del parseo que el estimador ya
/// hacia, en vez de volver a inflarlo nosotros.
///
/// 🔑 EL DATO QUE LO DECIDE, y va en la direccion contraria a la que yo suponia. El reparto que
/// se anadio a `preflate_rs` (PF_PARSE_NS / PF_EST_NS) mide las dos mitades por separado:
///
///     inflar (el nuestro)                                       5,0 s
///     estimar: PARSEAR (lo mismo, duplicado)                    2,4 s
///     estimar: estimar de verdad                               12,3 s
///
/// O sea que el parseo duplicado NO son «los 20 s del estimador»: son 2,4. Pero el que sobra es
/// **el nuestro**, que cuesta 2,1 veces mas — nuestro inflate decodifica Huffman BIT A BIT
/// (`inflate.rs`, el metodo de `puff`, escrito para ser demostrable y no rapido) y el suyo va con
/// tabla. Quitar el barato ahorraria 2,4 s; quitar el caro ahorra 5,0.
///
/// ⚠️ QUE SE PIERDE, dicho antes de medir: su token guarda `(long, dist)` y NO el simbolo que se
/// emitio. Para la fase 3 da igual —solo se simula `(long, dist)`— pero el bloque que audita si
/// `sim_long`/`sim_dist` son derivables se queda SIN FUENTE, y por eso se APAGA en este camino en
/// vez de publicar un 100% que solo significa «me lo he inventado yo».
///
/// ⚠️ Y NO SE BORRA NUESTRO INFLATE: sigue siendo el respaldo cuando su parseo revienta (esta
/// medido que revienta), y sigue siendo lo unico que sabe re-emitir el flujo byte a byte.
/// ⚠️ ENCENDIDA POR DEFECTO desde el 03-08. Esta MEDIDA como mejor en un eje y neutra en el otro
/// —reloj de produccion 55,4 -> 50,7 s (−445 EUR/PB-año) con acierto, fallos y coste IDENTICOS— y
/// el binario que sale de `cargo build` tiene que ser el bueno, no el viejo. La perilla se
/// conserva para poder APAGARLA y volver a medir la diferencia (eso es una ablacion), no para
/// tener que acordarse de encenderla antes de creerse un numero.
static UN_PARSEO: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_UN_PARSEO").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

/// Cuantos flujos tuvieron que caer al inflate nuestro porque su parseo revento.
static UN_PARSEO_RESPALDO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Cuantos flujos salieron de su parseo (el camino rapido).
static UN_PARSEO_OK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// SUS bloques traducidos a los nuestros, SIN volver a parsear nada.
///
/// ⚠️ `cabecera: None` NO significa «arboles fijos»: significa «este camino no la lleva». La fase
/// 3 no la mira (solo usa `tokens`), y meter aqui una conversion de `HuffmanOriginalEncoding` que
/// nadie lee seria pagar por nada. Si algun dia alguien la necesita, que falle al compilar y no
/// que lea un `None` como si fuera un dato.
///
/// ⚠️ `sim_long`/`sim_dist` se DERIVAN del canonico. Esta medido (`33a75f7`) que en este banco
/// ninguna copia usa el codigo 284, que es el unico caso en el que el simbolo no se deduce de la
/// longitud. Fuera de este banco esto seria una perdida de informacion real.
fn suyos_a_nuestros(contents: preflate_rs::DeflateContents) -> Vec<Bloque> {
    use preflate_rs::{DeflateToken as DT, DeflateTokenBlockType as DBT};
    contents.blocks.into_iter().map(|b| {
        let ultimo = b.last;
        match b.block_type {
            DBT::Stored { uncompressed } => Bloque::Almacenado { ultimo, datos: uncompressed },
            DBT::Huffman { tokens, .. } => Bloque::Comprimido {
                ultimo, cabecera: None,
                tokens: tokens.into_iter().map(|t| match t {
                    DT::Literal(c) => Token::Lit(c),
                    DT::Reference(r) => {
                        let (long, dist) = (r.len() as u16, r.dist() as u16);
                        let mut il = 0usize;
                        while il + 1 < 29 && LB[il + 1] <= long { il += 1 }
                        let mut id = 0usize;
                        while id + 1 < 30 && DB[id + 1] <= dist { id += 1 }
                        Token::Copia { long, dist,
                                       sim_long: (il + 257) as u16, sim_dist: id as u16 }
                    }
                }).collect(),
            },
        }
    }).collect()
}

/// El flujo inflado UNA vez: plano, bloques y parametros, todo del mismo parseo.
///
/// Devuelve `None` si su parseo revienta o falla; quien llama cae a nuestro inflate y lo cuenta.
fn una_pasada(d: &[u8]) -> Option<(Vec<u8>, Vec<Bloque>, Est)> {
    DENTRO_DEL_ESTIMADOR.with(|f| f.set(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
        preflate_rs::estimado_y_contenido(d).ok()));
    DENTRO_DEL_ESTIMADOR.with(|f| f.set(false));
    let (q, contents, plano) = r.ok().flatten()?;
    Some((plano.text().to_vec(), suyos_a_nuestros(contents), est_de(q)))
}

/// `VC_CFG_ESTIMADOR=1` corre UNA configuracion, la que describe el estimador, en UNA pasada —en
/// vez de nueve presets en cuatro—. Ataca las dos mitades del hueco a la vez: la precision (se
/// acaba el eslabon de cadena de mas) y el reloj, que es donde perdemos por 2,9x.
/// ⚠️ ENCENDIDA POR DEFECTO desde el 02-08: esta MEDIDA como mejor y el binario que sale de
/// `cargo build` tiene que ser el bueno, no el viejo. La perilla se conserva para poder APAGARLA
/// y volver a medir la diferencia -eso es una ablacion-, no para tener que acordarse de encender
/// cuatro variables antes de creerse un numero.
static CFG_ESTIMADOR: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_CFG_ESTIMADOR").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

static E_PEREZOSO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static E_BUENA: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(8);
static E_LAZY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(16);
static E_NICE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(128);
static E_CADENA: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(128);
static E_POL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static E_DIST3: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(DEMASIADO_LEJOS);

/// `VC_DIST3=1` usa la distancia ajustada del estimador en vez de la constante de zlib, Y LA
/// APLICA A LAS NUEVE COMO HACE preflate (`token_predictor.rs:496`), no solo al carril perezoso.
/// Las dos cosas van juntas a proposito: el valor ajustado YA codifica si ese codificador emitia
/// copias cortas lejanas, asi que la puerta perezoso/voraz sobra cuando el numero es suyo.
/// ⚠️ ENCENDIDA POR DEFECTO desde el 02-08: esta MEDIDA como mejor y el binario que sale de
/// `cargo build` tiene que ser el bueno, no el viejo. La perilla se conserva para poder APAGARLA
/// y volver a medir la diferencia -eso es una ablacion-, no para tener que acordarse de encender
/// cuatro variables antes de creerse un numero.
static DIST3_ESTIMADOR: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_DIST3").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

/// `VC_ORDEN_ZLIB=1` inserta la posicion en la cadena ANTES de la mirada perezosa, que es el
/// orden de `deflate_slow`. Sin el, nuestra mirada a pos+1 no ve el candidato de pos.
/// ⚠️ ENCENDIDA POR DEFECTO desde el 02-08: esta MEDIDA como mejor y el binario que sale de
/// `cargo build` tiene que ser el bueno, no el viejo. La perilla se conserva para poder APAGARLA
/// y volver a medir la diferencia -eso es una ablacion-, no para tener que acordarse de encender
/// cuatro variables antes de creerse un numero.
static ORDEN_ZLIB: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_ORDEN_ZLIB").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

/// `VC_CACHE=1` guarda el resultado de la mirada perezosa para la posicion siguiente, que es el
/// `pending_reference` de preflate (`token_predictor.rs:525`). MEJORA LAS DOS COSAS A LA VEZ:
///
///   - EL RELOJ, porque la mirada perezosa era el 31,6% de los pasos de cadena y se tiraba
///     entera: al avanzar un byte, la busqueda principal volvia a recorrer la misma cadena.
///   - Y LA PRECISION, que es lo que no se ve a primera vista. zlib reduce la cadena a la
///     cuarta parte cuando `prev_length >= good_match`, y `prev_length` es la longitud de la
///     posicion ANTERIOR (deflate.c, longest_match). Nuestra busqueda principal llamaba con
///     `previo = cero`, o sea SIN reducir NUNCA; la mirada perezosa si llamaba con la longitud
///     de la posicion anterior. La de la mirada es la de zlib y la principal estaba mal.
///
/// ⚠️ EXIGE VC_ORDEN_ZLIB. Sin el, la cadena que vio la mirada no contiene `pos` y la que vera
/// la busqueda siguiente si: serian dos cadenas distintas y el resultado guardado no valdria.
/// ⚠️ REFUTADA EN SU PRIMERA VERSION Y RESCATADA CON UNA CONDICION. Guardando SIEMPRE: 97,84% ->
/// 97,60% y 1,169% -> 1,432%. Guardando solo cuando `r0 < buena` —el unico caso en que la mirada
/// y la busqueda siguiente son la MISMA llamada— tiene que salir identica, y por eso se enciende.
/// La medida vieja: La hipotesis era que la busqueda principal deberia reducir la cadena por `prev_length`
/// como hace zlib. Pero preflate NO lo hace —`match_token_0` va con `max_chain` entero y solo
/// `match_token_1` reduce (token_predictor.rs:485 y 512)— o sea que nuestra estructura ya era
/// fiel, y guardar el resultado de la mirada mete un valor de cadena REDUCIDA donde va uno sin
/// reducir. Se conserva para poder repetir la medida, no porque sirva.
static CACHE_PEREZOSA: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_CACHE").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

#[inline(always)]
fn dist3() -> usize { E_DIST3.load(std::sync::atomic::Ordering::Relaxed) }

/// La configuracion de la ranura `j`. Con la perilla apagada es `CFGS[j]` y el compilador la
/// resuelve igual que antes; con ella encendida, la del flujo (y solo la ranura 0 esta activa).
/// El mismo interruptor, pero MOVIBLE en caliente. Hace falta para el techo sobre las DIEZ: se
/// corren las nueve, y luego UNA pasada mas con la del estimador, marcando en el mismo mapa.
static USA_ESTIMADOR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[inline(always)]
fn cfg(j: usize) -> Cfg {
    if USA_ESTIMADOR.load(std::sync::atomic::Ordering::Relaxed) {
        use std::sync::atomic::Ordering::Relaxed;
        Cfg { nivel: 0, perezoso: E_PEREZOSO.load(Relaxed), buena: E_BUENA.load(Relaxed),
              lazy: E_LAZY.load(Relaxed), nice: E_NICE.load(Relaxed),
              cadena: E_CADENA.load(Relaxed) }
    } else { CFGS[j] }
}

static TOOFAR_EN_VORAZ: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_TOOFAR").map(|v| v == "1").unwrap_or(false));

/// CUANTAS CONFIGURACIONES SOBREVIVEN AL PREFIJO. Por defecto 1, que es lo que habia.
///
/// 🔑 EL ATAJO ES YA EL HUECO MAS GRANDE DE LA FASE 3, mas que todo lo ganado el 02-08 junto:
///
///     las nueve, enteras       96,09%     3,574%     1.181,0 s
///     elegir en el prefijo     87,33%     1,969%       159,5 s
///
/// Ocho coma siete seis puntos entre SABER cual es la configuracion y ADIVINARLA en el primer
/// quinto del flujo. Y no es falta de informacion: mapear los parametros del estimador
/// (matching_type, max_chain, nice_length) directamente a un nivel acierta el 55,0%, PEOR que el
/// 65,3% del prefijo. Se midio antes de escribir esto y salio que no.
///
/// Lo que si es sospechoso es la forma de la decision: el atajo apaga OCHO de nueve y no vuelve
/// atras nunca. Con K>1 sobreviven las K que menos han fallado y al final gana la mejor de ellas —
/// cuesta K veces el atajo (que sigue siendo mucho menos que las nueve) y acota cuanto de los 8,76
/// puntos es «elegir pronto» y cuanto es «elegir UNA».
static ATAJO_K: std::sync::LazyLock<usize> = std::sync::LazyLock::new(||
    std::env::var("VC_ATAJO_K").ok().and_then(|v| v.parse().ok()).unwrap_or(1).clamp(1, N));
/// `VC_PREFIJO=n` fija el prefijo en bytes. Sin el, un quinto del flujo acotado a [64 KiB, 512 KiB].
static PREFIJO_FIJO: std::sync::LazyLock<Option<usize>> = std::sync::LazyLock::new(||
    std::env::var("VC_PREFIJO").ok().and_then(|v| v.parse().ok()));

/// LA LONGITUD MINIMA QUE EL CODIFICADOR EMITE — que NO es la minima del formato.
///
/// 🔑 ESTO ESTABA EN EL CENSO DESDE EL PRIMER DIA, EN UNA COLUMNA QUE NO MIRABAMOS. El estimador
/// de preflate devuelve `min_len` por flujo, y el reparto del banco nuevo es:
///
///     zlib                  min 3   1.402 flujos      Libdeflate4Fast   min 4    190
///     Libdeflate4           min 3     423             Libdeflate4Fast   min 6    109
///     RandomVector          min 3     184             Crc32cHash        min 6     27
///     MiniZFast             min 3      13             ZlibNG            min 4/6   31
///
/// O sea que ~357 flujos NUNCA emiten una coincidencia corta, y nosotros les proponiamos copias
/// de 3 bytes con un MIN_COPIA clavado. El diagnostico lo canta sin ambiguedad: sobre los 28 de
/// crc32c, la configuracion que menos se equivoca (nivel 4) tiene CERO fallos de distancia, CERO
/// «copia donde iba literal» y 930 de longitud... y 2.531.221 de «literal donde proponemos
/// copia». No fallabamos la coincidencia: proponiamos coincidencias que ese codificador no emite.
///
/// ⚠️ Y LA HIPOTESIS QUE TRAIA ERA OTRA. Habia escrito que faltaba `deflate_medium` de zlib-ng
/// —«recorta coincidencias»— y que era un carril nuevo. Leyendo deflate_medium.c: para los
/// niveles 3 y 4 `early_exit = level < 5` lo deja en VORAZ puro, sin mirada adelante, y el recorte
/// (`fizzle_matches`) solo entra en 5 y 6. Lo que si es suyo es `WANT_MIN_MATCH 4` (zutil.h:65):
/// una coincidencia mas corta se emite como literales. La causa era el umbral, no el carril.
///
/// ⚠️ DE DONDE SALE ESTE NUMERO, Y QUE CUESTA. `min_len` es el minimo sobre TODOS los tokens
/// emitidos del flujo (preflate_stream_info.rs:46), o sea que se deriva DEL MISMO FLUJO que
/// estamos prediciendo. Eso lo hace auto-consistente —si en el flujo no hay ningun token menor de
/// 6, prohibirnos proponer menos de 6 no puede introducir un error— pero NO es gratis: en el codec
/// de verdad este numero hay que ESCRIBIRLO en la cabecera del flujo, como hace preflate con su
/// bloque de parametros. Son unos pocos bits por flujo: 2.522 flujos a 3 bits son 946 bytes contra
/// 187 MB de DEFLATE, o sea 0,0005%. No cambia la economia, pero se dice en vez de darlo por
/// supuesto.
///
/// `VC_MIN_EMIT=n` lo fuerza para todos los flujos; sin el, manda el estimador.
/// EL TOPE DE CADENA CUANDO EL CODIFICADOR BUSCA MENOS QUE NUESTRA CONFIGURACION MAS CORTA.
///
/// La cadena mas corta de la tabla de zlib es 4 (nivel 1). Pero `deflate_quick` de zlib-ng lee UN
/// SOLO candidato —la cabeza de la cadena, deflate_quick.c:98— y el modo rapido de miniz igual: el
/// censo dice cadena 1 en los 13 flujos de MiniZFast. Buscando cuatro candidatos donde ellos miran
/// uno encontramos coincidencias MEJORES que las suyas, y eso sale como fallo por los tres lados a
/// la vez: 250.874 «literal donde proponemos copia», 88.444 de distancia y 193.799 de longitud.
///
/// No se añade una decima configuracion —seria un 11% mas de trabajo por posicion en TODOS los
/// flujos para 13 de 2.522—: se ACOTA la cadena por flujo, y solo cuando el estimador dice que el
/// codificador buscaba menos de lo que nuestra tabla sabe expresar. Con el tope sin poner, no
/// cuesta ni una comparacion distinta de la que ya habia.
static TOPE_CADENA: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
#[inline(always)]
fn tope_cadena() -> usize { TOPE_CADENA.load(std::sync::atomic::Ordering::Relaxed) }

static MIN_EMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(MIN_COPIA);
#[inline(always)]
fn min_emitida() -> usize { MIN_EMIT.load(std::sync::atomic::Ordering::Relaxed) }
static MIN_EMIT_FIJO: std::sync::LazyLock<Option<usize>> = std::sync::LazyLock::new(||
    std::env::var("VC_MIN_EMIT").ok().and_then(|v| v.parse().ok()));
/// `VC_TOPE_CADENA=n` lo fuerza para todos los flujos; sin el, manda el estimador.
static TOPE_CADENA_FIJO: std::sync::LazyLock<Option<usize>> = std::sync::LazyLock::new(||
    std::env::var("VC_TOPE_CADENA").ok().and_then(|v| v.parse().ok()));

const MIN_COPIA: usize = 3;
const MAX_COPIA: usize = 258;
/// TOO_FAR de zlib. Una coincidencia de 3 bytes a mas de esto gasta MAS BITS que los tres
/// literales sueltos, asi que zlib la rechaza. Es una regla del codificador, no del formato, y
/// sin ella el buscador proponia copiar donde el original puso literal en el 9,3% de los
/// simbolos.
const DEMASIADO_LEJOS: usize = 4096;

/// La tabla real de zlib, con sus cuatro columnas. `perezoso` separa deflate_fast (1-3) de
/// deflate_slow (4-9); `lazy` es el umbral por encima del cual deflate_slow ya no mira la
/// posicion siguiente.
#[derive(Clone, Copy)]
struct Cfg { nivel: u8, perezoso: bool, buena: usize, lazy: usize, nice: usize, cadena: usize }
const N: usize = 9;
const CFGS: [Cfg; N] = [
    Cfg { nivel: 1, perezoso: false, buena: 4,  lazy: 4,   nice: 8,   cadena: 4 },
    Cfg { nivel: 2, perezoso: false, buena: 4,  lazy: 5,   nice: 16,  cadena: 8 },
    Cfg { nivel: 3, perezoso: false, buena: 4,  lazy: 6,   nice: 32,  cadena: 32 },
    Cfg { nivel: 4, perezoso: true,  buena: 4,  lazy: 4,   nice: 16,  cadena: 16 },
    Cfg { nivel: 5, perezoso: true,  buena: 8,  lazy: 16,  nice: 32,  cadena: 32 },
    Cfg { nivel: 6, perezoso: true,  buena: 8,  lazy: 16,  nice: 128, cadena: 128 },
    Cfg { nivel: 7, perezoso: true,  buena: 8,  lazy: 32,  nice: 128, cadena: 256 },
    Cfg { nivel: 8, perezoso: true,  buena: 32, lazy: 128, nice: 258, cadena: 1024 },
    Cfg { nivel: 9, perezoso: true,  buena: 32, lazy: 258, nice: 258, cadena: 4096 },
];
/// EL TOPE DEL RECORRIDO — y la tabla de constantes lo tenia marcado «A OJO · SIN MOTIVO» con la
/// nota exacta: *«coincide con el max_chain del nivel 9 de zlib, asi que en teoria no recorta
/// nada. Pero nunca se ha comprobado que ningun flujo lo toque»*.
///
/// 🔑 SI LO TOCAN. preflate se niega a un flujo cuando la cadena que observa pasa de 4.096
/// (complevel_estimator.rs:86), y son 77 de los 91 que rechaza en este banco: existen flujos con
/// la cadena mas larga que nuestro tope. Cuando el estimador revienta caemos a `usize::MAX`, que
/// aqui se acota justo a esto — o sea que en esos flujos SI recortamos.
///
/// `VC_CADENA_MAX=n` lo mueve para medir que cuesta el recorte y que costaria quitarlo. Subirlo
/// lo pagan TODOS los flujos, asi que la pregunta no es «¿mejora?» sino «¿mejora mas de lo que
/// cuesta?», y eso se contesta en €/PB-año.
static CADENA_MAX: std::sync::LazyLock<usize> = std::sync::LazyLock::new(||
    std::env::var("VC_CADENA_MAX").ok().and_then(|v| v.parse().ok()).unwrap_or(4096));

/// ¿SE APELOTONAN LOS FALLOS AL FINAL DEL BLOQUE?
///
/// 🔑 `deflate_slow` exige MIN_LOOKAHEAD = MAX_MATCH + MIN_MATCH + 1 = 262 bytes de margen para
/// llamar a longest_match. Cerca del final de un bloque —o antes de un flush— el codificador NO
/// PUEDE buscar: emite literales aunque haya copia. Nosotros si buscamos, encontramos, y sale
/// como «dijimos copia y era literal», que es el 68% de nuestros fallos.
///
/// Si la hipotesis es buena, los fallos de ese canal estaran concentrados en los ultimos tokens
/// de cada bloque. Si estan repartidos, la hipotesis es mala y hay que buscar en otro sitio.
/// Se cuenta por TOKENS que quedan hasta el fin del bloque, no por bytes: es lo barato, y para
/// distinguir «apelotonado» de «repartido» sobra.
static FIN_FALLO: [std::sync::atomic::AtomicU64; 12] = [
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
];
static FIN_TODOS: [std::sync::atomic::AtomicU64; 12] = [
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0),
];
/// El cubo: 0..=9 son «quedan 0..9 tokens», 10 es «quedan 10..99», 11 es «100 o mas».
#[inline(always)]
fn cubo_fin(quedan: usize) -> usize {
    if quedan < 10 { quedan } else if quedan < 100 { 10 } else { 11 }
}

/// Cuantas veces el recorrido para PORQUE LLEGA AL TOPE, en vez de agotar la cadena o el limite
/// de la configuracion. Es la comprobacion que faltaba.
static TOPADOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// ⚠️ LOS CUATRO NO-zlib SE LLAMAN A LA IMPLEMENTACION DE preflate, no a una copia. Un hash es
/// una IDENTIDAD: si difiere en un bit, la cadena sale en otro orden y el simulador deja de
/// simular. Copiar la tabla CRC32C de 256 entradas habria sido copiar 256 sitios donde
/// desincronizarse en silencio.
#[inline(always)]
fn h3(d: &[u8], p: usize) -> usize {
    use preflate_rs::HashImplementation;
    match hash_actual() {
        H_MINIZ => {
            // 3 bytes y 4.096 cubos: la tabla mas pequeña de las cinco, por eso su cadena es
            // otra cosa. Es el modo mas rapido de miniz.
            if p + 3 > d.len() { return 0 }
            return preflate_rs::MiniZHash {}.get_hash(&d[p..p + 3]) as usize & (*CUBOS - 1);
        }
        H_ZLIBNG => {
            if p + 4 > d.len() { return 0 }
            return preflate_rs::ZlibNGHash {}.get_hash(&d[p..p + 4]) as usize & (*CUBOS - 1);
        }
        H_CRC32C => {
            // El de zlib-ng moderno. Veintiocho flujos del banco y el 32,1% del hueco.
            if p + 4 > d.len() { return 0 }
            return preflate_rs::Crc32cHash {}.get_hash(&d[p..p + 4]) as usize & (*CUBOS - 1);
        }
        _ => {}
    }
    if hash_actual() == H_LIBDEFLATE {
        // EL HASH DE libdeflate, leido de crates/preflate/src/hash_algorithm.rs:202 (esta en
        // NUESTRO repo desde el 31-07). CUATRO bytes y multiplicativo, contra los TRES rotativos
        // de zlib. No es una variante del mismo buscador: cambia cuantos bytes hacen falta para
        // entrar en la cadena, cuantos cubos hay y en QUE ORDEN salen los candidatos.
        //
        // ⚠️ Su salida son 16 bits (65.536 cubos) y nuestra tabla tiene 15 (32.768). Se recorta
        // con la mascara, o sea que dos cubos suyos caen en uno nuestro. Eso EMPEORA su
        // fidelidad, asi que si aun asi el acierto sube, sube DE VERDAD — la medida va en la
        // direccion segura. Igualar la tabla es lo siguiente si esto paga.
        if p + 4 > d.len() { return 0 }
        let v = u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]);
        ((v.wrapping_mul(0x1E35A7BD) >> 16) as usize) & (*CUBOS - 1)
    } else {
        (((d[p] as usize) << 10) ^ (((d[p + 1] as usize) << 5) ^ (d[p + 2] as usize))) & MASCARA_ZLIB
    }
}

#[inline(always)]
fn largo_comun(d: &[u8], a: usize, b: usize, tope: usize) -> usize {
    let mut k = 0;
    while k < tope && d[a + k] == d[b + k] { k += 1 }
    k
}

/// UN SOLO RECORRIDO PARA LAS NUEVE. Devuelve en `res[j]` lo que la configuracion `j` habria
/// encontrado en `pos`, dado que ya llevaba una coincidencia de `previo[j]` bytes (que es lo que
/// activa la reduccion de cadena de zlib).
///
/// `res[j] = (0,0)` significa «aqui esta configuracion no copia»: literal.
#[inline]
fn mejor_todos(d: &[u8], pos: usize, head: &[u32], prev: &[u32],
               previo: &[usize; N], activo: &[bool; N], res: &mut [(u16, u16); N]) {
    *res = [(0, 0); N];
    let n = d.len();
    if pos + MIN_COPIA > n { return }
    let tope = MAX_COPIA.min(n - pos);
    if tope < MIN_COPIA { return }

    // 🔑 EL CARRIL DE UNA SOLA CONFIGURACION — CAMINO P9, «¿pagamos el caso general para servir
    // al caso raro?».
    //
    // Con la configuracion del estimador solo hay UNA activa, pero el bucle general seguia
    // recorriendo las NUEVE ranuras dos veces por cada candidato de la cadena: una para ver si
    // alguna agota su tope y otra para ver si alguna llega a su `nice`. Ocho de esas nueve se
    // cierran en el paso 0 y aun asi cuestan su iteracion, en cada posicion de cada flujo.
    //
    // Este carril hace exactamente lo mismo —las mismas reglas, el mismo orden, los mismos
    // cortes— sin el andamiaje. No es una aproximacion: si diera un resultado distinto seria un
    // fallo, y por eso se mide con el general al lado.
    if *CFG_ESTIMADOR {
        let c0 = cfg(0);
        let mut lim = c0.cadena.min(tope_cadena());
        if previo[0] >= c0.buena { lim >>= 2 }
        let topa = lim.max(1) > *CADENA_MAX;
        let lim = lim.max(1).min(*CADENA_MAX);
        let (mut ml, mut md) = (0usize, 0usize);
        let mut cur = head[h3(d, pos)];
        let mut k = 0usize;
        let mut descartes = 0u64;
        while k < lim && cur != u32::MAX {
            let cp = cur as usize;
            let dd = pos - cp;
            let tope_dd = if !*ALCANCE_ZLIB { VENTANA }
                          else if k == 0 { MAX_DIST } else { MAX_DIST - 1 };
            if dd == 0 || dd > tope_dd { break }
            // Nada puede superar una coincidencia que ya llega al tope del flujo.
            if ml >= tope { break }
            if no_puede_superar(d, cp, pos, ml, n) {
                descartes += 1;
                cur = prev[cp & MASCARA];
                k += 1;
                continue
            }
            let l = largo_comun(d, cp, pos, tope);
            if l > ml {
                ml = l; md = dd;
                // La mejor longitud SOLO CRECE: en cuanto pasa el `nice`, esta configuracion
                // habria parado AQUI.
                if ml >= c0.nice || ml >= MAX_COPIA { break }
            }
            cur = prev[cp & MASCARA];
            k += 1;
        }
        // Solo cuenta como «topado» si el recorrido llego de verdad al tope Y todavia habia
        // cadena por delante: si se acabaron los candidatos, el tope no ha recortado nada.
        if topa && k >= *CADENA_MAX && cur != u32::MAX {
            TOPADOS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        // La mirada perezosa entra con `previo[0] > 0`, que es como la llama el bucle de tokens.
        suma_pasos(if previo[0] > 0 { 1 } else { 0 }, k as u64);
        if descartes > 0 { DESCARTES.fetch_add(descartes, std::sync::atomic::Ordering::Relaxed); }
        let descarta_3 = if *DIST3_ESTIMADOR {
            ml == MIN_COPIA && md > dist3()
        } else {
            (c0.perezoso || *TOOFAR_EN_VORAZ) && ml == MIN_COPIA && md > DEMASIADO_LEJOS
        };
        res[0] = if ml < min_emitida() || descarta_3 { (0, 0) } else { (ml as u16, md as u16) };
        return
    }

    // El tope efectivo de cada configuracion, ya con la reduccion de zlib aplicada.
    let mut lim = [0usize; N];
    let mut lim_max = 0usize;
    for j in 0..N {
        // Una configuracion apagada se cierra en el paso 0 y no alarga el recorrido. Asi el
        // tope efectivo es el de las ACTIVAS, que es de donde sale el ahorro de correr solo la
        // ganadora en el resto del flujo.
        if !activo[j] { lim[j] = 0; continue }
        let cj = cfg(j);
        let mut c = cj.cadena.min(tope_cadena());
        if previo[j] >= cj.buena { c >>= 2 }
        lim[j] = c.max(1);
        if lim[j] > lim_max { lim_max = lim[j] }
    }
    let lim_max = lim_max.min(*CADENA_MAX);

    // Resuelto[j] = ya sabemos que devuelve j (porque llego a su `nice` o agoto su cadena).
    let mut resuelto = [false; N];
    let mut pendientes = N;
    let (mut ml, mut md) = (0usize, 0usize);
    let mut cur = head[h3(d, pos)];
    let mut k = 0usize;

    while pendientes > 0 {
        // Primero: ¿alguna configuracion agota su cadena justo aqui? Se cierra con lo que haya.
        for j in 0..N {
            if !resuelto[j] && k >= lim[j] {
                resuelto[j] = true; pendientes -= 1;
                res[j] = (ml as u16, md as u16);
            }
        }
        if pendientes == 0 || cur == u32::MAX || k >= lim_max { break }
        let cp = cur as usize;
        let dd = pos - cp;
        let tope_dd = if !*ALCANCE_ZLIB { VENTANA }
                      else if k == 0 { MAX_DIST } else { MAX_DIST - 1 };
        if dd == 0 || dd > tope_dd { break }
        if no_puede_superar(d, cp, pos, ml, n) {
            DESCARTES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            cur = prev[cp & MASCARA];
            k += 1;
            continue
        }
        let l = largo_comun(d, cp, pos, tope);
        if l > ml {
            ml = l; md = dd;
            // La mejor longitud SOLO CRECE, asi que en cuanto pasa el `nice` de una
            // configuracion, esa configuracion habria parado AQUI. Se cierra con este valor.
            for j in 0..N {
                if !resuelto[j] && ml >= cfg(j).nice {
                    resuelto[j] = true; pendientes -= 1;
                    res[j] = (ml as u16, md as u16);
                }
            }
            if ml >= MAX_COPIA { break }
        }
        cur = prev[cp & MASCARA];
        k += 1;
    }
    // Lo que quede sin resolver se cierra con el mejor encontrado.
    for j in 0..N {
        if !resuelto[j] { res[j] = (ml as u16, md as u16) }
    }
    // Y las reglas de descarte, que se aplican DESPUES de elegir.
    //
    // ⚠️ TOO_FAR ES SOLO DEL CARRIL PEREZOSO, y aqui se aplicaba a los nueve niveles.
    //
    // Leido en la fuente de zlib (deflate.c, la que vendoriza paq8px): el descarte vive en la
    // linea 1965, DENTRO de deflate_slow —que empieza en la 1923—. deflate_fast ocupa de la 1824
    // a la 1922 y no lo tiene. O sea que en los niveles 1, 2 y 3 zlib SI emite una copia de 3
    // bytes a mas de 4096 de distancia, y nosotros predeciamos literal.
    //
    // La condicion real, con la estrategia por defecto, es:
    //     match_length <= 5 && (strategy == Z_FILTERED || (match_length == 3 && dist > TOO_FAR))
    // que sin Z_FILTERED se queda en «longitud exactamente 3 y distancia > 4096». Eso es lo que
    // teniamos; lo que faltaba era el «solo si es perezoso».
    //
    // POR QUE IMPORTA TANTO: el carril voraz acierta el 63,11% contra el 90,91% del perezoso, y
    // se lleva el 32,59% de TODOS los fallos con solo el 10,86% de los simbolos. Y no es que esos
    // flujos sean de otro codificador —siendo zlib de verdad aciertan 63,33%, no siendolo 61,63%,
    // practicamente lo mismo—: es que no sabiamos simular su deflate_fast.
    //
    // `VC_TOOFAR=1` vuelve a aplicarlo a los nueve, que es lo que habia, para poder medir la
    // diferencia sin recompilar.
    for j in 0..N {
        let (l, dd) = (res[j].0 as usize, res[j].1 as usize);
        // ⚠️ TOO_FAR sigue mirando MIN_COPIA (3) y no el minimo del codificador, y es a proposito:
        // es la regla de zlib sobre coincidencias DE TRES BYTES —«a mas de 4 KiB gasta mas bits que
        // tres literales»—, no sobre «la mas corta que este emita». Si el minimo es 6, la regla
        // simplemente no se aplica nunca, que es lo correcto.
        let descarta_3 = if *DIST3_ESTIMADOR {
            l == MIN_COPIA && dd > dist3()
        } else {
            (cfg(j).perezoso || *TOOFAR_EN_VORAZ) && l == MIN_COPIA && dd > DEMASIADO_LEJOS
        };
        if l < min_emitida() || descarta_3 { res[j] = (0, 0) }
    }
}

/// LOS SALTOS DE preflate — `calculate_hops`, hash_chain_holder.rs:205.
///
/// 🔑 CUANDO FALLAN LA DISTANCIA, ELLOS NO ESCRIBEN LA DISTANCIA NI SU DIFERENCIA. Escriben
/// **cuantos candidatos de la cadena que empatan o superan la longitud verdadera** hay que saltar
/// hasta llegar al bueno. Casi siempre 1 o 2, porque la mayoria de eslabones ni llega a esa
/// longitud. Nosotros pagabamos el zig-zag de una resta de distancias, que puede ser de cinco
/// cifras — y la cadena YA ESTA RECORRIDA, asi que el salto no cuesta calcularlo.
///
/// La regla se copia literal de la suya, incluido el orden: se cuenta el candidato ANTES de mirar
/// si ya hemos llegado o pasado la distancia buscada.
///
///     if match_length >= 3 && match_length >= best_len { hops += 1 }
///     if dist >= target.dist() { if dist == target.dist() { return hops } else { break } }
///
/// ⚠️ Devuelve `None` si el candidato no aparece —cadena agotada, fuera de ventana, o la posicion
/// no se indexo por la politica de diccionario—. Esos NO se maquillan: se cuentan aparte y se
/// cobran a la vieja usanza, que es lo caro. preflate en ese caso aborta con PredictionFailure.
/// LOS PASOS DE CADENA, POR SITIO. Un paso es una comparacion `largo_comun`, que es la unidad de
/// trabajo real de esta fase. Se acumulan en local y se suman UNA vez por llamada para que contar
/// no falsee lo que se cuenta.
///   0 = la busqueda principal · 1 = la mirada perezosa a pos+1 · 2 = los saltos de preflate
static PASOS: [std::sync::atomic::AtomicU64; 3] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];
#[inline(always)]
fn suma_pasos(i: usize, n: u64) {
    if n > 0 { PASOS[i].fetch_add(n, std::sync::atomic::Ordering::Relaxed); }
}

/// EL DESCARTE RAPIDO DE zlib — `deflate.c:1449`, y lo teniamos delante todo el dia.
///
/// 🔑 zlib NO compara el candidato entero. Antes mira CUATRO BYTES y, si alguno no cuadra, pasa
/// al siguiente sin tocar nada mas:
///
///     if (match[best_len]     != scan_end  ||
///         match[best_len - 1] != scan_end1 ||
///         *match              != *scan     ||
///         *++match            != scan[1])      continue;
///
/// Es un descarte PURO, no una heuristica: si el byte que esta en la posicion `best_len` no
/// coincide, la coincidencia no llega a `best_len` y por tanto NO PUEDE superar a la mejor que ya
/// tenemos. El resultado sale identico byte a byte; lo unico que cambia es cuanto se trabaja.
///
/// Nosotros haciamos `largo_comun` en los 7.710 millones de pasos de cadena del banco —61,7
/// comparaciones completas por simbolo—. Este es el mismo codigo que llevamos leyendo desde el
/// primer dia para copiar SUS REGLAS, y la optimizacion estaba tres lineas mas abajo.
#[inline(always)]
fn no_puede_superar(d: &[u8], cp: usize, pos: usize, ml: usize, n: usize) -> bool {
    // Con ml = 0 no hay nada que superar y el descarte no aplica.
    if ml < MIN_COPIA || pos + ml >= n { return false }
    d[cp + ml] != d[pos + ml]
        || d[cp + ml - 1] != d[pos + ml - 1]
        || d[cp] != d[pos]
        || d[cp + 1] != d[pos + 1]
}

/// EL `repredict_reference` DE preflate — token_predictor.rs:544.
///
/// 🔑 Cuando ellos predicen literal y la verdad era una copia, NO dan el token por perdido:
/// vuelven a buscar con la cadena entera y corrigen contra ESA busqueda. Y si tampoco encuentran,
/// lo intentan otra vez con cadena 4096:
///
///     let match_token = self.state.match_token_0(0, self.params.max_chain, input);
///     if let MatchResult::Success(m) = match_token { if m.len() >= MIN_MATCH { return Ok(m) } }
///     let match_token = self.state.match_token_0(0, 4096, input);   // otra vez, mas hondo
///
/// Nosotros corregiamos contra la coincidencia cruda que las reglas del nivel habian descartado
/// —que a veces es (0,0)— y entonces el zig-zag de la longitud se dispara: corregir «de 0 a 137»
/// cuesta ocho bits de magnitud donde corregir «de 134 a 137» cuesta dos.
///
/// Son 139.072 posiciones del banco, el 19% de las correcciones de longitud, y justo las caras.
#[inline]
fn mejor_uno(d: &[u8], pos: usize, head: &[u32], prev: &[u32], lim: usize) -> (usize, usize) {
    let n = d.len();
    if pos + MIN_COPIA > n { return (0, 0) }
    let tope = MAX_COPIA.min(n - pos);
    let (mut ml, mut md) = (0usize, 0usize);
    let mut cur = head[h3(d, pos)];
    let mut k = 0usize;
    while k < lim && cur != u32::MAX {
        let cp = cur as usize;
        let dd = pos - cp;
        if dd == 0 { cur = prev[cp & MASCARA]; continue }
        if dd > VENTANA { break }
        if ml >= tope { break }
        if !no_puede_superar(d, cp, pos, ml, n) {
            let l = largo_comun(d, cp, pos, tope);
            if l > ml { ml = l; md = dd; if ml >= MAX_COPIA { break } }
        }
        cur = prev[cp & MASCARA];
        k += 1;
    }
    (ml, md)
}

/// EL VOLCADO DE LAS CORRECCIONES — CAMINO P16, «¿se puede medir esto SIN construir antes lo que
/// la medida justificaria?».
///
/// 🔑 El techo medido dice que ajustando configuraciones NO se baja de ~1,13%: el 1,55% de los
/// tokens no los explica ninguna. Lo unico que atraviesa un techo asi es cambiar el CODIFICADOR
/// de las correcciones —bajar el coste sin tocar el acierto—, y tenemos uno propio mucho mas
/// fuerte que su CABAC: vc-cm.
///
/// Integrarlo es trabajo. Pero para saber SI COMPENSA no hace falta integrarlo: basta con
/// escribir el flujo de correcciones tal cual y comprimirlo por fuera. `VC_VOLCADO=<ruta>` lo
/// escribe con la MISMA informacion que cobra nuestro modelo —por canal, pares (racha, valor) en
/// varint— para que la comparacion sea entre CODIFICADORES y no entre formatos.
///
/// ⚠️ Solo tiene sentido con UNA configuracion (el defecto): con nueve, las correcciones
/// volcadas serian las de todas y no las de la que se elige.
static VOLCADO: std::sync::LazyLock<Option<String>> = std::sync::LazyLock::new(||
    std::env::var("VC_VOLCADO").ok().filter(|v| !v.is_empty()));

thread_local! {
    static CANALES: std::cell::RefCell<[Vec<u8>; NCAT]> =
        std::cell::RefCell::new(Default::default());
}

/// Varint de toda la vida: siete bits por byte, el alto marca continuacion.
fn varint(v: &mut Vec<u8>, mut x: u64) {
    while x >= 0x80 { v.push((x as u8) | 0x80); x >>= 7 }
    v.push(x as u8);
}

/// DEFECTO 200.000, MEDIDO: con ese corte el acierto sale IDENTICO (97,84%), el coste sube
/// 0,001 puntos y la produccion baja de 59,1 a 53,3 s — 551 EUR/PB-anyo por no barrer lo que ya
/// no cambia nada. Por debajo si duele: 50.000 cuesta 0,027 puntos y 5.000 cuesta 1,669.
///
/// `VC_EST_LIMITE=n` corta el barrido de profundidad del estimador tras n tokens. Es el 30% de
/// nuestro reloj y subestimar la cadena va en la direccion segura; lo que hay que vigilar es que
/// no se equivoque de HASH por cortar antes de tiempo.
static EST_LIMITE_N: std::sync::LazyLock<usize> = std::sync::LazyLock::new(||
    std::env::var("VC_EST_LIMITE").ok().and_then(|v| v.parse().ok()).unwrap_or(200_000));

/// `VC_TECHO_DIEZ=1` añade una pasada con la configuracion del estimador al medir el techo con
/// las nueve, para tener el suelo sobre las DIEZ y no una cota.
static TECHO_DIEZ: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_TECHO_DIEZ").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) != 0);

/// `VC_REPREDICT=0` lo apaga para poder medir lo que aporta.
static REPREDICT: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_REPREDICT").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

/// ¿HACEN FALTA LOS DOS SIMBOLOS GUARDADOS, O SON DERIVABLES?
///
/// 🔑 Su `DeflateTokenReference` ocupa TRES bytes —`len: u8, dist: u16`— y resuelve el unico caso
/// ambiguo (la longitud 258, que admite el codigo 284 con extras a uno o el 285) con un campo
/// `irregular258`. El nuestro ocupa OCHO: guarda los dos simbolos Huffman enteros como `u16`.
///
/// En DEFLATE la distancia determina su simbolo sin ambiguedad —`DIST_BASE` particiona 1..32768—
/// y la longitud tambien, SALVO el 258. O sea que `sim_dist` sobraria entero y `sim_long` se
/// reduciria a un bit: de ocho bytes por token a cuatro. Con 124,9 millones de tokens es medio
/// giga menos de estructuras que el bucle recorre entero.
///
/// ⚠️ PERO ESO ES ARGUMENTAR DESDE LA ESPECIFICACION, y hoy hemos visto cinco veces lo que pasa
/// con eso. Aqui se CUENTA sobre el banco: cuantos tokens traen un simbolo que no es el que se
/// derivaria. Si sale cero (salvo el 258), se pueden tirar los dos campos.
const LB: [u16; 29] = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
const DB: [u16; 30] = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
static SIM_TOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SIM_LONG_MAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SIM_DIST_MAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SIM_258: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SIM_284: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Cuantos candidatos se han descartado con cuatro bytes en vez de con una comparacion entera.
static DESCARTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[inline]
fn saltos(d: &[u8], pos: usize, head: &[u32], prev: &[u32], long: usize, dist: usize) -> Option<u32> {
    let n = d.len();
    if pos + MIN_COPIA > n { return None }
    let tope = MAX_COPIA.min(n - pos);
    let mut cur = head[h3(d, pos)];
    let mut hops = 0u32;
    let mut k = 0usize;
    while cur != u32::MAX && k < *CADENA_MAX {
        let cp = cur as usize;
        let dd = pos - cp;
        // ⚠️ CON EL ORDEN DE zlib LA CADENA YA CONTIENE `pos`, y arrancar en el mismo sitio daba
        // dd == 0 y cortaba el recorrido: TODAS las correcciones de distancia salian «no
        // encontrada» y se cobraban al zig-zag caro. Se vio porque el acierto MEJORO y el coste
        // EMPEORO a la vez, que no puede ser. preflate llama a calculate_hops antes de
        // commit_token, o sea con la cadena sin `pos`; aqui se salta ese candidato y ya.
        if dd == 0 { cur = prev[cp & MASCARA]; continue }
        if dd > VENTANA { break }
        let l = largo_comun(d, cp, pos, tope);
        if l >= MIN_COPIA && l >= long { hops += 1 }
        if dd >= dist {
            suma_pasos(2, k as u64 + 1);
            return if dd == dist { Some(hops) } else { None }
        }
        cur = prev[cp & MASCARA];
        k += 1;
    }
    suma_pasos(2, k as u64);
    None
}

/// LAS CINCO CATEGORIAS DE CORRECCION DE preflate, las que de verdad emite por token.
///
/// No es un alfabeto de cinco desenlaces mezclados como el nuestro: son cinco CANALES
/// independientes, cada uno con su propio contexto adaptativo, y cada token escribe en dos o
/// cuatro de ellos. La diferencia no es cosmetica —un canal donde el 99,9% es cero cuesta
/// practicamente nada, y mezclado con otro donde el 85% es cero, los dos cuestan lo del peor—.
const NCAT: usize = 5;
const CAT_LIT: usize = 0;   // LiteralPredictionWrong  — solo si predijimos literal
const CAT_REF: usize = 1;   // ReferencePredictionWrong — solo si predijimos copia
const CAT_LONG: usize = 2;  // LenCorrection            — por cada token cuya verdad es copia
const CAT_DIST: usize = 3;  // DistOnlyCorrection       — idem, en SALTOS
const CAT_DAL: usize = 4;   // DistAfterLenCorrection   — idem, cuando ademas fallo la longitud
const CAT_NOMBRE: [&str; NCAT] = ["lit?", "copia?", "longitud", "distancia", "dist-tras-long"];

/// `VC_SALTOS=1` enciende la contabilidad con la estructura de preflate. Apagada por defecto: el
/// recorrido extra de cadena cuesta reloj y falsearia la medida de tiempo de siempre.
/// ⚠️ ENCENDIDA POR DEFECTO desde el 02-08: esta MEDIDA como mejor y el binario que sale de
/// `cargo build` tiene que ser el bueno, no el viejo. La perilla se conserva para poder APAGARLA
/// y volver a medir la diferencia -eso es una ablacion-, no para tener que acordarse de encender
/// cuatro variables antes de creerse un numero.
static SALTOS_ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_SALTOS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0);

/// LA POLITICA DE DICCIONARIO DE CADA CONFIGURACION, Y NO HAY NADA QUE ESTIMAR.
///
/// `deflate.h:189` lo dice con todas las letras: **«max_insert_length is used only for
/// compression levels <= 3»**. Y en `deflate_fast` (deflate.c:1872) la rama es incondicional:
///
///     if (s->match_length <= s->max_insert_length && s->lookahead >= MIN_MATCH) {
///         ... mete las match_length posiciones ...
///     } else {
///         s->strstart += s->match_length;   // no mete NINGUNA de dentro
///     }
///
/// con `max_insert_length` == `max_lazy_match` == la columna `max_lazy` de la tabla, o sea 4, 5
/// y 6 para los niveles 1, 2 y 3. `deflate_slow`, en cambio, las mete todas (deflate.c:1996).
///
/// 🔑 O SEA QUE ESTO NO ERA UNA PALANCA QUE HUBIERA QUE APRENDER A ACTIVAR: ERA UN FALLO DE
/// SIMULACION. Nosotros haciamos `AddAll` para las nueve configuraciones, asi que en los tres
/// niveles voraces nuestra cadena tenia candidatos que la del codificador original NO TENIA —y
/// predeciamos coincidencias que el no podia haber encontrado—. Medido a mano sobre los flujos
/// afectados: +27,50 puntos (bf8e647). Es la causa gorda de que el carril voraz vaya al 65% y el
/// perezoso al 91%.
///
/// ⚠️ Y POR ESO HAY QUE CORRER CUATRO PASADAS Y NO UNA: la cadena es el estado, y dos politicas
/// distintas son dos cadenas distintas. No se pueden compartir. La buena noticia es que las tres
/// pasadas voraces son casi gratis —sus `max_chain` son 4, 8 y 32 contra los 128-4096 del
/// perezoso—, asi que lo que se paga es una fraccion pequena de la pasada larga.
const POL_DE_NIVEL: [usize; N] = {
    let mut p = [0usize; N];
    let mut j = 0;
    while j < N {
        p[j] = if CFGS[j].perezoso { 0 } else { CFGS[j].lazy };
        j += 1;
    }
    p
};

/// Las cuatro pasadas: (politica, que configuraciones la usan).
/// Se calcula de POL_DE_NIVEL para que anadir un nivel no obligue a acordarse de tocar esto.
fn pasadas() -> Vec<(usize, [bool; N])> {
    // Con la configuracion del estimador hay UNA pasada y UNA configuracion: la politica ya no
    // se deduce del nivel, la dice el estimador. Aqui esta la mitad del reloj —cuatro pasadas
    // pasan a una— y por eso este modo no es solo una cuestion de precision.
    if USA_ESTIMADOR.load(std::sync::atomic::Ordering::Relaxed) {
        let mut a = [false; N]; a[0] = true;
        return vec![(E_POL.load(std::sync::atomic::Ordering::Relaxed), a)];
    }
    let mut v: Vec<(usize, [bool; N])> = Vec::new();
    for j in 0..N {
        let pol = if let Some(o) = perilla_add() { o } else { POL_DE_NIVEL[j] };
        if let Some(e) = v.iter_mut().find(|e| e.0 == pol) { e.1[j] = true; }
        else { let mut a = [false; N]; a[j] = true; v.push((pol, a)); }
    }
    v
}

#[derive(Default, Clone, Copy)]
struct Cuenta {
    simbolos: u64,
    acierto: u64,
    lit_vs_copia: u64,
    copia_vs_lit: u64,
    otra_dist: u64,
    otra_long: u64,
    delta: [u64; 10],
    /// LONGITUD EN BITS del zig-zag de cada correccion, que es lo que de verdad se codifica.
    /// Antes no existia: la distancia se cobraba a 4 bits fijos sin haber mirado NUNCA por
    /// cuanto se falla, y la longitud a 8. Un numero inventado no deja de serlo por estar en
    /// una tabla.
    bl_long: [u64; 32],
    bl_dist: [u64; 32],
    /// LAS RACHAS — `encode_correction`, cabac_codec.rs:133.
    ///
    /// 🔑 UN ACIERTO NO EMITE SIMBOLO. Incrementa el contador de la ultima racha de su categoria;
    /// diez mil aciertos seguidos son UNA longitud de racha en exp-Golomb. Nuestro modelo cobraba
    /// `simbolos x H(p)`, que da por hecho que los fallos son INDEPENDIENTES — y no lo son: se
    /// apelotonan donde los parametros divergen. Con fallos a rachas, el por-simbolo cobra de mas.
    bl_racha: [[u64; 32]; NCAT],
    bl_valor: [[u64; 32]; NCAT],
    /// Los que la cadena no encuentra. Se cobran a la vieja usanza y se dicen en voz alta.
    salto_perdido: u64,
}

/// El zig-zag de preflate: el signo en el bit bajo, para que una diferencia de -1 cueste lo
/// mismo que una de +1 y no lo que cuesta un numero enorme. Es `encode_difference` de
/// cabac_codec.rs, y se copia el criterio para poder comparar magnitudes iguales.
fn zigzag(pred: usize, real: usize) -> u64 {
    if pred >= real { ((pred - real) as u64) << 1 } else { (((real - pred) as u64) << 1) | 1 }
}
fn bitlen(v: u64) -> usize { ((64 - v.leading_zeros()) as usize).min(31) }

impl Cuenta {
    fn fallos(&self) -> u64 {
        self.lit_vs_copia + self.copia_vs_lit + self.otra_dist + self.otra_long
    }
    fn suma(&mut self, o: &Cuenta) {
        self.simbolos += o.simbolos; self.acierto += o.acierto;
        self.lit_vs_copia += o.lit_vs_copia; self.copia_vs_lit += o.copia_vs_lit;
        self.otra_dist += o.otra_dist; self.otra_long += o.otra_long;
        for i in 0..10 { self.delta[i] += o.delta[i]; }
        for i in 0..32 { self.bl_long[i] += o.bl_long[i]; self.bl_dist[i] += o.bl_dist[i]; }
        for c in 0..NCAT { for i in 0..32 {
            self.bl_racha[c][i] += o.bl_racha[c][i]; self.bl_valor[c][i] += o.bl_valor[c][i];
        } }
        self.salto_perdido += o.salto_perdido;
    }
}

/// UNA pasada por el flujo que puntua las NUEVE configuraciones a la vez.
/// `prefijo`: hasta ese byte del plano se puntuan las NUEVE; a partir de ahi, solo la que iba
/// ganando. Con `prefijo = usize::MAX` se puntuan las nueve enteras, que es la referencia contra
/// la que se mide lo que cuesta el atajo.
///
/// Devuelve las nueve cuentas y cual se eligio en el prefijo (usize::MAX si no hubo atajo).
fn evalua(plano: &[u8], bloques: &[Bloque], head: &mut [u32], prev: &mut [u32],
          prefijo: usize, activo_ini: [bool; N], pol: usize) -> ([Cuenta; N], usize) {
    head.iter_mut().for_each(|x| *x = u32::MAX);
    prev.iter_mut().for_each(|x| *x = u32::MAX);
    let n = plano.len();
    let mut pos = 0usize;
    let mut q = [Cuenta::default(); N];
    let mut r0 = [(0u16, 0u16); N];
    let mut r1 = [(0u16, 0u16); N];
    let cero = [0usize; N];
    let mut activo = activo_ini;
    let mut elegida = usize::MAX;
    let mut sobreviven = [false; N];
    // La racha ABIERTA de cada canal. Solo se escribe cuando llega una correccion que no es cero
    // —y al final del flujo—, que es exactamente lo que hace `encode_correction`.
    let mut racha = [[0u64; NCAT]; N];
    // 🔑 EL TECHO DE ESTE ENFOQUE. Un fallo puede venir de haber elegido mal la configuracion
    // —y eso se arregla ajustando— o de que NINGUNA de las que sabemos expresar produce ese
    // token. Lo segundo no lo arregla ningun ajuste: es el suelo del modelo. Sin este numero,
    // «¿podemos bajar de X?» no tiene respuesta: no se sabe si queda camino o hay una pared.
    // Solo la corrida de REFERENCIA alimenta el techo: la del atajo corre un subconjunto.
    let cuenta_techo = prefijo == usize::MAX;
    let mut i_token = 0usize;
    // El resultado de la mirada perezosa, guardado para la posicion siguiente. `None` = hay que
    // buscar de verdad.
    let mut guardado: Option<[(u16, u16); N]> = None;

    /// Un valor en un canal: si es cero, alarga la racha y NO escribe nada. Si no, cierra la
    /// racha (una longitud) y escribe el valor. Los dos van a `value - 1` como en
    /// `write_exp_encoded`, para que un 1 no cueste mas que un 0.
    macro_rules! emite {
        ($j:expr, $cat:expr, $v:expr) => {{
            let v: u64 = $v;
            if v == 0 { racha[$j][$cat] += 1 }
            else {
                if VOLCADO.is_some() && cuenta_techo {
                    let r = racha[$j][$cat];
                    CANALES.with(|c| {
                        let mut c = c.borrow_mut();
                        varint(&mut c[$cat], r);
                        varint(&mut c[$cat], v - 1);
                    });
                }
                if racha[$j][$cat] > 0 {
                    q[$j].bl_racha[$cat][bitlen(racha[$j][$cat] - 1)] += 1;
                    racha[$j][$cat] = 0;
                }
                q[$j].bl_valor[$cat][bitlen(v - 1)] += 1;
            }
        }};
    }

    macro_rules! mete {
        ($qq:expr) => {{
            let p = $qq;
            if p + MIN_COPIA <= n {
                let hh = h3(plano, p);
                prev[p & MASCARA] = head[hh];
                head[hh] = p as u32;
            }
        }};
    }

    for b in bloques {
        match b {
            Bloque::Almacenado { datos, .. } => {
                for k in 0..datos.len() { mete!(pos + k); }
                pos += datos.len();
            }
            Bloque::Comprimido { tokens, .. } => {
                let n_tok = tokens.len();
                for (i_tok, t) in tokens.iter().enumerate() {
                    // Se acabo el prefijo: se elige la que menos ha fallado hasta aqui y se
                    // apagan las otras ocho. La configuracion del codificador no cambia a mitad
                    // de flujo, asi que seguir puntuandolas todas es pagar por saber algo que ya
                    // se sabe.
                    if elegida == usize::MAX && pos >= prefijo {
                        // Las K que menos han fallado sobreviven; las demas se apagan. Con K=1
                        // esto es exactamente lo que habia. `elegida` se marca para no repetir el
                        // corte, y la ganadora DE VERDAD se decide al final entre las que quedan.
                        let mut orden: Vec<usize> = (0..N).filter(|&j| activo[j]).collect();
                        orden.sort_by_key(|&j| q[j].fallos());
                        let k = (*ATAJO_K).min(orden.len()).max(1);
                        activo = [false; N];
                        for &j in &orden[..k] { activo[j] = true }
                        elegida = orden[0];
                        sobreviven = activo;
                        // Al apagar configuraciones cambia lo que `mejor_todos` habria devuelto,
                        // asi que lo guardado con las nueve activas ya no vale.
                        guardado = None;
                    }
                    match guardado.take() {
                        Some(g) => r0 = g,
                        None => mejor_todos(plano, pos, head, prev, &cero, &activo, &mut r0),
                    }
                    // La mirada perezosa a pos+1 solo hace falta si ALGUNA configuracion
                    // perezosa tiene aqui una coincidencia por debajo de su umbral `lazy`.
                    let hace_falta = (0..N).any(|j| activo[j] && cfg(j).perezoso
                                                 && r0[j].0 as usize >= min_emitida()
                                                 && (r0[j].0 as usize) < cfg(j).lazy);
                    // 🔑 EL ORDEN DE zlib, Y ERA UN FALLO NUESTRO, NO UNA OPTIMIZACION.
                    //
                    // `deflate_slow` hace INSERT_STRING(strstart) ANTES de llamar a
                    // longest_match, asi que cuando mira la posicion siguiente la cadena YA
                    // CONTIENE la actual. Nosotros insertabamos al avanzar, o sea DESPUES de la
                    // mirada perezosa: nuestra mirada a pos+1 no veia el candidato de pos y la
                    // suya si. No es un detalle de velocidad, es una diferencia en lo que
                    // simulamos.
                    //
                    // ⚠️ El orden importa en las dos direcciones: la busqueda de `pos` tiene que
                    // hacerse ANTES de insertar `pos`, porque un candidato a distancia 0 corta el
                    // recorrido. Por eso se inserta entre las dos y no al principio.
                    if *ORDEN_ZLIB {
                        match t {
                            Token::Lit(_) => { mete!(pos); }
                            Token::Copia { long, .. } => {
                                let l = *long as usize;
                                match pol {
                                    0 => { mete!(pos); }
                                    lim => { let _ = lim; let _ = l; mete!(pos); }
                                }
                            }
                        }
                    }
                    let mirada_hecha = hace_falta;
                    if hace_falta {
                        let previo: [usize; N] = std::array::from_fn(|j| r0[j].0 as usize);
                        mejor_todos(plano, pos + 1, head, prev, &previo, &activo, &mut r1);
                    }
                    // 🔑 EL SALTO ES EL MISMO PARA LAS NUEVE. La cadena la comparten todas las
                    // configuraciones de esta pasada (misma politica de diccionario) y la verdad
                    // —longitud y distancia del token real— tambien es una sola. Asi que se
                    // calcula UNA VEZ por token y no nueve.
                    let mut cache_salto: Option<Option<u32>> = None;
                    macro_rules! salto_de {
                        ($long:expr, $dist:expr) => {{
                            if cache_salto.is_none() {
                                cache_salto = Some(saltos(plano, pos, head, prev, $long, $dist));
                            }
                            cache_salto.unwrap()
                        }};
                    }
                    /// La correccion de distancia tal y como la emite `predict_block`: si fallo la
                    /// longitud va por el canal DistAfterLen y en `saltos-1`; si solo fallo la
                    /// distancia, por DistOnly y en `saltos`; si no fallo nada, un cero.
                    macro_rules! corrige_dist {
                        ($j:expr, $pl:expr, $pd:expr, $long:expr, $dist:expr) => {{
                            if $pl != $long {
                                match salto_de!($long, $dist) {
                                    Some(h) if h >= 1 => emite!($j, CAT_DAL, (h - 1) as u64),
                                    _ => { q[$j].salto_perdido += 1;
                                           emite!($j, CAT_DAL, zigzag($pd, $dist).max(1)) }
                                }
                            } else if $pd != $dist {
                                match salto_de!($long, $dist) {
                                    Some(h) => emite!($j, CAT_DIST, h as u64),
                                    None => { q[$j].salto_perdido += 1;
                                              emite!($j, CAT_DIST, zigzag($pd, $dist).max(1)) }
                                }
                            } else { emite!($j, CAT_DIST, 0) }
                        }};
                    }
                    let mut alguna_acerto = false;
                    for j in 0..N {
                        if !activo[j] { continue }
                        q[j].simbolos += 1;
                        let (l0, d0) = (r0[j].0 as usize, r0[j].1 as usize);
                        let pred = if l0 < min_emitida() { None }
                                   else if cfg(j).perezoso && l0 < cfg(j).lazy && hace_falta
                                           && (r1[j].0 as usize) > l0 { None }
                                   else { Some((l0, d0)) };
                        match (t, pred) {
                            (Token::Lit(_), None) => {
                                alguna_acerto = true;
                                q[j].acierto += 1;
                                if *SALTOS_ON { emite!(j, CAT_LIT, 0) }
                            }
                            (Token::Lit(_), Some(_)) => {
                                q[j].lit_vs_copia += 1;
                                if cuenta_techo && j == elegida_provisional(&q, &activo) {
                                    FIN_FALLO[cubo_fin(n_tok - 1 - i_tok)]
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                }
                                if *SALTOS_ON { emite!(j, CAT_REF, 1) }
                            }
                            (Token::Copia { long, dist, .. }, None) => {
                                q[j].copia_vs_lit += 1;
                                if *SALTOS_ON {
                                    let (long, dist) = (*long as usize, *dist as usize);
                                    // Su `repredict_reference`: si la coincidencia cruda no llega
                                    // ni al minimo del formato, se vuelve a buscar con la cadena
                                    // entera antes de corregir. Corregir «de 0 a 137» cuesta ocho
                                    // bits de magnitud; «de 134 a 137», dos.
                                    let (l0, d0) = if *REPREDICT && l0 < MIN_COPIA {
                                        let r = mejor_uno(plano, pos, head, prev, *CADENA_MAX);
                                        if r.0 >= MIN_COPIA { r } else { (l0, d0) }
                                    } else { (l0, d0) };
                                    emite!(j, CAT_LIT, 1);
                                    // Su `repredict_reference`: cuando predijo literal y la
                                    // verdad era copia, NO da el token por perdido — vuelve a
                                    // buscar y corrige contra ESA busqueda. Lo mas parecido que
                                    // tenemos es la coincidencia cruda (l0,d0), la que las reglas
                                    // del nivel habian descartado.
                                    emite!(j, CAT_LONG, zigzag(l0, long));
                                    corrige_dist!(j, l0, d0, long, dist);
                                }
                            }
                            (Token::Copia { long, dist, .. }, Some((pl, pd))) => {
                                let (long, dist) = (*long as usize, *dist as usize);
                                if *SALTOS_ON {
                                    emite!(j, CAT_REF, 0);
                                    emite!(j, CAT_LONG, zigzag(pl, long));
                                    corrige_dist!(j, pl, pd, long, dist);
                                }
                                if pl == long && pd == dist { alguna_acerto = true; q[j].acierto += 1 }
                                else if pl == long {
                                    q[j].otra_dist += 1;
                                    q[j].bl_dist[bitlen(zigzag(pd, dist))] += 1;
                                }
                                else {
                                    q[j].otra_long += 1;
                                    let dl = if pl > long { pl - long } else { long - pl };
                                    q[j].delta[dl.min(9)] += 1;
                                    q[j].bl_long[bitlen(zigzag(pl, long))] += 1;
                                    // Si ademas falla la distancia, esa correccion tambien se
                                    // paga: antes se cobraba SOLO la de longitud y la distancia
                                    // viajaba gratis.
                                    if pd != dist { q[j].bl_dist[bitlen(zigzag(pd, dist))] += 1; }
                                }
                            }
                        }
                    }
                    if cuenta_techo {
                        FIN_TODOS[cubo_fin(n_tok - 1 - i_tok)]
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    if cuenta_techo && alguna_acerto {
                        EXPLICADO.with(|e| {
                            let mut e = e.borrow_mut();
                            if i_token < e.len() { e[i_token] = true }
                        });
                    }
                    i_token += 1;
                    // SE AVANZA SEGUN EL ORIGINAL, nunca segun la prediccion.
                    // Con el orden de zlib, la posicion `pos` ya se metio arriba: aqui van solo
                    // las de dentro de la coincidencia.
                    // Si el token real era un literal avanzamos UN byte, que es justo la posicion
                    // que la mirada perezosa acaba de calcular, y con la cadena ya en el estado
                    // que tendra. Se guarda. Si era una copia, saltamos mas lejos y no sirve.
                    // 🔑 GUARDAR LA MIRADA, PERO SOLO CUANDO ES DEMOSTRABLEMENTE LA MISMA
                    // LLAMADA. La primera version guardaba siempre y perdio 303.287 aciertos: el
                    // valor de la mirada sale de una cadena REDUCIDA —llama con previo = r0— y la
                    // busqueda de la vuelta siguiente llama con previo = 0, sin reducir.
                    //
                    //     let mut c = cfg.cadena;
                    //     if previo >= cfg.buena { c >>= 2 }
                    //
                    // Pero si `r0 < buena` NINGUNA de las dos reduce: son la misma llamada, con
                    // la misma cadena (con el orden de zlib, `pos` ya esta metido y un literal no
                    // mete nada mas). Ahi guardar no puede cambiar un byte, y hay que exigirlo:
                    // el control de esta medida es que salga IDENTICA.
                    guardado = if *CACHE_PEREZOSA && *ORDEN_ZLIB && mirada_hecha
                                  && matches!(t, Token::Lit(_))
                                  && (0..N).all(|j| !activo[j]
                                                 || (r0[j].0 as usize) < cfg(j).buena)
                               { Some(r1) } else { None };
                    let desde = if *ORDEN_ZLIB { 1 } else { 0 };
                    match t {
                        Token::Lit(_) => { if desde == 0 { mete!(pos); } pos += 1; }
                        Token::Copia { long, .. } => {
                            let l = *long as usize;
                            // EL TERCER EJE DEL CENSO, Y EL QUE NI SE NOS HABIA OCURRIDO: QUE SE
                            // METE EN EL DICCIONARIO. Nosotros metemos SIEMPRE todas las
                            // posiciones de dentro de una coincidencia -eso es `AddAll`- pero el
                            // censo dice que el 30,6% de los flujos NO hace eso: 512 usan
                            // `AddFirst(n)` -solo la primera, si la coincidencia pasa de n-, 48
                            // `AddFirstAndLast` y 43 las variantes de miniz y zlib-ng.
                            //
                            // Y NO ES UN DETALLE: si el codificador original solo indexo la
                            // primera, nuestra cadena tiene candidatos que la SUYA NO TENIA, y
                            // predecimos coincidencias que el no podia haber encontrado. Eso sale
                            // contado como «otra longitud», que era el 9,1% de fallos que no
                            // sabiamos explicar.
                            match pol {
                                0 => { for k in desde..l { mete!(pos + k); } }
                                lim => {
                                    if l <= lim { for k in desde..l { mete!(pos + k); } }
                                    else if desde == 0 { mete!(pos); }
                                }
                            }
                            pos += l;
                        }
                    }
                }
            }
        }
    }
    // ⚠️ LA GANADORA SALE DE ENTRE LAS SUPERVIVIENTES, NO DE LA QUE IBA PRIMERA EN EL PREFIJO.
    // Con K=1 son la misma y esto no cambia nada. Con K>1 es justo el punto: se elige DESPUES de
    // haber corrido el flujo entero con las K, que es lo unico que puede recuperar parte de los
    // 8,76 puntos que hoy pierde el atajo.
    let elegida = if *ATAJO_K > 1 && elegida != usize::MAX {
        (0..N).filter(|&j| sobreviven[j]).min_by_key(|&j| q[j].fallos()).unwrap_or(elegida)
    } else { elegida };
    // La racha que queda abierta al acabar el flujo tambien se escribe: `finish()` la vuelca.
    // Sin esto, un flujo que acaba en mil aciertos los regalaria.
    if *SALTOS_ON {
        for j in 0..N { for c in 0..NCAT {
            if racha[j][c] > 0 {
                q[j].bl_racha[c][bitlen(racha[j][c] - 1)] += 1;
                racha[j][c] = 0;
            }
        } }
    }
    (q, elegida)
}

/// EL TECHO, Y LA PRIMERA VERSION ESTABA MAL.
///
/// 🚨 Contaba «ninguna configuracion activa lo explica» POR PASADA. Pero las nueve corren en
/// CUATRO pasadas agrupadas por politica de diccionario, asi que una posicion que la pasada
/// voraz no explica puede explicarla la perezosa — y salia contada como inexplicable. Daba
/// 18,58% cuando la pregunta es otra.
///
/// Ahora se acumula POR POSICION a traves de las cuatro pasadas de referencia (las del atajo no
/// cuentan: corren un subconjunto y la pregunta es que puede explicar la tabla ENTERA).
thread_local! {
    static EXPLICADO: std::cell::RefCell<Vec<bool>> = std::cell::RefCell::new(Vec::new());
}
static TECHO_SIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static TECHO_TOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// La que va ganando, para atribuirle el fallo a UNA sola configuracion y no a las nueve. Con la
/// del estimador solo hay una y esto devuelve 0 siempre.
fn elegida_provisional(q: &[Cuenta; N], activo: &[bool; N]) -> usize {
    let mut mi = usize::MAX;
    for j in 0..N {
        if !activo[j] { continue }
        if mi == usize::MAX || q[j].fallos() < q[mi].fallos() { mi = j }
    }
    if mi == usize::MAX { 0 } else { mi }
}

/// Deja el mapa a cero con sitio para `n` tokens. Se llama una vez por flujo.
fn techo_reinicia(n: usize) {
    EXPLICADO.with(|e| { let mut e = e.borrow_mut(); e.clear(); e.resize(n, false); });
}
/// Cierra el flujo: cuenta las posiciones que NINGUNA pasada explico.
fn techo_cierra() {
    EXPLICADO.with(|e| {
        let e = e.borrow();
        let sin = e.iter().filter(|&&x| !x).count() as u64;
        TECHO_SIN.fetch_add(sin, std::sync::atomic::Ordering::Relaxed);
        TECHO_TOT.fetch_add(e.len() as u64, std::sync::atomic::Ordering::Relaxed);
    });
}

fn main() {
    // El hook de panico, SELECTIVO: se calla solo mientras corre el estimador de preflate, que
    // revienta en el 8,7% de los flujos. Un panico nuestro sigue saliendo por pantalla.
    let anterior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !DENTRO_DEL_ESTIMADOR.with(|f| f.get()) { anterior(info) }
    }));
    preflate_rs::EST_LIMITE.store(*EST_LIMITE_N, std::sync::atomic::Ordering::Relaxed);
    USA_ESTIMADOR.store(*CFG_ESTIMADOR, std::sync::atomic::Ordering::Relaxed);
    let t_inicio = std::time::Instant::now();
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let tope: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();
    // Muestra REPARTIDA por todo el banco y no «los primeros N». Los ficheros van ordenados por
    // nombre y el nombre agrupa por origen: los 60 primeros daban 97,14% de copias exactas y el
    // banco entero 84,70%. Esa muestra sesgada costo una conclusion equivocada.
    if tope < casos.len() && tope > 0 {
        let paso = casos.len() as f64 / tope as f64;
        casos = (0..tope).map(|i| casos[(i as f64 * paso) as usize].clone()).collect();
    }

    let mut head = vec![u32::MAX; *CUBOS];
    let mut prev = vec![u32::MAX; VENTANA];
    let mut por_cfg = [Cuenta::default(); N];
    // --por-flujo FICHERO: una linea por flujo, para cruzarla con el censo de codificadores
    // (bin/vc-censo-codificadores). Sin esto solo tenemos el agregado, y el agregado no puede
    // contestar «¿fallamos MAS en los que no son zlib?», que es la pregunta que decide si la
    // fase 3 se afina o se reescribe.
    let por_flujo: Option<String> = std::env::args().collect::<Vec<_>>()
        .windows(2).find(|w| w[0] == "--por-flujo").map(|w| w[1].clone());
    let mut lineas: Vec<String> = Vec::new();

    // --oraculo CENSO.tsv: a cada flujo se le pone LO QUE EL CENSO DICE QUE ES (hash y politica
    // de diccionario) en vez de una perilla global. Mide el TECHO de identificar perfectamente:
    // lo que sacariamos si nuestro estimador fuera tan bueno como el suyo. ⚠️ Es un techo, no un
    // resultado — el estimador de verdad se equivocara, y el censo mismo falla en el 8,1%.
    let oraculo: Option<std::collections::HashMap<String, (bool, usize)>> =
        std::env::args().collect::<Vec<_>>().windows(2)
            .find(|w| w[0] == "--oraculo").map(|w| w[1].clone())
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| {
                let mut m = std::collections::HashMap::new();
                for (i, l) in t.lines().enumerate() {
                    if i == 0 { continue }
                    let c: Vec<&str> = l.split('\t').collect();
                    if c.len() < 7 { continue }
                    let ld = c[1].starts_with("Libdeflate");
                    // AddFirst(n) -> n ; AddFirstAndLast(n) -> n ; el resto -> AddAll (0)
                    let pol = if c[6].starts_with("AddFirst(") || c[6].starts_with("AddFirstAndLast(") {
                        c[6].trim_end_matches(')').split('(').nth(1)
                            .and_then(|x| x.parse::<usize>().ok()).unwrap_or(0)
                    } else { 0 };
                    m.insert(c[0].to_string(), (ld, pol));
                }
                m
            });
    if oraculo.is_some() {
        println!("\n  ⚠️ MODO ORACULO: a cada flujo se le da lo que el censo dice que es.");
        println!("     Es el TECHO de identificar perfectamente, NO un resultado del producto.");
    }

    let mut mejor_total = Cuenta::default();
    let mut elegido = [0u64; N];
    let mut deflate_total = 0usize;
    let mut n_flujos = 0u64;
    // A que hash manda el estimador cada flujo. Sin el reparto no se sabe si el +X viene de
    // acertar mucho en pocos o poco en muchos, que son dos historias distintas — y con cinco
    // hashes hace mas falta todavia: los tres nuevos son 72 flujos de 2.522.
    let mut por_hash = [0u64; 5];
    let mut por_min = [0u64; 9];
    let mut n_cadena_corta = 0u64;
    // Los dos avisos que impiden que este modo se lea mejor de lo que es.
    let mut n_est_fallo = 0u64;
    let mut n_pol_aprox = 0u64;
    let mut n_elegida_vacia = 0u64;
    /// Cuantos flujos traen copias de tres MAS LEJOS de lo que la constante de zlib permite. Son
    /// los que la constante a ojo estaba estropeando.
    let mut n_dist3_lejos = 0u64;
    let mut n_sin_tokens = 0u64;
    let mut atajo_total = Cuenta::default();
    let (mut seg_completo, mut seg_atajo, mut acierta_atajo) = (0.0f64, 0.0f64, 0u64);

    // 🔑 LOS RELOJES POR DENTRO — CAMINO P15. El reparto de nuestro tiempo se venia deduciendo
    // RESTANDO totales (161,0 - 67,4 = «lo demas»), que es el nivel 3 de la jerarquia: el ruido
    // de las dos medidas se suma justo donde buscas una parte pequeña. Aqui se mide cada trozo
    // directamente, que es el nivel 1 y estaba a tres lineas de distancia.
    let (mut seg_leer, mut seg_inflar, mut seg_estimar) = (0.0f64, 0.0f64, 0.0f64);
    // UN SOLO PARSEO: cuando esta encendido, el estimador devuelve TAMBIEN los tokens y nuestro
    // inflate no llega a correr. Ver `UN_PARSEO`. El respaldo —su parseo revienta— sigue siendo
    // nuestro inflate, y se cuenta.
    let un_parseo = *UN_PARSEO && *ESTIMA_HASH && oraculo.is_none();
    for p in &casos {
        let tl = std::time::Instant::now();
        let Ok(d) = std::fs::read(p) else { continue };
        seg_leer += tl.elapsed().as_secs_f64();
        let mut de_una: Option<Est> = None;
        let (plano, bloques) = if un_parseo {
            use std::sync::atomic::Ordering::Relaxed;
            let te = std::time::Instant::now();
            let r = una_pasada(&d);
            seg_estimar += te.elapsed().as_secs_f64();
            match r {
                Some((pl, bl, e)) => { UN_PARSEO_OK.fetch_add(1, Relaxed); de_una = Some(e); (pl, bl) }
                None => {
                    UN_PARSEO_RESPALDO.fetch_add(1, Relaxed);
                    let ti = std::time::Instant::now();
                    let Ok(x) = descomprime_con_bloques(&d) else { continue };
                    seg_inflar += ti.elapsed().as_secs_f64();
                    x
                }
            }
        } else {
            let ti = std::time::Instant::now();
            let Ok(x) = descomprime_con_bloques(&d) else { continue };
            seg_inflar += ti.elapsed().as_secs_f64();
            x
        };
        if plano.len() < MIN_COPIA { continue }
        if let Some(o) = &oraculo {
            let k = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
            // ⚠️ El oraculo sigue siendo BINARIO (zlib / libdeflate) porque asi se escribio
            // cuando solo habia dos. Como referencia ya no vale para los tres nuevos, y por eso
            // el techo que publicaba se quedo corto — ver el comentario de ESTIMA_HASH.
            let (ld, pol) = o.get(k).copied().unwrap_or((false, 0));
            HASH_LD.store(if ld { H_LIBDEFLATE } else { H_ZLIB }, std::sync::atomic::Ordering::Relaxed);
            POL_ADD.store(pol, std::sync::atomic::Ordering::Relaxed);
        } else if *ESTIMA_HASH {
            // El hash lo decide el estimador; la politica, el nivel. Ver ESTIMA_HASH.
            use std::sync::atomic::Ordering::Relaxed;
            // Con UN SOLO PARSEO ya viene de la pasada de arriba; si aquello cayo al respaldo,
            // hay que estimar aparte —y entonces si se paga el doble parseo, en esos flujos—.
            let e = match de_una.take() {
                Some(e) => e,
                None => {
                    let te = std::time::Instant::now();
                    let e = hash_estimado(&d);
                    seg_estimar += te.elapsed().as_secs_f64();
                    e
                }
            };
            HASH_LD.store(e.hash, Relaxed);
            MIN_EMIT.store(MIN_EMIT_FIJO.unwrap_or(e.min), Relaxed);
            // ⚠️ SOLO POR DEBAJO DE 4 cuando se corre con los nueve presets: por encima, la cadena
            // es lo que los distingue y acotarla los volveria indistinguibles. Con la
            // configuracion del estimador esa objecion desaparece —no hay nivel que distinguir—
            // y la cadena se usa entera, que es lo que hace preflate.
            let tope = if *CFG_ESTIMADOR || e.cadena < 4 { e.cadena.max(1) } else { usize::MAX };
            TOPE_CADENA.store(TOPE_CADENA_FIJO.unwrap_or(tope), Relaxed);
            if tope != usize::MAX && e.cadena < 4 { n_cadena_corta += 1 }
            E_PEREZOSO.store(e.perezoso, Relaxed);
            E_BUENA.store(e.buena, Relaxed);
            E_LAZY.store(e.lazy, Relaxed);
            E_NICE.store(e.nice, Relaxed);
            E_CADENA.store(e.cadena, Relaxed);
            E_POL.store(e.pol, Relaxed);
            E_DIST3.store(if *DIST3_ESTIMADOR { e.dist3 } else { DEMASIADO_LEJOS }, Relaxed);
            if e.dist3 > DEMASIADO_LEJOS { n_dist3_lejos += 1 }
            if e.fallo { n_est_fallo += 1 }
            if e.pol_aprox { n_pol_aprox += 1 }
            por_min[e.min.min(8)] += 1;
            por_hash[e.hash as usize] += 1;
        }
        // ⚠️ ESTA AUDITORIA SOLO VALE CON NUESTROS TOKENS. Pregunta si el SIMBOLO que venia en el
        // flujo coincide con el canonico, y con UN SOLO PARSEO el simbolo lo derivamos NOSOTROS
        // del canonico: saldria «0 discrepancias» siempre, que no es una medida sino un espejo.
        // Se apaga y se dice, en vez de publicar un 100% que no significa nada.
        if !un_parseo {
            use std::sync::atomic::Ordering::Relaxed;
            for b in &bloques {
                if let Bloque::Comprimido { tokens, .. } = b {
                    for t in tokens {
                        if let Token::Copia { long, dist, sim_long, sim_dist } = t {
                            SIM_TOT.fetch_add(1, Relaxed);
                            // El simbolo canonico: el mayor i con LB[i] <= long.
                            let mut il = 0usize;
                            while il + 1 < 29 && LB[il + 1] <= *long { il += 1 }
                            // ⚠️ 258 NO es lo mismo que AMBIGUO: admite el 285 (estandar) y el
                            // 284 con los cinco extras a uno. Solo el segundo obliga a guardar
                            // algo. Contar «cuantos son 258» y llamarlo ambiguedad seria el
                            // mismo error de denominador de todo el dia.
                            if *long == 258 {
                                SIM_258.fetch_add(1, Relaxed);
                                if *sim_long == 284 { SIM_284.fetch_add(1, Relaxed); }
                            }
                            else if *sim_long as usize != il + 257 { SIM_LONG_MAL.fetch_add(1, Relaxed); }
                            let mut id = 0usize;
                            while id + 1 < 30 && DB[id + 1] <= *dist { id += 1 }
                            if *sim_dist as usize != id { SIM_DIST_MAL.fetch_add(1, Relaxed); }
                        }
                    }
                }
            }
        }
        deflate_total += d.len();
        n_flujos += 1;
        // REFERENCIA: las nueve, enteras. Es contra esto contra lo que se mide el atajo.
        let t0 = std::time::Instant::now();
        let mut q = [Cuenta::default(); N];
        let mut corrio = [false; N];
        techo_reinicia(bloques.iter().map(|b| match b {
            Bloque::Comprimido { tokens, .. } => tokens.len(),
            _ => 0,
        }).sum());
        for (pol, act) in pasadas() {
            let (qp, _) = evalua(&plano, &bloques, &mut head, &mut prev, usize::MAX, act, pol);
            for j in 0..N { if act[j] { q[j] = qp[j]; corrio[j] = true; } }
        }
        // 🔑 EL TECHO SOBRE LAS DIEZ. El de las nueve (1,55%) es una COTA: la configuracion del
        // estimador tiene parametros continuos y explica cosas que ningun preset expresa. Aqui se
        // corre UNA pasada mas con ella, marcando en el MISMO mapa por posicion, y lo que quede
        // sin marcar es el suelo de verdad de este enfoque.
        //
        // Solo cuenta para el techo: su Cuenta se tira, porque mezclarla con las nueve seria
        // comparar poblaciones distintas.
        if *TECHO_DIEZ && !*CFG_ESTIMADOR {
            USA_ESTIMADOR.store(true, std::sync::atomic::Ordering::Relaxed);
            let mut a = [false; N]; a[0] = true;
            let _ = evalua(&plano, &bloques, &mut head, &mut prev, usize::MAX, a,
                           E_POL.load(std::sync::atomic::Ordering::Relaxed));
            USA_ESTIMADOR.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        let (q, corrio) = (q, corrio);
        seg_completo += t0.elapsed().as_secs_f64();
        // ⚠️ SOLO ENTRE LAS QUE HAN CORRIDO. Con los nueve presets corren las nueve y esto no
        // cambia nada; con la configuracion del estimador corre UNA, y las ocho ranuras vacias
        // valen Cuenta::default(): CERO fallos sobre CERO simbolos, y nadie puede ganarles.
        //
        // 🚨 ES EL MISMO FALLO QUE 90d2d20, EN OTRO SITIO DEL MISMO FICHERO. Alli el atajo elegia
        // sin filtrar por `activo[j]`; aqui `main` elegia sin filtrar por «ha corrido». Arregle
        // aquel y NO busque el patron en el resto del fichero. Sintoma: acierto 100,00% con 0
        // fallos sobre 16 M simbolos de los 125 M del banco.
        let mut mi = corrio.iter().position(|&c| c).unwrap_or(0);
        for j in 0..N { if corrio[j] && q[j].fallos() < q[mi].fallos() { mi = j } }
        // EL CANDADO, NO LA DISCIPLINA — y afinado, porque en su primera version grito en falso.
        //
        // La condicion de ERROR no es «la elegida vio cero simbolos»: es «vio cero MIENDO OTRA
        // los vio». Con los nueve presets las nueve ranuras corren en las cuatro pasadas y ven
        // exactamente los mismos tokens, asi que cero en la elegida implica cero en todas — o
        // sea, un flujo SIN tokens que predecir (los 21 que el censo lista con cadena 0 y nice
        // 0). Eso no corrompe nada: suma cero. Contarlo como error hacia que el candado avisara
        // 21 veces de algo que esta bien, y un candado que grita en falso se acaba ignorando.
        let vio_alguna = (0..N).any(|j| corrio[j] && q[j].simbolos > 0);
        if q[mi].simbolos == 0 {
            if vio_alguna { n_elegida_vacia += 1 } else { n_sin_tokens += 1 }
        }
        techo_cierra();
        for j in 0..N { por_cfg[j].suma(&q[j]); }
        elegido[mi] += 1;
        mejor_total.suma(&q[mi]);
        if por_flujo.is_some() {
            lineas.push(format!("{}\t{}\t{}\t{}\t{}",
                p.file_name().and_then(|x| x.to_str()).unwrap_or("?"),
                q[mi].simbolos, q[mi].acierto, cfg(mi).nivel, d.len()));
        }

        // ATAJO: elegir en el prefijo y correr solo la ganadora. Lo unico que puede empeorar es
        // el acierto, y por eso se cuenta aparte y se compara.
        let pref = PREFIJO_FIJO.unwrap_or_else(|| (plano.len() / 5).clamp(64 * 1024, 512 * 1024));
        let t1 = std::time::Instant::now();
        // El atajo elige DENTRO de cada pasada; la ganadora global sale de comparar las
        // ganadoras de las cuatro, que es exactamente lo que hace el camino completo.
        let mut qa = [Cuenta::default(); N];
        let mut el = usize::MAX;
        for (pol, act) in pasadas() {
            let (qp, e) = evalua(&plano, &bloques, &mut head, &mut prev, pref, act, pol);
            for j in 0..N { if act[j] { qa[j] = qp[j]; } }
            let e = if e == usize::MAX { (0..N).filter(|&j| act[j])
                        .min_by_key(|&j| qp[j].fallos()).unwrap_or(0) } else { e };
            if el == usize::MAX || qa[e].fallos() < qa[el].fallos() { el = e }
        }
        let (qa, el) = (qa, el);
        seg_atajo += t1.elapsed().as_secs_f64();
        let el = if el == usize::MAX { mi } else { el };
        if el == mi { acierta_atajo += 1 }
        atajo_total.suma(&qa[el]);
    }
    if *ESTIMA_HASH && oraculo.is_none() {
        println!("\n  A QUE HASH MANDA EL ESTIMADOR CADA FLUJO ({n_flujos} en total)");
        for (i, nom) in ["zlib", "libdeflate", "miniz", "zlib-ng", "crc32c"].iter().enumerate() {
            if por_hash[i] == 0 { continue }
            println!("    {:>6} {:>5.1}%  {nom}", por_hash[i],
                     100.0 * por_hash[i] as f64 / n_flujos.max(1) as f64);
        }
        if *CFG_ESTIMADOR {
            println!("\n  CONFIGURACION DEL ESTIMADOR, UNA POR FLUJO (no uno de los nueve presets)");
            println!("    ⚠ {n_est_fallo} flujos con el estimador reventado: van al nivel 6 de respaldo");
            println!("    ⚠ {n_pol_aprox} flujos con politica que NO sabemos expresar: aproximada");
        }
        if n_dist3_lejos > 0 {
            println!("\n  {n_dist3_lejos} flujos emiten copias de TRES bytes mas alla de los 4.096 de zlib");
            println!("    (la constante que teniamos a ojo). Con VC_DIST3=1 manda la del estimador.");
        }
        if n_cadena_corta > 0 {
            println!("\n  {n_cadena_corta} flujos buscan MENOS candidatos que nuestra config mas corta (cadena < 4)");
        }
        println!("\n  Y LA LONGITUD MINIMA QUE EMITE CADA UNO (la del formato es 3)");
        for i in MIN_COPIA..9 {
            if por_min[i] == 0 { continue }
            println!("    {:>6} {:>5.1}%  minimo {i}", por_min[i],
                     100.0 * por_min[i] as f64 / n_flujos.max(1) as f64);
        }
    }
    let seg = t_inicio.elapsed().as_secs_f64();

    let ent = |p: f64| if p <= 0.0 || p >= 1.0 { 0.0 }
                       else { -p * p.log2() - (1.0 - p) * (1.0 - p).log2() };
    // La entropia empirica de un histograma: lo que costaria codificarlo con un modelo que
    // ya conoce su forma. Es el limite al que converge un codificador adaptativo.
    let h_hist = |h: &[u64]| -> f64 {
        let n: u64 = h.iter().sum();
        if n == 0 { return 0.0 }
        h.iter().filter(|&&c| c > 0)
         .map(|&c| { let p = c as f64 / n as f64; -(c as f64) * p.log2() }).sum()
    };
    // LA COTA ALTA: cada correccion a tarifa plana, falle por 1 o por 200. Es la de antes, y se
    // conserva a proposito — sin ella no se ve cuanto de aquel 13,14% era el modelo y no el dato.
    let coste_plano = |q: &Cuenta| -> (f64, f64) {
        let p = if q.simbolos == 0 { 0.0 } else { q.fallos() as f64 / q.simbolos as f64 };
        let marca = q.simbolos as f64 * ent(p);
        let corr = (q.lit_vs_copia + q.copia_vs_lit) as f64 * 4.0
                 + q.otra_dist as f64 * 4.0
                 + q.otra_long as f64 * 8.0;
        (marca, corr)
    };
    // EL MODELO UNICO. Un solo numero, cada cosa cobrada UNA VEZ.
    //
    // El error que corrige: el modelo anterior cobraba la marca «¿acerte?» por un lado y la
    // CLASE de fallo por otro, o sea parte de la misma informacion dos veces; y el plano dejaba
    // la clase gratis. Dos modelos que no cobran lo mismo no son dos cotas de una cantidad: son
    // dos cantidades, y compararlos no significa nada. Lo vio el dueño.
    //
    //   1. EL DESENLACE. Cinco posibles por simbolo: acierto, literal-donde-dije-copia,
    //      copia-donde-dije-literal, otra distancia, otra longitud. Su entropia YA INCLUYE la
    //      marca y la clase, sin contar nada dos veces.
    //   2. LA MAGNITUD, solo cuando hay fallo de magnitud: bits del zig-zag a la entropia de su
    //      histograma, mas los (bl-1) de mantisa, que son casi uniformes.
    //   3. La distancia se cobra SIEMPRE que difiera, tambien cuando ademas falla la longitud.
    //
    // ⚠️ Es la entropia de un codificador IDEAL: una COTA INFERIOR de lo que lograria uno real,
    // que paga el aprendizaje. Contra el 1,700% REAL de preflate la comparacion no es exacta,
    // pero va en la direccion SEGURA: si la cota inferior ya lo supera, somos peores y no hay
    // discusion.
    let coste_ent = |q: &Cuenta| -> (f64, f64) {
        let desenlaces = [q.acierto, q.lit_vs_copia, q.copia_vs_lit, q.otra_dist, q.otra_long];
        let desenlace = h_hist(&desenlaces);
        let mantisa = |h: &[u64]| -> f64 {
            h.iter().enumerate().map(|(bl, &c)| c as f64 * (bl.max(1) - 1) as f64).sum()
        };
        let magnitud = h_hist(&q.bl_long) + mantisa(&q.bl_long)
                     + h_hist(&q.bl_dist) + mantisa(&q.bl_dist);
        (desenlace, magnitud)
    };
    let coste = coste_plano;
    let pc = |b: f64| 100.0 * (b / 8.0) / deflate_total.max(1) as f64;

    println!();
    println!("  LA FASE 3 — {n_flujos} flujos, {} simbolos, {deflate_total} B de DEFLATE, {seg:.1} s",
             mejor_total.simbolos);
    println!();
    println!("  {:<8} {:>9} {:>9} {:>9} {:>9} {:>9}  {:>8}",
             "config", "acierto", "lit≠copia", "copia≠lit", "otra dist", "otra long", "coste");
    for j in 0..N {
        let q = &por_cfg[j];
        let (m, cr) = coste(q);
        let s = q.simbolos.max(1) as f64;
        println!("  nivel {:<2} {:>8.2}% {:>9} {:>9} {:>9} {:>9}  {:>7.3}%",
                 CFGS[j].nivel, 100.0 * q.acierto as f64 / s,
                 q.lit_vs_copia, q.copia_vs_lit, q.otra_dist, q.otra_long, pc(m + cr));
    }
    if n_elegida_vacia > 0 {
        println!();
        println!("  🚨 {n_elegida_vacia} flujos han elegido una configuracion vacia HABIENDO OTRA que si");
        println!("     corrio. Eso si corrompe `mejor_total`. No es un aviso: es un error.");
    }
    if n_sin_tokens > 0 {
        println!();
        println!("  ({n_sin_tokens} flujos no tienen ni un token que predecir: todo literales o almacenado.");
        println!("   Suman cero y no afectan a ninguna cuenta. Se dicen para que no parezcan un fallo.)");
    }
    println!();
    let (m, cr) = coste(&mejor_total);
    let s = mejor_total.simbolos.max(1) as f64;
    let nuestros_bytes = ((m + cr) / 8.0) as usize;
    println!("  LA MEJOR CONFIG DE CADA FLUJO (esto es lo que haria la fase 3):");
    println!("    acierto {:.2}%   fallos {}", 100.0 * mejor_total.acierto as f64 / s,
             mejor_total.fallos());
    println!("    marca por entropia {:>12} B  {:>7.3}%", (m / 8.0) as usize, pc(m));
    println!("    correcciones       {:>12} B  {:>7.3}%", (cr / 8.0) as usize, pc(cr));
    println!("    TOTAL              {:>12} B  {:>7.3}%", nuestros_bytes, pc(m + cr));
    let (me, ce) = coste_ent(&mejor_total);
    println!();
    println!("  EL COSTE, UN SOLO MODELO — cada cosa cobrada una vez, nada gratis, nada doble:");
    println!("    el DESENLACE  (cual de los 5, a su entropia)                  {:>7.3}%", pc(me));
    println!("      -> incluye «¿acerte?» Y cual de los cuatro fallos fue");
    println!("    la MAGNITUD   (bits del zig-zag + mantisa)                    {:>7.3}%", pc(ce));
    println!("    TOTAL, con un codificador IDEAL                               {:>7.3}%", pc(me + ce));
    println!("    preflate, MEDIDO sobre este mismo banco                         1,700%");
    println!();
    println!("  El total de arriba es una COTA INFERIOR: un codificador real paga el aprendizaje");
    println!("  y no llega. Asi que contra el 1,700% REAL de preflate la comparacion va en la");
    println!("  direccion segura — si nuestra cota inferior ya lo supera, somos peores sin");
    println!("  discusion, y no sirve de nada mejorar el codificador.");
    println!();
    println!("  Y MIRA DONDE ESTA: si el DESENLACE solo ya supera el 1,700%, entonces el problema");
    println!("  es que ACERTAMOS POCO. Si el desenlace es pequeño y la magnitud grande, entonces");
    println!("  acertamos bien y fallamos POR MUCHO, que es otro problema y tiene otro arreglo.");
    println!("  (Se conserva el modelo plano de antes, {:>7.3}%, solo para ver cuanto de aquel", pc(m + cr));
    println!("   13,14% era el modelo y no el dato. No es comparable con lo de arriba.)");
    println!();

    // 🔑 EL MISMO DATO CON LA ESTRUCTURA DE preflate, LEIDA DE SU CODIGO.
    //
    // Los dos modelos de arriba son NUESTROS y comparten un supuesto que su codec no hace: que
    // cada simbolo paga su marca. El suyo no. Al leer token_predictor.rs y cabac_codec.rs salen
    // tres diferencias, y ninguna es cosmetica:
    //
    //   1. la distancia se corrige en SALTOS de cadena (calculate_hops), no en zig-zag de la
    //      resta de distancias. Un salto tipico es 1 o 2; una resta de distancias, cinco cifras.
    //   2. un acierto NO emite simbolo: alarga una racha, y la racha se escribe una vez. Cobrar
    //      `simbolos x H(p)` da por hecho que los fallos son independientes, y se apelotonan.
    //   3. un canal por categoria con su propio contexto, no un alfabeto de cinco mezclado.
    //      Mezclar un canal que es 99,9% cero con otro que es 85% cero sale al precio del peor.
    //
    // Se cobra con NUESTRO mismo convenio (entropia del histograma de longitudes en bits + los
    // bl-1 de mantisa) para que la unica diferencia sea la ESTRUCTURA y no el contable.
    let mantisa = |h: &[u64]| -> f64 {
        h.iter().enumerate().map(|(bl, &c)| c as f64 * (bl.max(1) - 1) as f64).sum()
    };
    let coste_pf = |q: &Cuenta| -> [f64; NCAT] {
        std::array::from_fn(|c| h_hist(&q.bl_racha[c]) + mantisa(&q.bl_racha[c])
                             + h_hist(&q.bl_valor[c]) + mantisa(&q.bl_valor[c]))
    };
    if *SALTOS_ON {
        let cs = coste_pf(&mejor_total);
        let tot: f64 = cs.iter().sum();
        println!("  EL MISMO DATO CON LA ESTRUCTURA DE preflate (leida de su codigo, no supuesta):");
        println!("  {:<16} {:>12} {:>14}  {:>8}", "canal", "rachas", "correcciones", "coste");
        for c in 0..NCAT {
            let r: u64 = mejor_total.bl_racha[c].iter().sum();
            let v: u64 = mejor_total.bl_valor[c].iter().sum();
            println!("    {:<14} {:>12} {:>14}  {:>7.3}%", CAT_NOMBRE[c], r, v, pc(cs[c]));
        }
        println!("    {:<14} {:>27}  {:>7.3}%", "TOTAL", "", pc(tot));
        println!("    {:<14} {:>27}  {:>7.3}%", "preflate real", "", 1.700);
        println!();
        if mejor_total.salto_perdido > 0 {
            println!("  ⚠ {} correcciones cuyo candidato NO aparece en nuestra cadena. Se cobran a la",
                     mejor_total.salto_perdido);
            println!("    vieja usanza (zig-zag), que es lo caro, para no maquillar la cifra. preflate");
            println!("    ahi aborta con PredictionFailure, asi que tampoco le sale gratis.");
            println!();
        }
    }
    println!("  DE LAS «otra longitud» —donde queda casi todo el coste—, POR CUANTO se falla:");
    let td: u64 = mejor_total.delta.iter().sum();
    for (i, v) in mejor_total.delta.iter().enumerate() {
        if *v == 0 { continue }
        let et = if i == 9 { "9 bytes o mas".to_string() } else { format!("{i} byte(s)") };
        println!("    {:<20} {:>10}  {:>6.1}%", et, v, 100.0 * *v as f64 / td.max(1) as f64);
    }
    println!();
    println!("  QUE CONFIG GANO EN CADA FLUJO:");
    for j in 0..N {
        if elegido[j] == 0 { continue }
        println!("    nivel {:<2} ({:<8}) {:>6} flujos  {:>5.1}%", CFGS[j].nivel,
                 if CFGS[j].perezoso { "perezoso" } else { "voraz" }, elegido[j],
                 100.0 * elegido[j] as f64 / n_flujos.max(1) as f64);
    }

    // ==================== LA CUENTA DEL DINERO ====================
    //
    // El dueño lo puso como condicion: «el dinero manda». Asi que «mejor que preflate» tiene que
    // decidirse en euros, no en porcentajes. Los euros son dos sumandos y los dos estan medidos:
    // lo que se GUARDA (alquiler del disco, todos los meses) y lo que se TARDA (la maquina, una
    // vez). Las cifras de preflate salen de bin/vc-preflate-coste sobre este mismo banco.
    // 🚨 ESTAS CONSTANTES ESTUVIERON MEDIDAS SOBRE OTRO BANCO. Hasta el 02-08 valian 6_371_168 B
    // y 172,969 s sobre el banco VIEJO (374 MB, 1.973 PNG) mientras lo nuestro se media sobre
    // banco2 (187 MB, mixto). Se escalaba cada lado con su propio denominador —que es correcto
    // para una TASA— pero las dos tasas venian de contenidos distintos, y ademas el numerador de
    // preflate salia del filtro roto: correcciones del subconjunto que consume, divididas por el
    // banco entero. Ahora las dos columnas salen de bin/vc-preflate-reparto sobre banco2.
    //
    // Y ES TODO INCLUIDO: preflate no consume 91 de los 2.522 flujos —se niega cuando la cadena
    // observada pasa de 4.096, complevel_estimator.rs:86, que NO es una perilla— y esos hay que
    // guardarlos enteros. Contar solo lo que sabe hacer seria regalarle los 3,6% mas dificiles.
    //
    //   correcciones sobre lo que consume   2.758.450 B  de 183.651.819 B   = 1,502%
    //   los 91 que rechaza, enteros         3.618.022 B
    //   TOTAL                               6.376.472 B  de 187.269.841 B   = 3,405%
    const PREFLATE_B: f64 = 6_376_472.0;
    const PREFLATE_S: f64 = 54.1;         // inflar + estimar + re-deflate de verificacion
    const INFLATE_S: f64 = 5.16;          // nuestro inflate, prorrateado a banco2
    /// El DEFLATE del banco ENTERO, que es sobre lo que estan medidas las dos constantes de
    /// arriba. CADA LADO SE ESCALA CON SU PROPIA ENTRADA: escalar las cifras de preflate con el
    /// denominador de NUESTRA muestra las inflaba 39 veces y daba un «ganamos» inventado. Es el
    /// mismo error que este proyecto lleva persiguiendo todo el dia —comparar dos cosas medidas
    /// sobre corpus distintos— y esta vez lo delato lo absurdo del numero, no revisar el metodo.
    const BANCO_B: f64 = 187_269_841.0;
    /// 🔑 LA CALIBRACION, Y POR QUE HACE FALTA. Nuestro coste es un MODELO —entropia de los
    /// histogramas mas mantisa— y el suyo son BYTES REALES de un aritmetico adaptativo. Compararlos
    /// a pelo es el error que este proyecto lleva todo el dia persiguiendo. Pero el modelo se puede
    /// CALIBRAR: bin/vc-preflate-reparto cobra SU salida con NUESTRO mismo contable y sale 1,111%
    /// donde sus bytes reales son 1,502%. O sea que el modelo se queda corto por un factor 1,352.
    ///
    /// Se aplica el MISMO factor al nuestro. No es exacto —su codec y el que escribiriamos no
    /// tienen por que perder lo mismo en el aprendizaje— pero es infinitamente mejor que comparar
    /// un modelo contra unos bytes, y va en la direccion incomoda para nosotros, que es la unica
    /// en la que un ajuste asi es admisible.
    const CALIBRA: f64 = 1.502 / 1.111;
    const EUR_H: f64 = 46.0 / (30.0 * 24.0);
    const PB: f64 = 1e15;
    const MESES: f64 = 12.0;
    // ── EL ATAJO: que cuesta y que ahorra ──────────────────────────────────────────────────
    let (ma, ca) = coste(&atajo_total);
    let atajo_bytes = ((ma + ca) / 8.0) as usize;
    let sa = atajo_total.simbolos.max(1) as f64;
    println!();
    println!("  ================= EL ATAJO: elegir en el prefijo =================");
    println!();
    println!("  {:<30} {:>10} {:>12} {:>10}", "", "acierto", "coste", "segundos");
    println!("  {:<30} {:>9.2}% {:>11.3}% {:>10.1}", "las nueve, enteras",
             100.0 * mejor_total.acierto as f64 / s, pc(m + cr), seg_completo);
    println!("  {:<30} {:>9.2}% {:>11.3}% {:>10.1}", "elegir en el prefijo",
             100.0 * atajo_total.acierto as f64 / sa, pc(ma + ca), seg_atajo);
    println!();
    println!("  el prefijo elige la MISMA config en {} de {} flujos ({:.1}%)",
             acierta_atajo, n_flujos, 100.0 * acierta_atajo as f64 / n_flujos.max(1) as f64);
    println!("  y cuesta {:.1} veces menos reloj", if seg_atajo > 0.0 { seg_completo / seg_atajo } else { 0.0 });

    // Todo se lleva a «por PB de DEFLATE de entrada», y CADA LADO CON SU PROPIA ESCALA.
    let esc_n = PB / deflate_total.max(1) as f64;      // la de nuestra muestra
    let esc_p = PB / BANCO_B;                          // la del banco entero
    // Nuestro reloj: el inflate SIEMPRE hay que pagarlo -tambien lo paga preflate- y encima la
    // estimacion. Se cobra la del ATAJO, que es lo que de verdad se ejecutaria. El inflate se
    // prorratea por BYTES y no por numero de flujos: los flujos no miden lo mismo.
    // 🚨 EL RELOJ NUESTRO SE COBRABA A OJO Y SALIA UN 29% BARATO.
    //
    // Antes: `INFLATE_S * (bytes/BANCO_B) + seg_atajo`, o sea una constante de inflate medida
    // otro dia mas la simulacion. Y no cobraba lo mas caro que hacemos: llamar a
    // `parametros_estimados` en CADA flujo, que es el estimador de preflate entero —inflate suyo
    // incluido—. Dependemos de el y viajaba gratis.
    //
    // Ahora se cobra lo que de verdad tarda: TODO el reloj de la corrida menos la pasada de
    // REFERENCIA, que solo existe para medir lo que cuesta el atajo y no se ejecutaria en
    // produccion. Es decir: leer + inflar + estimar + UNA simulacion. Nada a ojo.
    //
    // Sobre banco2: 161,0 s totales - 67,4 de referencia = 93,6 s, contra los 72,7 que cobraba.
    let nuestro_s = (seg - seg_completo).max(seg_atajo);
    let _ = INFLATE_S;
    // Si la contabilidad con su estructura esta encendida, es ESA la que va al dinero: es la que
    // se puede comparar con la suya. Y calibrada, que es lo que la hace honesta.
    let nuestros_bytes = if *SALTOS_ON {
        let cs = coste_pf(&mejor_total);
        ((cs.iter().sum::<f64>() / 8.0) * CALIBRA) as usize
    } else { atajo_bytes };
    let _ = seg;
    {
        use std::sync::atomic::Ordering::Relaxed;
        let (sin, tot) = (TECHO_SIN.load(Relaxed), TECHO_TOT.load(Relaxed));
        if tot > 0 {
            println!();
            println!("  ================= EL TECHO: LO QUE NINGUN AJUSTE ARREGLA =================");
            println!();
            println!("  {:<54} {:>14}", "tokens del banco", tot);
            println!("  {:<54} {:>14} {:>6.2}%", "que NINGUNA configuracion explica", sin,
                     100.0 * sin as f64 / tot as f64);
            println!();
            println!("  Un fallo por elegir mal la configuracion se arregla ajustando. Uno que");
            println!("  ninguna configuracion explica, NO: es el suelo de este modelo. Sin este");
            println!("  numero, «¿podemos bajar de X?» no tiene respuesta.");
        }
    }
    {
    {
        use std::sync::atomic::Ordering::Relaxed;
        let f: Vec<u64> = (0..12).map(|i| FIN_FALLO[i].load(Relaxed)).collect();
        let t: Vec<u64> = (0..12).map(|i| FIN_TODOS[i].load(Relaxed)).collect();
        let (sf, st): (u64, u64) = (f.iter().sum(), t.iter().sum());
        if st > 0 {
            println!();
            println!("  ==== ¿SE APELOTONAN LOS FALLOS «dijimos copia y era literal» AL FINAL? ====");
            println!();
            println!("  {:<24} {:>14} {:>14} {:>10}", "tokens hasta el fin", "tokens", "fallos", "tasa");
            for i in 0..12 {
                if t[i] == 0 { continue }
                let nom = if i < 10 { format!("quedan {i}") }
                          else if i == 10 { "quedan 10-99".to_string() }
                          else { "quedan 100 o mas".to_string() };
                println!("  {:<24} {:>14} {:>14} {:>9.3}%", nom, t[i], f[i],
                         100.0 * f[i] as f64 / t[i] as f64);
            }
            println!("  {:<24} {:>14} {:>14} {:>9.3}%", "TODOS", st, sf,
                     100.0 * sf as f64 / st as f64);
            println!();
            println!("  Si la tasa de los ultimos tokens es MUCHO mayor que la del resto, el");
            println!("  codificador estaba sin margen para buscar (MIN_LOOKAHEAD = 262) y eso se");
            println!("  puede modelar. Si es parecida, la hipotesis es mala y hay que mirar otra cosa.");
        }
    }
        let t = TOPADOS.load(std::sync::atomic::Ordering::Relaxed);
        println!();
        println!("  ====== ¿ALGUN FLUJO TOCA EL TOPE DE CADENA? (la pregunta abierta) ======");
        println!();
        println!("  {:<54} {:>14}", "tope actual (CADENA_MAX)", *CADENA_MAX);
        println!("  {:<54} {:>14}", "veces que el recorrido para POR EL TOPE y quedaba cadena", t);
        if t == 0 {
            println!("\n  El tope no recorta NADA en este banco. Queda contestado: no es que sea");
            println!("  el numero correcto, es que da igual — y eso tambien es una respuesta.");
        } else {
            println!("\n  SI recorta. Subirlo lo pagan todos los flujos, asi que la pregunta es si");
            println!("  mejora MAS de lo que cuesta, y eso se contesta en €/PB-año.");
        }
    }
    println!();
    println!("  ================= LOS PASOS DE CADENA, POR SITIO =================");
    println!();
    let pasos: Vec<u64> = (0..3).map(|i| PASOS[i].load(std::sync::atomic::Ordering::Relaxed)).collect();
    let tp: u64 = pasos.iter().sum();
    for (nom, v) in [("la busqueda principal", pasos[0]),
                     ("la mirada perezosa a pos+1", pasos[1]),
                     ("los saltos de preflate (la correccion de distancia)", pasos[2])] {
        println!("  {:<54} {:>14} {:>6.1}%", nom, v, 100.0 * v as f64 / tp.max(1) as f64);
    }
    println!("  {:<54} {:>14}", "TOTAL", tp);
    println!("  {:<54} {:>14.1}", "pasos por simbolo", tp as f64 / mejor_total.simbolos.max(1) as f64);
    let des = DESCARTES.load(std::sync::atomic::Ordering::Relaxed);
    println!("  {:<54} {:>14} {:>6.1}%", "descartados con CUATRO BYTES (zlib, deflate.c:1449)",
             des, 100.0 * des as f64 / tp.max(1) as f64);
    {
        use std::sync::atomic::Ordering::Relaxed;
        let (tot, ml, md, c258) = (SIM_TOT.load(Relaxed), SIM_LONG_MAL.load(Relaxed),
                                   SIM_DIST_MAL.load(Relaxed), SIM_258.load(Relaxed));
        if tot > 0 {
            println!();
            println!("  ============ ¿HACEN FALTA LOS SIMBOLOS GUARDADOS? ============");
            println!();
            println!("  {:<50} {:>14}", "copias del banco", tot);
            let c284 = SIM_284.load(Relaxed);
            println!("  {:<50} {:>14} {:>7.4}%", "de longitud 258", c258,
                     100.0 * c258 as f64 / tot as f64);
            println!("  {:<50} {:>14} {:>7.4}%", "   ...y AMBIGUAS de verdad (codigo 284)", c284,
                     100.0 * c284 as f64 / tot as f64);
            println!("  {:<50} {:>14} B", "tamano REAL de nuestro Token hoy",
                     std::mem::size_of::<Token>());
            println!("  {:<50} {:>14}", "con sim_long NO derivable (excluido el 258)", ml);
            println!("  {:<50} {:>14}", "con sim_dist NO derivable", md);
            println!();
            if ml == 0 && md == 0 {
                println!("  ✅ Los dos son derivables. `sim_dist` sobra entero y `sim_long` se reduce");
                if c284 == 0 {
                    println!("     Y NI SIQUIERA UN BIT: ninguna copia usa el codigo 284, asi que el");
                    println!("     simbolo de longitud tambien se deriva SIEMPRE en este banco.");
                    println!("     ⚠️ «en este banco»: un flujo con 284 existe y hay que saber leerlo.");
                } else {
                    println!("     a UN BIT: solo {} copias usan el 284.", c284);
                }
            } else {
                println!("  ⚠️ NO son derivables del todo. Hay que guardarlos, y la comparacion con");
                println!("     sus 3 bytes no era justa: extraemos mas que ellos.");
            }
        }
    }
    println!();
    println!("  ================= A DONDE SE VA NUESTRO RELOJ =================");
    println!();
    let otros = (seg - seg_leer - seg_inflar - seg_estimar - seg_completo - seg_atajo).max(0.0);
    for (nom, v) in [("leer del disco", seg_leer), ("inflar (el nuestro)", seg_inflar),
                     ("estimar: PARSEAR (lo mismo que nuestro inflate, duplicado)",
                      preflate_rs::PF_PARSE_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9),
                     ("estimar: estimar de verdad",
                      preflate_rs::PF_EST_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9),
                     ("estimar (total, para cuadrar)", seg_estimar),
                     ("simular: la pasada de REFERENCIA (no va a produccion)", seg_completo),
                     ("simular: la que si va a produccion", seg_atajo),
                     ("lo demas", otros)] {
        println!("  {:<54} {:>7.1} s  {:>5.1}%", nom, v, 100.0 * v / seg.max(1e-9));
    }
    println!("  {:<54} {:>7.1} s", "TOTAL de la corrida", seg);
    println!("  {:<54} {:>7.1} s", "lo que costaria en PRODUCCION (sin la referencia)",
             seg - seg_completo);
    {
        use std::sync::atomic::Ordering::Relaxed;
        let (ok, res) = (UN_PARSEO_OK.load(Relaxed), UN_PARSEO_RESPALDO.load(Relaxed));
        if ok + res > 0 {
            println!();
            println!("  UN SOLO PARSEO ({}): {} flujos de {} salen de SU parseo ({:.1}%), {} caen a",
                     "VC_UN_PARSEO=1", ok, ok + res,
                     100.0 * ok as f64 / (ok + res).max(1) as f64, res);
            println!("  nuestro inflate porque el suyo revienta — y esos pagan los dos parseos.");
            println!("  ⚠️ La auditoria de sim_long/sim_dist va APAGADA en este camino: el simbolo");
            println!("     lo derivamos nosotros, asi que compararlo consigo mismo no mide nada.");
        }
    }
    if let Some(ruta) = VOLCADO.as_ref() {
        let mut fuera: Vec<u8> = Vec::new();
        let mut tam = [0usize; NCAT];
        CANALES.with(|c| {
            let c = c.borrow();
            for i in 0..NCAT { tam[i] = c[i].len(); fuera.extend_from_slice(&c[i]); }
        });
        // Y ademas UNO POR CANAL: un compresor modela mejor datos homogeneos que cinco
        // flujos distintos pegados, y saber si eso compensa es una medida de dos minutos.
        CANALES.with(|c| {
            let c = c.borrow();
            for i in 0..NCAT {
                let _ = std::fs::write(format!("{ruta}.{i}"), &c[i]);
            }
        });
        match std::fs::write(ruta, &fuera) {
            Ok(()) => {
                println!();
                println!("  ================= EL VOLCADO DE LAS CORRECCIONES =================");
                println!();
                for i in 0..NCAT {
                    println!("  {:<20} {:>12} B", CAT_NOMBRE[i], tam[i]);
                }
                println!("  {:<20} {:>12} B  ({:.3}% del DEFLATE, EN CRUDO)", "TOTAL", fuera.len(),
                         100.0 * fuera.len() as f64 / deflate_total.max(1) as f64);
                println!();
                println!("  Escrito en {ruta}. Comprimelo con vc-cm y comparalo contra el {:.3}% que",
                         pc(coste_pf(&mejor_total).iter().sum::<f64>()));
                println!("  cobra nuestro modelo: si vc-cm baja de ahi, el techo se atraviesa por el");
                println!("  CODIFICADOR y no hace falta acertar mas.");
            }
            Err(e) => eprintln!("no puedo escribir {ruta}: {e}"),
        }
    }
    println!();
    println!("  ================= EL DINERO, que es quien decide =================");
    println!();
    println!("  {:<26} {:>13} {:>17} {:>12}", "", "% del DEFLATE", "GB guardados/PB", "horas/PB");
    println!("  {:<26} {:>12.3}% {:>17.1} {:>12.1}", "preflate",
             100.0 * PREFLATE_B / BANCO_B, PREFLATE_B * esc_p / 1e9, PREFLATE_S * esc_p / 3600.0);
    if *SALTOS_ON {
        println!("  (el nuestro va con la ESTRUCTURA de preflate y CALIBRADO x{CALIBRA:.3}: su propio");
        println!("   coste cobrado con nuestro contable sale 1,111% donde sus bytes reales son 1,502%)");
    }
    println!("  {:<26} {:>12.3}% {:>17.1} {:>12.1}", "vdeflate fase 3 (hoy)",
             100.0 * nuestros_bytes as f64 / deflate_total.max(1) as f64,
             nuestros_bytes as f64 * esc_n / 1e9, nuestro_s * esc_n / 3600.0);
    println!();
    for (nom, precio) in [("S3", 21.30), ("Backblaze", 5.55), ("bulk", 2.50), ("disco", 1.20)] {
        let eur = |b: f64, s: f64, e: f64| (b * e / 1e12) * precio * MESES
                                          + (s * e / 3600.0) * EUR_H;
        let pf = eur(PREFLATE_B, PREFLATE_S, esc_p);
        let ns = eur(nuestros_bytes as f64, nuestro_s, esc_n);
        println!("  a precio {:<12} preflate {:>12.0} €/PB-año   vdeflate {:>12.0}   {}",
                 nom, pf, ns,
                 if ns < pf { format!("GANAMOS por {:.0}", pf - ns) }
                 else { format!("perdemos por {:.0}", ns - pf) });
    }
    if let Some(f) = &por_flujo {
        let mut t = String::from("# UNA LINEA POR FLUJO, para cruzar con bin/vc-censo-codificadores.\n");
        t.push_str("# La pregunta: ¿nuestro acierto se hunde justo en los flujos que el censo dice\n");
        t.push_str("# que NO son zlib? Si si, la fase 3 se reescribe; si no, el limite esta en otro sitio.\n");
        t.push_str("fichero\tsimbolos\taciertos\tnivel_ganador\tbytes_deflate\n");
        for l in &lineas { t.push_str(l); t.push('\n'); }
        if std::fs::write(f, t).is_err() { eprintln!("no puedo escribir {f}") }
        else { println!("\n  por-flujo: {} lineas en {}", lineas.len(), f) }
    }
    println!();
    println!("  ⚠ preflate REVIENTA con un panic en 171 de los 1.973 flujos y su cifra esta");
    println!("    medida solo sobre los 1.802 que acepta. Nosotros hacemos los 1.973. El rival");
    println!("    tiene el examen mas facil, asi que si perdemos, perdemos de verdad.");
}

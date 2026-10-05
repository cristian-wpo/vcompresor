// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! # vlz — nuestro códec LZ, el que sustituye a zstd en el núcleo
//!
//! ## Por qué existe
//!
//! Orden del dueño, 2026-07-31: «zstd deberíamos estar haciéndolo nosotros pero ya; es el núcleo
//! de nuestro compresor y no debe ser de terceros por ningún motivo».
//!
//! Y hay un motivo técnico que apareció el mismo día: todo lo que descubrimos sobre cómo
//! comprimir mejor —el buscador de largo alcance, el tamaño de ventana, la estrategia— tuvimos
//! que PEDÍRSELO a zstd por su API de parámetros, en vez de cambiarlo. Con 58.395 líneas de C
//! ajenas en el centro del producto, la última palabra sobre nuestro propio compresor no era
//! nuestra.
//!
//! ## Lo que NO hacemos, y es la decisión que lo hace abordable
//!
//! **No somos compatibles con zstd.** El archivo es nuestro, así que no hay que reimplementar su
//! formato ni sus tablas ni su cabecera: hay que hacer el códec que le conviene a NUESTROS datos.
//! Reimplementar zstd sería el proyecto grande e inútil; hacer un LZ propio bien medido es el
//! proyecto pequeño y nuestro.
//!
//! ## Las dos decisiones de diseño, y las dos salen de una medida de hoy
//!
//! **1. Codificador ARITMÉTICO, no Huffman ni FSE.** Medido el 2026-07-31 con `vc-zstd-perfil`:
//! barriendo las nueve estrategias de zstd a nivel 19, el reloj va de 0,049 s a 7,3 s —144
//! veces— y lo único que cambia entre ellas es cuánto se esfuerza el BUSCADOR. O sea que
//! codificar la entropía es menos del 1% del tiempo. zstd usa Huffman + FSE porque necesita
//! descomprimir a gigabytes por segundo; nosotros ya sabemos que ahí no está el coste, así que
//! podemos gastar en un codificador que comprime mejor y no notarlo.
//!
//! **2. Buscador de largo alcance desde el primer día.** Medido el mismo día sobre el corpus
//! real del Mac: una estrategia barata CON buscador de largo alcance comprime un 5,89% MÁS
//! gastando 8,8 veces MENOS que la estrategia cara sin él. En el plano expandido de un PNG pasa
//! lo contrario, así que es una perilla, no un valor fijo — pero tiene que existir.
//!
//! ## Estado
//!
//! **Fase 1**: comprime y descomprime byte a byte. Es lo único que hay hoy, y hasta que la
//! vuelta sea exacta sobre todo lo que le echemos, ningún número de tamaño significa nada.

use std::io::Write;

// ============================== EL CODIFICADOR DE RANGO ==============================
//
// Aritmético binario adaptativo. Cada decisión se codifica como un bit con una probabilidad que
// se ajusta sola según lo que va viendo, así que no hay tablas que transmitir ni que construir:
// el descompresor aprende exactamente lo mismo por el mismo camino.
//
// POR QUE ESTE Y NO HUFFMAN. Huffman no puede gastar menos de un bit por símbolo, y en un flujo
// donde el 97% de las decisiones van hacia el mismo lado eso es tirar bits a puñados. El
// aritmético gasta el logaritmo de la probabilidad, o sea 0,04 bits cuando toca. La cuenta de
// arriba dice que el tiempo que cuesta no se nota.

/// Probabilidad de que el siguiente bit sea 0, en 12 bits (0..4096). Se adapta sola.
#[derive(Clone, Copy)]
struct Prob(u16);

impl Default for Prob { fn default() -> Self { Prob(2048) } }   // a mitad: sin prejuicio

impl Prob {
    /// La adaptación. El 5 es la velocidad: más alto olvida antes, más bajo confía más en lo
    /// visto. Se eligió 5 porque es lo que usan LZMA y vc-cm, y cambiarlo sin medir sería
    /// exactamente lo que este proyecto persigue.
    #[inline(always)]
    fn ajusta(&mut self, bit: u8) {
        if bit == 0 { self.0 += (4096 - self.0) >> 5; }
        else { self.0 -= self.0 >> 5; }
    }
}

struct Codificador { bajo: u64, rango: u32, cache: u8, cache_n: u64, sal: Vec<u8> }

impl Codificador {
    /// `cache_n` arranca en 1 y `cache` en 0: eso hace que el primer byte que salga sea un cero
    /// de relleno, y el descodificador se lo salta. No es un descuido, es como se sincronizan
    /// los dos lados sin mandar nada mas.
    fn new() -> Self { Codificador { bajo: 0, rango: !0, cache: 0, cache_n: 1, sal: Vec::new() } }

    /// EL ACARREO, que es lo unico delicado de un codificador de rango.
    ///
    /// `bajo` tiene 33 bits utiles: los 32 del valor y uno de acarreo encima. Cuando los 32 de
    /// abajo estan por debajo de 0xFF000000 ya no puede haber propagacion hacia atras y se puede
    /// soltar el byte pendiente; si hay acarreo, se suma a el y a los 0xFF que se guardaron
    /// esperando. La primera version comparaba `bajo` ENTERO —acarreo incluido— y recortaba a 32
    /// bits DESPUES de desplazar en vez de ANTES. Las dos cosas se comen el acarreo, y un
    /// acarreo perdido descoloca todo lo que viene detras: la ida y vuelta lo delato en 6 de 8.
    #[inline]
    fn saca(&mut self) {
        if (self.bajo as u32) < 0xFF00_0000 || (self.bajo >> 32) != 0 {
            let acarreo = (self.bajo >> 32) as u8;
            let mut b = self.cache;
            loop {
                self.sal.push(b.wrapping_add(acarreo));
                b = 0xFF;
                self.cache_n -= 1;
                if self.cache_n == 0 { break }
            }
            self.cache = ((self.bajo as u32) >> 24) as u8;
        }
        self.cache_n += 1;
        // TRUNCA A 32 BITS, no crece a 40. Los ocho bits de arriba ACABAN de guardarse en la
        // cache (o son 0xFF y estan contados en `cache_n`), asi que dejarlos dentro de `bajo`
        // los mete otra vez y ademas hace que `bajo >> 32` deje de significar «hay acarreo».
        // Con 40 bits la ida y vuelta fallaba hasta con UN literal suelto.
        self.bajo = ((self.bajo as u32) << 8) as u64;
    }

    #[inline]
    fn bit(&mut self, p: &mut Prob, bit: u8) {
        let lim = (self.rango >> 12) * p.0 as u32;
        if bit == 0 { self.rango = lim; } else { self.bajo += lim as u64; self.rango -= lim; }
        p.ajusta(bit);
        while self.rango < (1 << 24) { self.rango <<= 8; self.saca(); }
    }

    /// Bits sin modelo, para lo que de verdad es equiprobable (los bits bajos de una distancia).
    ///
    /// LOS n DE GOLPE, NO DE UNO EN UNO. La pregunta es del dueño —«¿es obligatorio de bit en
    /// bit? ¿no se puede de a 4?»— y aqui, y SOLO aqui, la respuesta es que si.
    ///
    /// Por que aqui si: estos bits no pasan por ningun `Prob`, asi que no hay nada que
    /// actualizar entre medias, y dividir el rango por dos n veces es dividirlo por 2^n una
    /// sola vez. Los bits CON modelo no se pueden agrupar y ganar nada: un arbol binario de n
    /// niveles con su probabilidad por nodo describe la MISMA distribucion que un alfabeto de
    /// 2^n simbolos —el producto de las condicionales es la conjunta—, asi que juntarlos no
    /// compra bytes y encima adapta peor.
    ///
    /// ⚠️ NO DA LOS MISMOS BYTES que el bucle: el bucle trunca en cada paso (`rango>>1` n veces)
    /// y esto trunca una vez (`rango>>n`), y las dos truncaciones no coinciden. Sigue siendo un
    /// codificador EXACTO —el descodificador hace la particion identica— y la diferencia de
    /// tamaño esta en las centesimas. Lo que cambia de verdad es el numero de operaciones.
    ///
    /// El tope de n es 24: `rango` vale 2^24 como minimo al entrar, asi que `rango >> n` nunca
    /// llega a cero. Quien llama nunca pide mas de 15.
    #[inline]
    fn crudos(&mut self, v: u32, n: u32) {
        debug_assert!(n <= 24 && (n == 0 || v < (1u32 << n)));
        if n == 0 { return }
        self.rango >>= n;
        self.bajo += (v as u64) * self.rango as u64;
        while self.rango < (1 << 24) { self.rango <<= 8; self.saca(); }
    }

    fn cierra(mut self) -> Vec<u8> {
        for _ in 0..5 { self.saca(); }
        self.sal
    }
}

struct Descodificador<'a> { rango: u32, codigo: u32, ent: &'a [u8], i: usize }

impl<'a> Descodificador<'a> {
    fn new(ent: &'a [u8]) -> Self {
        let mut d = Descodificador { rango: !0, codigo: 0, ent, i: 1 };  // el 1: el byte de cache
        for _ in 0..4 { d.codigo = (d.codigo << 8) | d.byte(); }
        d
    }
    #[inline]
    fn byte(&mut self) -> u32 {
        let b = if self.i < self.ent.len() { self.ent[self.i] } else { 0 };
        self.i += 1; b as u32
    }
    #[inline]
    fn bit(&mut self, p: &mut Prob) -> u8 {
        let lim = (self.rango >> 12) * p.0 as u32;
        let bit = if self.codigo < lim { self.rango = lim; 0 }
                  else { self.codigo -= lim; self.rango -= lim; 1 };
        p.ajusta(bit);
        while self.rango < (1 << 24) { self.rango <<= 8; self.codigo = (self.codigo << 8) | self.byte(); }
        bit
    }
    /// El espejo de `Codificador::crudos`: una division en vez de n comparaciones.
    ///
    /// EL TOPE NO ES PARANOIA. El codificador reparte el rango en 2^n trozos de `rango>>n`, y
    /// `rango` casi nunca es multiplo exacto de 2^n: sobra un pico arriba en el que el
    /// codificador NUNCA escribe. Los bytes de cierre del flujo si pueden caer ahi, y sin el
    /// tope la division devolveria 2^n —un valor que no existe— y corromperia lo que viene
    /// detras. Ese pico pertenece al ultimo simbolo.
    #[inline]
    fn crudos(&mut self, n: u32) -> u32 {
        if n == 0 { return 0 }
        self.rango >>= n;
        let v = (self.codigo / self.rango).min((1u32 << n) - 1);
        self.codigo -= v * self.rango;
        while self.rango < (1 << 24) { self.rango <<= 8; self.codigo = (self.codigo << 8) | self.byte(); }
        v
    }
}

// ============================== EL BUSCADOR DE COINCIDENCIAS ==============================
//
// Cadena de hash sobre una ventana, que es lo mismo que hacen zlib y zstd, y lo mismo que ya
// escribimos para medir la fase 3 de vdeflate. Aquí es nuestro y podemos tocarlo.

/// EL TOPE de la ventana. Ya no es LA ventana: la de verdad se dimensiona a la ENTRADA.
///
/// Estaba en 4 MiB fijos —«es una perilla y hay que medirla», decia— y son los MISMOS 4 MiB que
/// el 31-07 se encontraron en zstd: el archivador daba frames de 64 MiB y el codec miraba 4. Aqui
/// pasaba igual, en nuestro propio codigo, con la diferencia de que aqui SI podemos cambiarlo.
///
/// 32 MiB de tope y no mas porque `prev` es un u32 por posicion: 32 Mi x 4 B = 128 MB. Pasar de
/// ahi es pagar memoria por un alcance que los frames no le van a dar nunca.
const VENTANA_TOPE: usize = 1 << 25;

/// La ventana PARA ESTA ENTRADA: la potencia de dos que la cubre, acotada al tope. Un fichero de
/// 5 MB no necesita —ni debe reservar— 128 MB de cadena.
/// ¿El salto sobre los tramos secos crece? `VC_VLZ_SALTO=0` lo apaga.
///
/// Encendido (defecto) es el comportamiento de zstd: tras N bytes sin encontrar nada se avanza
/// mas de uno y NO se indexan las posiciones saltadas. Apagado se busca e indexa en todas.
/// La tabla de lo que cuesta cada opcion esta donde se usa, en `comprime`.
fn salto_crece() -> bool {
    static E: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_VLZ_SALTO").map(|v| v != "0").unwrap_or(true))
}

#[inline]
fn ventana_de(n: usize) -> usize {
    n.next_power_of_two().clamp(1 << 16, VENTANA_TOPE)
}
const CUBOS: usize = 1 << 20;
const LOG_CUBOS: u32 = CUBOS.trailing_zeros();
const MIN_COPIA: usize = 4;              // por debajo de 4 casi nunca compensa

/// CUANTAS DISTANCIAS RECIENTES SE GUARDAN. zstd guarda 3 (`ZSTD_REP_NUM`), LZMA guarda 4.
/// Sale a constante a proposito: la estructura es de ellos, el numero se mide aqui.
const NREPS: usize = 8;

/// LO QUE VALE UN BYTE DE LONGITUD frente a lo que cuesta una distancia entera, en las mismas
/// unidades. De zstd_lazy.c:1635 —`gain2 = mlRep*3` contra `gain1 = ml*3 - log2(off) + 1`—.
/// El 3 es SUYO y esta afinado para FSE + tablas; el nuestro es aritmetico adaptativo, asi que
/// este numero hay que barrerlo, no heredarlo.
///
/// GOBIERNA LAS DOS DECISIONES: que coincidencia elige `busca` y cuando gana un repcode. Se probo
/// separarlo en dos constantes y NO compensa —compra 368 B en codigo y cuesta 330.801 B en sql—:
/// un modelo de coste, uno solo.
///
/// ⚠️ EL 5 ERA UN ARTEFACTO DEL CORPUS DEL BARRIDO. Se afino sobre codigo+csvjson+logs, y sql NO
/// ESTABA DENTRO. Barrido sobre cinco tipos, el optimo es 4 y en sql la diferencia es un ESCALON,
/// no una curva: 3.239.982 B con 4 contra 3.572.586 con 5, un 10,3%. Mecanismo: el criterio
/// deja que un repcode sea hasta `(lg-1)/PESO_LONG` bytes MAS CORTO y aun asi gane; con lg~20 eso
/// son 4 bytes con peso 4 y 3 con peso 5. En sql hay estructura repetida a larga distancia donde
/// el repcode queda justo ~4 bytes por debajo de la mejor coincidencia de la cadena — con peso 5
/// se dejan de coger TODOS y se pagan distancias enteras.
const PESO_LONG: i32 = 4;
const MAX_COPIA: usize = 1 << 16;   // ya no lo limita la codificacion, solo el coste de escanear

// EL INDICE ESPARCIDO DE LARGO ALCANCE: PROBADO Y NO ENVIADO, con su numero.
//
// zstd lleva uno (`zstd_ldm.c`, BSD-3): un SEGUNDO indice que solo muestrea 1 de cada 128
// posiciones —y por CONTENIDO, con un hash rodante gear— y solo acepta coincidencias de 64 bytes
// o mas. La idea es buena: para encontrar una coincidencia larga no hace falta indexar todas las
// posiciones, basta con que UNA caiga muestreada en los dos lados, y con muestreo por contenido
// eso pasa solo. Y el gear ya lo tenemos: es el mismo de vc-core/src/cdc.rs.
//
// SE IMPLEMENTO Y SE MIDIO, y no paga:
//     sobre 84 MB (2,5 veces el tope de ventana)   esf 6: -0,168%   esf 64: -0,007%
//     sobre los corpus de <=32 MB                  esf 6: -0,623%   esf 64: -0,013%
// A cambio de una pasada entera sobre el dato y 4 MB de tabla.
//
// ⚠️ ESTO DECIA «lo refutado es MI version, no la idea: la mia sondea UNA ranura y la de zstd usa
// cubos de 16». ERA UNA COARTADA, y el 01-08 se cayo midiendo SU implementacion —la de verdad,
// con sus cubos de 16— sobre los mismos 135,2 MB:
//
//     zstd -9 --zstd=wlog=25  (32 MiB, SIN su indice largo)     9.405.026 B
//     zstd -9 --long=25       (32 MiB, CON su indice largo)     9.725.733 B   +3,41% PEOR
//
// Encender su indice de largo alcance EMPEORA SU PROPIO RESULTADO en este corpus. O sea que lo
// que no paga aqui es LA IDEA, no mi version de ella. Pregunta cerrada: no se retoma.
//
// Y el porque encaja con lo que ya sabiamos: el indice esparcido esta pensado para copias de
// seguridad y maquinas virtuales, donde hay bloques identicos a gigabytes de distancia. Nuestro
// arbol es codigo fuente, y ahi las repeticiones lejanas ya las coge la ventana de 32 MiB.

#[inline(always)]
fn hash4(d: &[u8], p: usize) -> usize {
    let v = u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]);
    // EL DESPLAZAMIENTO SALE DE CUBOS, no de un 17 escrito a mano. Con el 17 clavado el hash
    // solo podia dar 2^17 valores distintos pasara lo que pasara: subir CUBOS reservaba la
    // memoria y no usaba ni un bit mas. Se veia en que la salida era IDENTICA byte a byte.
    ((v.wrapping_mul(2654435761)) >> (32 - LOG_CUBOS)) as usize & (CUBOS - 1)
}

/// Cuantos bytes seguidos coinciden. DE OCHO EN OCHO, no de uno en uno.
///
/// Es LA funcion caliente: una vez por candidato de la cadena —hasta `esfuerzo` por posicion— y
/// con el paso perezoso hasta tres veces mas. Byte a byte el procesador hace una comparacion y un
/// salto por CADA byte; con u64 hace una por cada OCHO, y cuando fallan, `trailing_zeros() >> 3`
/// dice en cual de los ocho sin volver atras.
///
/// DA EXACTAMENTE EL MISMO RESULTADO: no es un atajo que aproxime, es la misma cuenta con menos
/// instrucciones. Comprobado midiendo — los bytes de salida salen identicos.
///
/// Los limites salen solos: `tope` ya esta acotado a `n - b` por quien llama y `a < b`, asi que
/// leer ocho desde `k + 8 <= tope` no se sale por ningun lado.
#[inline(always)]
fn largo_comun(d: &[u8], a: usize, b: usize, tope: usize) -> usize {
    let mut k = 0;
    while k + 8 <= tope {
        let x = u64::from_le_bytes(d[a + k..a + k + 8].try_into().unwrap());
        let y = u64::from_le_bytes(d[b + k..b + k + 8].try_into().unwrap());
        if x != y { return k + ((x ^ y).trailing_zeros() >> 3) as usize }
        k += 8;
    }
    while k < tope && d[a + k] == d[b + k] { k += 1 }
    k
}

// ============================== EL MODELO ==============================
//
// Lo que decide cuántos bits cuesta cada cosa. Empieza plano y aprende del propio dato.

struct Modelo {
    /// ¿literal o copia? El contexto es si lo anterior fue copia: los literales van en rachas.
    tipo: [Prob; 2],
    /// Literales, orden 1: el byte anterior elige el árbol. 256 contextos x 256 nodos.
    lit: Vec<Prob>,
    /// Longitud de la copia, en árbol binario de 9 niveles. UN ARBOL DE n NIVELES USA LOS
    /// INDICES 1..2^n-1, asi que aqui hacen falta 512 y no 64. La primera version puso 64 y se
    /// salia del array: Rust lo pilla en el acto, en C habria sido corrupcion silenciosa.
    long: [Prob; 512],
    long_log: [Prob; 64],
    /// Cuántos bits tiene la distancia (su logaritmo), que es lo único que vale la pena modelar.
    dist_log: [Prob; 64],
    /// ¿La distancia es una de las recientes? El contexto es si la copia ANTERIOR ya lo fue:
    /// las repeticiones vienen en rachas (una tabla, una columna, un bloque repetido).
    rep_si: [Prob; 2],
    /// Cual de las NREPS, en arbol binario. 2 niveles bastan para 3 y para 4.
    rep_cual: [Prob; 16],
}

impl Modelo {
    fn new() -> Self {
        Modelo { tipo: [Prob::default(); 2], lit: vec![Prob::default(); 256 * 256],
                 long: [Prob::default(); 512], long_log: [Prob::default(); 64], dist_log: [Prob::default(); 64],
                 rep_si: [Prob::default(); 2], rep_cual: [Prob::default(); 16] }
    }
}

/// Escribe un número en árbol binario con `n` niveles, cada nivel con su propia probabilidad.
#[inline]
fn pon_arbol(c: &mut Codificador, ps: &mut [Prob], v: u32, n: u32) {
    let mut ctx = 1usize;
    for i in (0..n).rev() {
        let bit = ((v >> i) & 1) as u8;
        c.bit(&mut ps[ctx], bit);
        ctx = (ctx << 1) | bit as usize;
    }
}

#[inline]
fn saca_arbol(d: &mut Descodificador, ps: &mut [Prob], n: u32) -> u32 {
    let mut ctx = 1usize;
    for _ in 0..n {
        let bit = d.bit(&mut ps[ctx]);
        ctx = (ctx << 1) | bit as usize;
    }
    (ctx as u32) - (1 << n)
}

// ============================== COMPRIMIR ==============================

/// `esfuerzo`: cuántos candidatos de la cadena se miran. Es LA perilla del tiempo — medido en
/// zstd, el 99% del reloj es esto — y por eso está fuera y no escondida dentro.
pub fn comprime(d: &[u8], esfuerzo: usize) -> Vec<u8> {
    let mut c = Codificador::new();
    let mut m = Modelo::new();
    let mut head = vec![u32::MAX; CUBOS];
    let ventana = ventana_de(d.len());
    let mascara = ventana - 1;
    let mut prev = vec![u32::MAX; ventana];
    let n = d.len();
    let mut pos = 0usize;
    let mut previo_copia = 0usize;
    // LAS DISTANCIAS RECIENTES. 0 = ranura sin estrenar.
    let mut reps = [0usize; NREPS];
    let mut previo_rep = 0usize;
    // donde acabo la ultima copia, para saber cuanto llevamos en seco (el `anchor` de zstd)
    let mut ancla = 0usize;
    // la busqueda de pos+1 ya hecha, para no repetirla al aplazar (P4)
    let mut guardada: Option<(usize, usize)> = None;
    let mut pend: Option<(usize, usize)> = None;

    macro_rules! mete {
        ($q:expr) => {{
            let q = $q;
            if q + MIN_COPIA <= n {
                let h = hash4(d, q);
                prev[q & mascara] = head[h];
                head[h] = q as u32;
            }
        }};
    }

    // LA BUSQUEDA, sacada a cierre para poder preguntarla en pos+1 y pos+2. Eso es lo unico que
    // separa a lo codicioso de lo perezoso, y en la escala de zstd son DOS escalones:
    // fast=1, dfast=2, greedy=3, lazy=4, lazy2=5. vlz estaba en greedy y el nivel 9 de zstd
    // -contra el que nos comparamos- es lazy2.
    let busca = |q: usize, head: &Vec<u32>, prev: &Vec<u32>| -> (usize, usize) {
        let tope = MAX_COPIA.min(n - q);
        let (mut ml, mut md) = (0usize, 0usize);
        // la mejor PUNTUACION vista, no la mejor longitud: longitud x peso menos el coste de la
        // distancia. Ver el comentario largo mas abajo.
        let mut mejor = i64::MIN;
        if tope >= MIN_COPIA {
            let mut cur = head[hash4(d, q)];
            let mut pasos = 0;
            while cur != u32::MAX && pasos < esfuerzo {
                let cp = cur as usize;
                let dd = q - cp;
                if dd == 0 || dd > ventana { break }
                // P2 DEL CAMINO — ¿cuantas veces leemos el mismo byte? Antes, cada candidato
                // se comparaba DESDE EL PRINCIPIO aunque no tuviera ninguna posibilidad. Si el
                // byte que esta en la longitud que YA llevamos no coincide, ese candidato no
                // puede superarla: se descarta con UNA comparacion en vez de con `ml`. Es la
                // criba que hace que la cadena se pueda recorrer entera sin que cueste.
                // LA CRIBA, leida de zstd_lazy.c:715 y es mejor que la que teniamos:
                //     if (MEM_read32(match + ml - 3) == MEM_read32(ip + ml - 3))  /* potentially better */
                // Nosotros comparabamos UN byte en la longitud actual. Ellos comparan los CUATRO
                // que terminan ahi, en una sola carga de u32: el mismo coste -una carga, una
                // comparacion- y un filtro cuatro veces mas fuerte. Un candidato tiene que
                // reproducir los cuatro ultimos bytes de la mejor coincidencia para merecer que
                // se le compare entero.
                if ml >= 4 && q + ml < n {
                    let a = u32::from_le_bytes(d[cp + ml - 3..cp + ml + 1].try_into().unwrap());
                    let b = u32::from_le_bytes(d[q + ml - 3..q + ml + 1].try_into().unwrap());
                    if a != b { cur = prev[cp & mascara]; pasos += 1; continue }
                } else if ml > 0 && d[cp + ml] != d[q + ml] {
                    cur = prev[cp & mascara]; pasos += 1; continue
                }
                let l = largo_comun(d, cp, q, tope);
                // ELEGIR POR COSTE, NO POR LONGITUD. Antes era `if l > ml`, o sea cualquier
                // coincidencia estrictamente mas larga SIN MIRAR A QUE DISTANCIA. Una de 5 bytes
                // a 3.000.000 cuesta ~44 bits; una de 4 a distancia 100 cuesta ~14. En texto la
                // larga gana igual y no se notaba; en datos casi incompresibles TODAS son cortas,
                // manda la distancia, y cada lejana «mejor por un byte» era una perdida neta.
                // Lo caza el banco POR TIPO: jpg y png-normal daban MAS bytes a esfuerzo 64 que a
                // esfuerzo 6, que es imposible si la eleccion es correcta.
                // La forma es la de zstd_lazy.c:1657 -`gain2 = ml2*4 - highbit32(ofbCandidate)`-
                // y el peso es el MISMO que ya usaban los repcodes: un modelo de coste, uno solo.
                if l >= MIN_COPIA {
                    let lg = 32 - ((dd - 1) as u32).leading_zeros();
                    let punt = l as i64 * PESO_LONG as i64 - lg as i64;
                    // `>` y no `>=`: en empate gana el que se vio ANTES, y la cadena va de mas
                    // reciente a mas viejo, o sea el mas CERCANO. Que es el que menos cuesta.
                    if punt > mejor { mejor = punt; ml = l; md = dd; if l >= tope { break } }
                }
                cur = prev[cp & mascara];
                pasos += 1;
            }
        }
        (ml, md)
    };

    // PROBAR LAS DISTANCIAS RECIENTES. Esto NO recorre ninguna cadena: son NREPS posiciones
    // conocidas y una comparacion de 8 en 8 en cada una. Cuesta lo que cuestan tres candidatos
    // de los `esfuerzo` que ya se miran, y encuentra lo mas barato de codificar que hay.
    let prueba_reps = |q: usize, reps: &[usize; NREPS]| -> (usize, usize) {
        let tope = MAX_COPIA.min(n - q);
        let (mut rml, mut ridx) = (0usize, 0usize);
        if tope >= MIN_COPIA {
            for (i, &r) in reps.iter().enumerate() {
                if r == 0 || r > q { continue }
                let l = largo_comun(d, q - r, q, tope);
                if l > rml { rml = l; ridx = i; }
            }
        }
        (rml, ridx)
    };

    while pos < n {
        let tope = MAX_COPIA.min(n - pos);
        // P4 DEL CAMINO, LA MITAD QUE FALTABA. Se guardaba la busqueda de pos+1 al aplazar —eso
        // ya estaba— pero la vuelta siguiente hacia `busca(pos)` IGUAL y acto seguido la pisaba
        // entera con la guardada. O sea que se ahorraba la busqueda y luego se pagaba de todas
        // formas. Ahora solo se busca si no hay nada guardado.
        let (mut ml, mut md) = match guardada.take() {
            Some((m, dd)) => (m, dd),
            None => busca(pos, &head, &prev),
        };

        // EL PASO PEREZOSO. Lo codicioso coge la primera coincidencia que encuentra; lo perezoso
        // pregunta si un byte mas alla hay una MEJOR, y si la hay emite un literal y se espera.
        //
        // El margen es +1 y no 0 a proposito: aplazar cuesta un literal, asi que una coincidencia
        // que solo empata no compensa. Se mira a +1 y a +2 -eso es «lazy2»- y en cuanto una gana
        // se corta: si la de +1 ya es mejor, no hace falta preguntar por la de +2.
        // P4 DEL CAMINO — ¿tiramos algo que ya habiamos calculado? Se tiraba. El paso perezoso
        // buscaba en pos+1 y pos+2, y al aplazar avanzaba UNO y volvia a buscar pos+1 DESDE CERO.
        // Dos tercios de las busquedas iban a la basura. Ahora la de pos+1 se guarda: si se
        // aplaza, la siguiente vuelta la encuentra hecha.

        // ¿GANA UN REPCODE? La forma del criterio es de zstd (zstd_lazy.c:1635): un byte de
        // longitud vale PESO_LONG y una distancia entera cuesta su logaritmo. Un repcode puede
        // ser MAS CORTO que la mejor coincidencia y aun asi salir mas barato, porque no paga
        // distancia. El caso frecuente es el empate —la misma distancia otra vez— y ahi gana
        // siempre, que es justo lo que se estaba tirando.
        let mut rep_usado: Option<usize> = None;
        if ml < tope {
            let (rml, ridx) = prueba_reps(pos, &reps);
            if rml >= MIN_COPIA {
                let lg_md = if md > 1 { 32 - ((md - 1) as u32).leading_zeros() } else { 0 } as i32;
                let gana = if ml >= MIN_COPIA {
                    rml as i32 * PESO_LONG > ml as i32 * PESO_LONG - lg_md + 1
                } else { true };
                if gana { ml = rml; md = reps[ridx]; rep_usado = Some(ridx); }
            }
        }

        // ⚠️ EL REPCODE VA ANTES DEL PEREZOSO, Y EL ORDEN NO ES ESTETICO. Puesto despues, pisaba
        // el aplazamiento: el perezoso aplaza poniendo ml=0 y dejando en `guardada` la busqueda
        // de pos+1; el repcode veia ese ml=0, ganaba «por defecto» y emitia una copia — con
        // `guardada` todavia puesta, que la vuelta siguiente aplicaba en la posicion equivocada.
        // El flujo salia corrupto y el descodificador acababa leyendo un logaritmo de distancia
        // imposible (division por cero en `crudos`). Asi el repcode es UN CANDIDATO MAS y el
        // perezoso decide sobre el mejor de los dos, que es el orden de zstd.
        let mut aplaza = false;
        if ml >= MIN_COPIA && ml < tope && pos + 1 < n {
            for salto in 1..=2usize {
                if pos + salto >= n { break }
                let (m2, d2) = busca(pos + salto, &head, &prev);
                if salto == 1 { pend = Some((m2, d2)); }
                if m2 > ml + salto { aplaza = true; break }
            }
        }
        if aplaza { ml = 0; md = 0; rep_usado = None; guardada = pend.take(); } else { pend = None; }

        if ml >= MIN_COPIA {
            c.bit(&mut m.tipo[previo_copia], 1);
            // LONGITUD: arbol plano para lo corto, ESCAPE para lo largo.
            //
            // El arbol plano de 9 bits topaba la copia en 515, asi que una repeticion de 10 KB
            // habia que partirla en veinte. La distancia, dos lineas mas abajo, lleva desde el
            // primer dia el esquema que hacia falta —logaritmo modelado + bits bajos crudos— y a
            // la longitud no se le habia aplicado. zstd hace exactamente eso (MaxML=52 + hasta 16
            // bits extra, y llega a 131.074): no habia que copiarlo, habia que mirar dos lineas
            // mas abajo en nuestro propio fichero.
            //
            // PERO NO ENTERO, y esto es lo que la medida enseño: pasar TODA la longitud a
            // logaritmo compra en csvjson (-9,2%) y logs (-4,6%) y PIERDE en sql (+1,0%), porque
            // ahi las longitudes son cortas y concentradas y los bits crudos van SIN MODELAR
            // mientras que el arbol plano los aprende uno a uno. Con el escape se quedan las dos
            // cosas: el arbol sigue mandando en 0..510, que es donde ganaba, y el 511 dice «viene
            // un logaritmo». Se paga un simbolo de mas solo en las copias largas, que son las que
            // mas bytes ahorran. Asi sale monotono: mejora los CUATRO tipos a los dos esfuerzos.
            let lm = (ml - MIN_COPIA) as u32;
            if lm < 511 {
                pon_arbol(&mut c, &mut m.long, lm, 9);
            } else {
                pon_arbol(&mut c, &mut m.long, 511, 9);
                let e = lm - 511;
                let llg = 32 - e.leading_zeros();
                pon_arbol(&mut c, &mut m.long_log, llg, 6);
                if llg > 1 { c.crudos(e & ((1 << (llg - 1)) - 1), llg - 1); }
            }
            // LA DISTANCIA: primero «¿es una de las recientes?», que es UN bit modelado.
            match rep_usado {
                Some(i) => {
                    c.bit(&mut m.rep_si[previo_rep], 1);
                    pon_arbol(&mut c, &mut m.rep_cual, i as u32, 3);
                    // la usada pasa al frente y las de delante bajan un puesto (igual que zstd
                    // en zstd_lazy.c:1719): lo mas reciente es lo mas probable.
                    let r = reps[i];
                    for k in (1..=i).rev() { reps[k] = reps[k - 1]; }
                    reps[0] = r;
                    previo_rep = 1;
                }
                None => {
                    c.bit(&mut m.rep_si[previo_rep], 0);
                    // distancia: su logaritmo va modelado (es lo que se repite) y los bits bajos crudos
                    let dm1 = (md - 1) as u32;
                    let lg = 32 - dm1.leading_zeros();          // 0 si dm1==0
                    pon_arbol(&mut c, &mut m.dist_log, lg, 6);
                    if lg > 1 { c.crudos(dm1 & ((1 << (lg - 1)) - 1), lg - 1); }
                    for k in (1..NREPS).rev() { reps[k] = reps[k - 1]; }
                    reps[0] = md;
                    previo_rep = 0;
                }
            }
            for k in 0..ml { mete!(pos + k); }
            pos += ml;
            ancla = pos;
            previo_copia = 1;
        } else {
            // EL SALTO QUE CRECE, leido de zstd_lazy.c:1614:
            //     size_t const step = ((size_t)(ip-anchor) >> kSearchStrength) + 1;
            //     /* jump faster over incompressible sections */
            // Nosotros avanzabamos de UNO EN UNO siempre, asi que en un JPEG se buscaba en cada
            // posicion para no encontrar nada nunca. Ellos avanzan mas cuanto mas llevan sin
            // encontrar: tras 256 bytes secos el salto es 2, tras 512 es 3, y asi.
            //
            // Los literales que se saltan HAY QUE EMITIRLOS igual —el descompresor necesita los
            // bytes—; lo que se ahorra es BUSCAR en esas posiciones, que es lo caro. Y tampoco se
            // indexan: es lo que ellos llaman `lazySkipping`, y por eso el salto se guarda.
            // ⚠️ EL SALTO ES 1 SI VENIMOS DE APLAZAR. El paso perezoso aplaza poniendo ml=0,
            // que cae aqui — y la busqueda que dejo guardada es la de pos+1, no la de pos+salto.
            // Saltar mas de uno la aplicaba en la posicion equivocada y salia un puntero a otro
            // sitio: flujo corrupto. Lo caza el banco -«NO DESCOMPRIME»- pero solo en codigo, que
            // es donde hay rachas largas sin coincidencia Y coincidencias que aplazar.
            // ⚠️ Y ES UNA PERILLA, NO UNA OPTIMIZACION, aunque se copio como si lo fuera.
            // MEDIDO el 01-08 (esfuerzo 6, mediana de 3 rondas alternando, ronda 1 descartada):
            //
            //     muestra        con salto       sin salto      bytes    MB/s con   MB/s sin
            //     codigo         6.886.153       6.886.152    -0,000%      32,5       32,5
            //     logs           1.369.860       1.369.860    +0,000%     108,4      111,4
            //     png              165.451         164.097    -0,818%      11,6        9,7
            //     png-normal       753.312         749.819    -0,464%      12,6        9,8
            //     video        32.839.286      32.518.161    -0,978%       4,2        2,4
            //
            // EN DATOS COMPRIMIBLES NO HACE NADA: ni bytes ni reloj. Tiene sentido —ahi se
            // encuentra coincidencia casi siempre, el ancla se reinicia y el salto nunca crece—.
            // EN INCOMPRESIBLES compra 1,75x de reloj y cuesta ~1% de bytes. Ese es todo su
            // efecto, y no estaba medido: se cogio de zstd por velocidad y punto.
            //
            // 🔑 Y EXPLICA LA NO-MONOTONIA: sin salto, png-normal da 812.909 B a esfuerzo 6 y
            // 812.780 a esfuerzo 64 —monotono—; con salto da 816.522 y 816.578 —al reves—. Como
            // las posiciones saltadas NO SE INDEXAN, mas esfuerzo NO da un superconjunto de
            // candidatos: cambia CUALES hay. Asi que la no-monotonia no es un fallo que arreglar,
            // es el precio del atajo. zstd la tiene igual.
            //
            // Se deja ENCENDIDO por defecto y con interruptor: apagarlo es cambiar el punto de
            // operacion del producto, y eso lo decide el dueño con la tabla de arriba delante.
            let salto = if guardada.is_some() || !salto_crece() { 1 }
                        else { ((pos - ancla) >> 8) + 1 };
            for k in 0..salto.min(n - pos) {
                c.bit(&mut m.tipo[previo_copia], 0);
                let ctx = if pos + k > 0 { d[pos + k - 1] as usize } else { 0 };
                pon_arbol(&mut c, &mut m.lit[ctx * 256..(ctx + 1) * 256], d[pos + k] as u32, 8);
                // solo se indexa la posicion que SE HA BUSCADO, no las que se saltan
                if k == 0 { mete!(pos + k); }
                previo_copia = 0;
            }
            pos += salto.min(n - pos);
        }
    }

    // Cabecera: el tamaño original, para saber cuándo parar al descomprimir.
    let cuerpo = c.cierra();
    let mut out = Vec::with_capacity(cuerpo.len() + 9);
    out.extend_from_slice(b"VLZ1");
    let _ = out.write_all(&(n as u64).to_le_bytes());
    out.extend_from_slice(&cuerpo);
    out
}

// ============================== DESCOMPRIMIR ==============================

pub fn descomprime(blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < 12 || &blob[..4] != b"VLZ1" { return Err("no es un flujo vlz".into()) }
    let n = u64::from_le_bytes(blob[4..12].try_into().unwrap()) as usize;
    let mut d = Descodificador::new(&blob[12..]);
    let mut m = Modelo::new();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut previo_copia = 0usize;
    let mut reps = [0usize; NREPS];
    let mut previo_rep = 0usize;

    while out.len() < n {
        if d.bit(&mut m.tipo[previo_copia]) == 1 {
            let lm0 = saca_arbol(&mut d, &mut m.long, 9);
            let lm = if lm0 < 511 { lm0 } else {
                let llg = saca_arbol(&mut d, &mut m.long_log, 6);
                let e = if llg > 1 { (1u32 << (llg - 1)) | d.crudos(llg - 1) }
                        else if llg == 1 { 1 } else { 0 };
                511 + e
            };
            let ml = lm as usize + MIN_COPIA;
            let md = if d.bit(&mut m.rep_si[previo_rep]) == 1 {
                let i = saca_arbol(&mut d, &mut m.rep_cual, 3) as usize;
                if i >= NREPS || reps[i] == 0 { return Err("flujo vlz corrupto: repcode invalido".into()) }
                let r = reps[i];
                for k in (1..=i).rev() { reps[k] = reps[k - 1]; }
                reps[0] = r;
                previo_rep = 1;
                r
            } else {
                let lg = saca_arbol(&mut d, &mut m.dist_log, 6);
                let dm1 = if lg > 1 { (1u32 << (lg - 1)) | d.crudos(lg - 1) }
                          else if lg == 1 { 1 } else { 0 };
                let md = dm1 as usize + 1;
                for k in (1..NREPS).rev() { reps[k] = reps[k - 1]; }
                reps[0] = md;
                previo_rep = 0;
                md
            };
            if md > out.len() || ml > n - out.len() { return Err("flujo vlz corrupto".into()) }
            let ini = out.len() - md;
            // Las copias pueden SOLAPARSE: distancia 1 y longitud 100 es cien veces el mismo
            // byte. Es la misma regla que en DEFLATE y hay que copiarla de una en una, o con el
            // atajo del memset cuando la distancia es 1 (que es el caso frecuente: medido en el
            // banco de vdeflate, el 72,1% de los bytes copiados).
            if md == 1 {
                let b = out[ini];
                out.resize(out.len() + ml, b);
            } else if md >= ml {
                out.extend_from_within(ini..ini + ml);
            } else {
                for k in 0..ml { let v = out[ini + k]; out.push(v); }
            }
            previo_copia = 1;
        } else {
            let ctx = if out.is_empty() { 0 } else { *out.last().unwrap() as usize };
            let b = saca_arbol(&mut d, &mut m.lit[ctx * 256..(ctx + 1) * 256], 8) as u8;
            out.push(b);
            previo_copia = 0;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// LA PRUEBA QUE MANDA: lo que entra tiene que salir IGUAL. Mientras esto no pase sobre todo
    /// lo que le echemos, ningún número de tamaño significa nada.
    fn ida_y_vuelta(d: &[u8]) {
        for esf in [1usize, 8, 64] {
            let z = comprime(d, esf);
            let v = descomprime(&z).expect("no descomprime");
            assert_eq!(v.len(), d.len(), "longitud distinta con esfuerzo {esf}");
            assert!(v == d, "contenido distinto con esfuerzo {esf}");
        }
    }

    #[test] fn vacio() { ida_y_vuelta(b""); }
    #[test] fn un_byte() { ida_y_vuelta(b"a"); }
    #[test] fn sin_repeticion() { ida_y_vuelta(b"abcdefghijklmnopqrstuvwxyz"); }
    #[test] fn todo_igual() { ida_y_vuelta(&vec![0x41u8; 100_000]); }

    #[test]
    fn copias_solapadas() {
        // distancia 1 y longitud larga: el caso que rompe una copia hecha con memcpy
        let mut v = b"x".to_vec();
        v.extend(std::iter::repeat(b'y').take(5000));
        ida_y_vuelta(&v);
    }

    #[test]
    fn texto_repetitivo() {
        let mut v = Vec::new();
        for i in 0..5000 { v.extend_from_slice(format!("linea {} de un fichero de texto\n", i % 97).as_bytes()); }
        ida_y_vuelta(&v);
    }

    #[test]
    fn ruido() {
        // Incompresible: aquí lo que se comprueba es que no se rompe ni se dispara de tamaño.
        let mut x = 123456789u64;
        let v: Vec<u8> = (0..200_000).map(|_| {
            x ^= x << 13; x ^= x >> 7; x ^= x << 17; (x & 0xFF) as u8
        }).collect();
        ida_y_vuelta(&v);
    }

    #[test]
    fn no_acepta_basura() {
        assert!(descomprime(b"").is_err());
        assert!(descomprime(b"NOPE12345678").is_err());
    }
}

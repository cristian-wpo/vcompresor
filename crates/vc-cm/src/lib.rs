// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! vc-cm — compresor de mezcla de contextos PROPIO, entero y determinista. Cero dependencias.
//!
//! 8 modelos (órdenes 0-5 + palabra + match) -> ICM con historia de bits -> cadena ISSE ->
//! mezclador logístico entero de DOS CAPAS -> APM/SSE -> codificador aritmético binario de 32
//! bits. Al no haber coma flotante en el camino de la probabilidad, la salida es BYTE-IDÉNTICA
//! en x86 y ARM (probado).
//!
//! Además de binario suelto, es la librería que el archivador usa como códec de máximo ratio
//! del carril texto.


// ---------- historia de bits (ICM) ----------
// En vez de guardar una probabilidad por contexto —que necesita muchas observaciones para
// afinarse— se guarda un ESTADO de 8 bits que resume la historia vista en ese contexto:
// cuántos ceros, cuántos unos y cuál mandó al final. La traducción estado -> probabilidad la
// aprende un StateMap COMPARTIDO por todos los contextos del modelo, así que un contexto
// recién visto YA hereda una estimación buena en vez de partir de 0,5.
//
// Eso es exactamente lo que separa a lpaq/zpaq de un modelo de contexto directo, y lo que
// medimos que nos faltaba: zpaq -m5 nos sacaba un 32% con todo lo demás igualado.

/// El conteo menor limita al mayor: así toda la historia cabe en 8 bits. Un contexto que ha
/// visto 40 ceros y ningún uno es útil; uno que ha visto 40 y 40 no aporta nada que no
/// aporten 7 y 7, así que no se le gasta espacio de estados.
///
/// ⚠️ VENIA HEREDADA DE lpaq —[41,40,12,6,5,4,4,3]— y nunca se habia re-medido aqui, que es
/// justo la clase que bench/constantes.tsv marca como peligrosa. Al ponerla en la mesa salieron
/// dos cosas que nadie habia mirado, y luego una tercera al barrerla:
///
///   1. NO SON 241 ESTADOS, SON 193. El comentario de abajo decia 241 y llevaba equivocado desde
///      que se escribio. Nadie los conto: se copio la cifra junto con la tabla.
///
///   2. LIM[5], LIM[6] Y LIM[7] SON INALCANZABLES. Para que exista un estado con conteo menor 5
///      hace falta que el mayor llegue a 5, y LIM[5] vale 4. Lo mismo con 6 y 7. Tres de las ocho
///      entradas no hacen absolutamente nada: el minimo real nunca pasa de 4.
///
/// Y de ahi la oportunidad: el estado ocupa UN BYTE, o sea 256 sitios, y usamos 193. Sobran 63
/// estados que YA ESTAMOS PAGANDO en memoria y no gastamos. Ampliar la tabla no cuesta ni un byte
/// mas por ranura —cuesta resolucion en el StateMap, que es otra cosa y hay que medirla—.
///
/// `VC_LIM_FORMA=41,40,12,8,7,6,5,4` la cambia para barrerla. Lo que gane se vuelve a fijar como
/// constante: la forma de la tabla decide los bytes de salida, asi que NO puede ser una perilla
/// del producto (clase DETERMINISMO) — un archivo tiene que descomprimirse igual en toda maquina.
static LIM: std::sync::LazyLock<[u8; 8]> = std::sync::LazyLock::new(|| {
    // ⚠️ EL DEFECTO YA NO ES EL DE lpaq. Barrida el 01-08 sobre SEIS tipos y gana en los seis:
    //   codigo -0,115% · logs -0,237% · csvjson -0,187% · sql -0,109% · bin -0,011% · pdf -0,006%
    // Y sale gratis: 231 estados de 256 contra 193, el estado sigue ocupando un byte, y en codigo
    // cuesta 125,55 Gciclos contra 126,47 de la anterior —o sea que ademas es mas barata—.
    //
    // 🔑 LO QUE ENSENA, que vale mas que el -0,025% ponderado: LA RESOLUCION HACE FALTA EN LA
    // COLA, NO EN LA CABEZA. Esta forma gasta sus estados nuevos en contextos que han visto LOS
    // DOS bits; la variante que los gastaba en rachas mas largas de un solo bit ([56,54,...],
    // 251 estados, la que MAS tiene) es la UNICA de las seis que pierde. O sea que 41 ceros
    // seguidos ya bastan para saber lo que hay que saber, y en cambio distinguir «vi 12 y 3» de
    // «vi 18 y 3» si aporta. Llenar el byte no es el objetivo: importa DONDE se gasta.
    let d = [41u8, 40, 18, 12, 9, 7, 6, 5];
    match std::env::var("VC_LIM_FORMA") {
        Err(_) => d,
        Ok(v) => {
            let p: Vec<u8> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if p.len() == 8 { [p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]] } else { d }
        }
    }
});

fn valido(a: u8, b: u8) -> bool {
    let (min, max) = if a <= b { (a, b) } else { (b, a) };
    min <= 7 && max <= LIM[min as usize]
}

/// Al observar un bit, el conteo CONTRARIO se descuenta en vez de quedarse quieto. Es lo que
/// hace al modelo no estacionario: si la fuente cambia, el contexto reacciona en un par de
/// observaciones en vez de arrastrar para siempre lo que vio al principio.
fn descuento(x: u8) -> u8 { if x > 2 { 2 + (x - 2) / 2 } else { x } }

fn acota(a: u8, b: u8) -> (u8, u8) {
    if a <= b {
        let a = a.min(7);
        (a, b.min(LIM[a as usize]))
    } else {
        let b = b.min(7);
        (a.min(LIM[b as usize]), b)
    }
}

/// Tabla de transiciones: estado x bit -> estado. Se genera enumerando los pares (n0,n1)
/// válidos. Con la LIM heredada salen 193 —no 241, como decia este comentario desde que se
/// escribio— de los 256 que caben en el byte. El aviso de abajo cuenta cuantos son de verdad.
struct Historia { nex: [[u8; 2]; 256], n0: [u8; 256], n1: [u8; 256] }

impl Historia {
    fn new() -> Historia {
        let mut pares: Vec<(u8, u8)> = Vec::new();
        let mut idx = vec![255u8; 64 * 64];
        for a in 0..64u8 {
            for b in 0..64u8 {
                if valido(a, b) {
                    assert!(pares.len() < 256, "no caben los estados en un byte");
                    idx[a as usize * 64 + b as usize] = pares.len() as u8;
                    pares.push((a, b));
                }
            }
        }
        // Que lo diga el codigo y no un comentario: una cifra escrita a mano se queda vieja.
        if std::env::var("VC_LIM_FORMA").is_ok() {
            eprintln!("  vc-cm: LIM={:?} -> {} estados de 256", *LIM, pares.len());
        }
        let mut h = Historia { nex: [[0; 2]; 256], n0: [0; 256], n1: [0; 256] };
        for (e, &(a, b)) in pares.iter().enumerate() {
            h.n0[e] = a;
            h.n1[e] = b;
            let (x, y) = acota(a + 1, descuento(b)); // bit 0
            h.nex[e][0] = idx[x as usize * 64 + y as usize];
            let (x, y) = acota(descuento(a), b + 1); // bit 1
            h.nex[e][1] = idx[x as usize * 64 + y as usize];
        }
        h
    }
    /// Prior del estado: la proporción que sugieren sus propios conteos. Arrancar el
    /// StateMap aquí en vez de en 0,5 le ahorra al modelo miles de observaciones.
    fn prior(&self, e: usize) -> u16 {
        let (a, b) = (self.n0[e] as u32, self.n1[e] as u32);
        (((2 * b + 1) * 65535) / (2 * (a + b) + 2)) as u16
    }
}

// ---------- tablas stretch/squash (dominio logístico entero) ----------
// squash(x) = 4096/(1+e^{-x/256}), x en [-2047,2047] -> p en [0,4095]
// stretch = inversa. Ambas precomputadas de forma determinista (sin float en runtime del códec:
// se generan una vez al arrancar con la MISMA fórmula en toda máquina; el resultado es una tabla
// de enteros idéntica en cualquier CPU porque redondeamos explícitamente).
struct Tables { squash: Vec<i32>, stretch: Vec<i16>, h: Historia }
impl Tables {
    fn new() -> Self {
        // squash por tabla de anclas + interpolación entera (la receta de lpaq, 100% reproducible)
        let t: [i32; 33] = [1,2,3,6,10,16,27,45,73,120,194,310,488,747,1101,1546,
            2047,2549,2994,3348,3607,3785,3901,3975,4022,4050,4068,4079,4085,4089,4092,4093,4094];
        let mut squash = vec![0i32; 4096]; // índice = x+2048, x en [-2048,2047]
        for i in 0..4096 {
            let x = i as i32 - 2048;
            if x <= -2047 { squash[i] = 1; continue; }
            if x >= 2047 { squash[i] = 4095; continue; }
            let w = x & 127;
            let idx = ((x >> 7) + 16) as usize;
            squash[i] = (t[idx] * (128 - w) + t[idx + 1] * w + 64) >> 7;
        }
        // stretch = inversa monótona de squash
        let mut stretch = vec![0i16; 4096];
        let mut pi = 0usize;
        for x in -2047..=2047i32 {
            let p = squash[(x + 2048) as usize] as usize;
            while pi <= p { stretch[pi] = x as i16; pi += 1; }
        }
        while pi < 4096 { stretch[pi] = 2047; pi += 1; }
        Tables { squash, stretch, h: Historia::new() }
    }
    #[inline] fn squash(&self, x: i32) -> i32 { let x = x.clamp(-2047, 2047); self.squash[(x + 2048) as usize] }
    #[inline] fn stretch(&self, p: i32) -> i32 { self.stretch[p.clamp(0, 4095) as usize] as i32 }
}


// ---------- DMC: compresión de Markov dinámica ----------
// Todo lo demás en este fichero mira el dato de la misma manera: coge N bytes de contexto,
// los hashea, y busca en una tabla. El DMC no. Es una MÁQUINA DE ESTADOS que empieza pequeña
// y se va partiendo sola: cuando una transición se usa mucho, el estado de destino se CLONA
// para que ese camino tenga estadística propia. Nadie le dice cuántos bytes de contexto usar
// — la profundidad la decide el dato, y sale distinta en cada sitio del fichero.
//
// Por eso suma aunque ya haya doce modelos de contexto: no es más de lo mismo, es otra forma
// de mirar. Lo que un orden fijo no puede es tener memoria larga aquí y corta ahí.

const DMC_NODOS: usize = 1 << 22;   // 4M estados ~ 48 MB

/// ⚠️ LAS DOS CONSTANTES **DEL** DMC, Y NUNCA SE HAN BARRIDO.
///
/// Son las que deciden CUANDO se clona un estado, o sea todo el comportamiento del modelo: con
/// umbrales bajos el arbol crece deprisa y se queda sin nodos antes de tiempo; con umbrales
/// altos casi no crece y el DMC no aporta nada que no aporte un orden fijo. El 64 y el 128 son
/// los de la literatura clasica de DMC y aqui se copiaron sin re-medir — clase «A OJO» en
/// bench/constantes.tsv, que es donde salieron listadas.
///
/// `VC_DMC_U1` y `VC_DMC_U2` las mueven para barrerlas. Lo que gane vuelve a ser constante: los
/// dos deciden los bytes de salida.
/// ⚠️ EL DEFECTO YA NO ES EL DE LA LITERATURA (64/128). Barrido el 01-08 en once configuraciones
/// y verificado en seis tipos: 4/8 gana -0,121% sobre el total y -0,703% en sql.
///
/// 🔑 LO IMPORTANTE DEL BARRIDO NO ES EL NUMERO, ES QUE LA CURVA SE DA LA VUELTA. Bajando desde
/// 128/256 mejora de forma monotona hasta 4/8 y en 2/4 vuelve a subir en sql (480.559 -> 480.938).
/// Sin ese punto de retorno el barrido no habria contestado nada: habria dicho «mas abajo es
/// mejor», que es lo que decia el barrido de tallas del CDC por probar solo hacia un lado.
/// Y tiene explicacion fisica: con umbrales muy bajos el arbol clona sin parar y se queda sin
/// nodos (DMC_NODOS = 4M) antes de acabar el fichero, asi que el ultimo tramo se modela con un
/// arbol congelado.
///
/// ⚠️ Y lo que NO se esconde: 4/8 PIERDE en logs (+0,163%), donde el DMC no aporta nada de todas
/// formas. Y sobre el total de los seis, 2/4 saca 653 bytes de 10,6 millones — un empate. Se elige
/// 4/8 porque gana en cuatro de los seis tipos y en el de mas peso, y porque 2/4 vuelve a caer en
/// el BORDE del barrido: elegir un punto interior que gana ancho es mas defendible que uno del
/// extremo que gana por ruido.
static DMC_UMBRAL: std::sync::LazyLock<u16> = std::sync::LazyLock::new(||
    std::env::var("VC_DMC_U1").ok().and_then(|v| v.parse().ok()).unwrap_or(4));
static DMC_UMBRAL2: std::sync::LazyLock<u16> = std::sync::LazyLock::new(||
    std::env::var("VC_DMC_U2").ok().and_then(|v| v.parse().ok()).unwrap_or(8));

struct Dmc {
    nx: Vec<[u32; 2]>,   // a dónde va cada estado con bit 0 y con bit 1
    c: Vec<[u16; 2]>,    // cuántas veces se ha ido por cada lado
    t: u32,              // estado actual
    n: usize,            // estados en uso
}

impl Dmc {
    fn new() -> Dmc {
        let mut d = Dmc { nx: vec![[0; 2]; DMC_NODOS], c: vec![[0; 2]; DMC_NODOS], t: 0, n: 0 };
        d.arranca();
        d
    }
    /// Arranca con el árbol de un byte: 255 nodos que representan los bits vistos del byte en
    /// curso, y cuyas hojas vuelven a la raíz. Empezar de cero con UN estado funciona, pero
    /// tarda muchísimo en aprender lo que ya sabemos: que los datos van por bytes.
    fn arranca(&mut self) {
        self.n = 255;
        for i in 0..255usize {
            let (a, b) = (2 * i + 1, 2 * i + 2);
            self.nx[i] = if b < 255 { [a as u32, b as u32] } else { [0, 0] };
            self.c[i] = [0, 0];
        }
        self.t = 0;
    }
    #[inline]
    fn total(&self, i: usize) -> u32 { self.c[i][0] as u32 + self.c[i][1] as u32 }
    /// P(bit=1) en 12 bits.
    #[inline]
    fn p(&self) -> i32 {
        let i = self.t as usize;
        let (a, b) = (self.c[i][0] as u32, self.c[i][1] as u32);
        (((2 * b + 1) * 4096) / (2 * (a + b) + 2)).clamp(1, 4095) as i32
    }
    fn update(&mut self, bit: i32, bpos: u32) {
        let u = self.t as usize;
        let y = bit as usize;
        let mut destino = self.nx[u][y] as usize;
        // ¿Se parte? Sólo si esta arista ya se usa mucho Y al destino le sobra estadística
        // para repartir. Sin la segunda condición se clonan estados vacíos y no se aprende
        // nada, sólo se gasta memoria.
        if self.n + 1 < DMC_NODOS
            && self.c[u][y] >= *DMC_UMBRAL
            && self.total(destino) as u16 as u32 > 0
            && self.total(destino) > self.c[u][y] as u32 + *DMC_UMBRAL2 as u32
        {
            let nuevo = self.n;
            self.n += 1;
            self.nx[nuevo] = self.nx[destino];
            // Los conteos del destino se reparten en proporción a lo que aportaba esta
            // arista: el clon nace sabiendo su parte, no de cero.
            let tot = self.total(destino).max(1);
            let parte = self.c[u][y] as u32;
            for k in 0..2 {
                let v = self.c[destino][k] as u32;
                let mio = (v * parte) / tot;
                self.c[nuevo][k] = mio as u16;
                self.c[destino][k] = (v - mio) as u16;
            }
            self.nx[u][y] = nuevo as u32;
            destino = nuevo;
        }
        // Conteo con techo: al llegar arriba se parten los dos, que es lo que hace al modelo
        // no estacionario — si la fuente cambia, se entera.
        if self.c[u][y] >= 60000 { self.c[u][0] /= 2; self.c[u][1] /= 2; }
        self.c[u][y] += 1;
        self.t = destino as u32;
        // Al cerrar un byte se vuelve a la raíz del árbol de arranque si el camino murió.
        if bpos == 7 && self.nx[destino][0] == 0 && self.nx[destino][1] == 0 {
            self.nx[destino] = [0, 0];
            self.t = 0;
        }
    }
}

// ---------- modelo: N contextos hasheados + mezclador logístico entero + APM ----------
/// Etapas de la cadena ISSE: órdenes 2,3,4,6 y palabra, encadenadas sobre la base de orden 1.
const NISSE: usize = 7;
/// Qué contexto refina cada etapa, de menos a más específico: órdenes 2, 3, 4, 5, palabra, 8
/// y 12. Antes iba implícito (cxt[k+2]), lo que ataba la cadena al orden en que estuvieran
/// declarados los modelos; ahora se dice cuál es cuál.
const ISSE_CTX: [usize; NISSE] = [2, 3, 4, 5, 6, 7, 14];

/// ⚠️ EL UNICO NEGATIVO CADUCADO DE LA TABLA DE CONSTANTES, ahora en perilla para repetirlo.
///
/// Se midio que meter tambien las salidas INTERMEDIAS de la cadena al mezclador EMPEORABA
/// —0,7349 contra 0,7263 bpb en codigo— porque un mezclador con 512 juegos de pesos no converge
/// dentro de un frame. Pero aquello era el mezclador de UNA capa. Hoy hay DOS, con cuatro
/// especializaciones y una capa final que reparte confianza, o sea que la premisa de la medida
/// («no converge») ya no se cumple. Un negativo cuya premisa cambio no es un negativo: es una
/// medida pendiente.
///
/// ✅ MEDIDO EL 02-08 Y EL NEGATIVO QUEDA REFUTADO. Seis tipos, 8 MiB cada uno, --max:
///
/// ```text
///     codigo    987.994 -> 985.202   -0,283%      logs      119.439 -> 119.501   +0,052%  ⚠️
///     csvjson   487.323 -> 486.501   -0,169%      sql       480.559 -> 480.125   -0,090%
///     bin     1.151.516 -> 1.143.366 -0,708%      pdfoffice 7.370.174 -> 7.355.128 -0,204%
///     ------------------------------------------------------------------------------------
///     TOTAL  10.597.005 -> 10.569.823            -0,257%      ciclos +0,5%   RSS 1,00x
/// ```
///
/// Cinco de seis. Con el mezclador de UNA capa esto perdia; con el de DOS gana. No cambio la
/// idea, cambio quien la recibe.
///
/// ⚠️ Y EL COSTE NO ES CERO, aunque la memoria si: son seis escrituras de `st[]` por bit, +0,5%
/// de ciclos. El producto escalar no crece (NCTXP sigue en 160) y el RSS es identico en los seis
/// tipos. El cambio se paga en reloj, no en RAM.
/// ⚠️ Pierde en logs (+0,052%), como el DMC. Se adopta igual y se deja escrito.
static ISSE_INT_ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    std::env::var("VC_ISSE_INT").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) != 0
});

/// Modelos ICM directos. A los órdenes 0..5 y palabra se suman cuatro que miran el dato de
/// otra manera —y que son justo los que zpaq tiene y nosotros no teníamos:
///  - orden 8: contexto largo, para repeticiones que el match model no engancha.
///  - dos SPARSE, que se SALTAN bytes: uno mira solo el byte a distancia 2, otro los de
///    distancia 1 y 3. En datos con relleno, campos de ancho fijo o binarios, el byte
///    inmediatamente anterior es ruido y el de más atrás es la señal.
///  - INDIRECTO: no usa el contexto, usa lo que HISTÓRICAMENTE vino después de ese contexto.
///    Es lo que captura "después de una coma casi siempre va un dígito" sin que nadie se lo
///    haya dicho.
///  - tres de FILA: cuando el dato es en realidad una tabla —CSV, JSON de API, logs de
///    formato fijo, volcados SQL, estructuras binarias— el mejor predictor de un byte no es
///    el que tiene al lado sino el que tiene ENCIMA: la misma columna de la fila anterior.
/// Candidatos a marcar registro, en orden de preferencia. El salto de línea primero.
const SEPS: [u8; 6] = [b'\n', b',', b';', b')', b'\t', b'|'];
const NSEP: usize = 6;

const NTAB: usize = 36;
/// Campos distintos que recordamos por registro. Mas alla se agrupan en el ultimo: una tabla
/// con 200 columnas es rarisima y el coste de equivocarse ahi es cero.
const NCAMPO: usize = 64;
/// Ranuras de los modelos numericos, para poder ablacionar por separado (VC_NUM).
const INUM: usize = 26;
/// Ranuras de la familia de texto (VC_TXT).
const ITXT: usize = 30;
/// El modelo de ANIDAMIENTO: la huella de la pila de corchetes, y esa huella cruzada con el
/// byte anterior. Idea leida de `nestModel` de paq8px el 01-08; los pesos son nuestros y estan
/// barridos aparte. Ver `PESOS_ANID`.
const IANID: usize = 34;
/// Letras del run alfanumerico que guardamos para poder sacarle la raiz. Una palabra inglesa
/// de mas de 24 letras no existe fuera de la quimica organica.
const WMAX: usize = 24;

/// Raiz aproximada: quita el sufijo ingles mas largo que deje al menos 3 letras.
///
/// No es Porter ni falta: lo unico que se le pide es que "running", "runs" y "run" caigan en
/// la MISMA ranura. Equivocarse cuesta un contexto un poco peor, no un fallo — la
/// reversibilidad no depende de esto, es solo estadistica.
#[inline]
fn raiz(w: &[u8]) -> u32 {
    const SUF: [&[u8]; 10] = [b"ation", b"ness", b"ment", b"able", b"ible",
                              b"tion", b"ing", b"ed", b"ly", b"es"];
    let mut n = w.len();
    for sx in SUF {
        if n >= sx.len() + 3 && w[n - sx.len()..n].eq_ignore_ascii_case(sx) {
            n -= sx.len();
            break;
        }
    }
    if n >= 4 && (w[n - 1] | 0x20) == b's' { n -= 1; }
    let mut x = 0x811C9DC5u32;
    for &b in &w[..n] { x = (x ^ (b | 0x20) as u32).wrapping_mul(0x01000193); }
    x
}

/// Digito en la posicion `pos` (desde la IZQUIERDA) de un numero de `len` digitos.
/// Devuelve 10 si esa posicion no existe — que es informacion util, no un fallo: significa
/// "el numero de arriba ya se habia acabado aqui".
#[inline]
fn digito_en(v: u64, len: u32, pos: u32) -> u32 {
    if pos >= len || len == 0 || len > 19 { return 10; }
    let mut d = 1u64;
    for _ in 0..(len - 1 - pos) { d = d.saturating_mul(10); }
    ((v / d) % 10) as u32
}
/// Entradas del mezclador: los ICM directos + match + la salida de la cadena ISSE.
/// Donde empiezan las entradas de «contexto determinista»: una por contexto de tabla.
///
/// Van al FINAL del vector a proposito. Asi si el interruptor las apaga, se quedan a 0 y sus
/// pesos no se mueven —el mezclador no les suma nada—, que es exactamente el mismo mecanismo
/// que ya usa `apagados` para desactivar un modelo entero. Un apagado limpio, sin ramas.
/// Donde empiezan las entradas de RACHA: una por contexto de tabla. La ranura recuerda el
/// ultimo byte visto detras de ese contexto y cuantas veces seguidas.
const IRACHA: usize = NTAB + 9;
/// Donde empiezan las entradas de LOS TRES ULTIMOS BYTES DISTINTOS. La racha cubre «aqui siempre
/// viene lo mismo»; esto cubre «aqui vienen estos tres y hoy toca uno», que en datos
/// estructurados —CSV, JSON, SQL, codigo— es al menos tan comun.
const ITRES: usize = NTAB + 9 + NTAB;
const IDET: usize = NTAB + 9 + NTAB + NTAB;
/// Cuantas entradas gobierna la mascara `apagados`, que es un u64. Son las ORIGINALES: las
/// vistas «determinista» del final NO entran ahi —las gobierna VC_VISTAS— y meterlas habria
/// desbordado la mascara en cuanto NCTX paso de 64. El compilador lo caza; en C habria sido un
/// desplazamiento indefinido.
const NCTX_MASCARA: usize = NTAB + 9;
/// Y QUE QUEPA EN EL u64 LO COMPRUEBA EL COMPILADOR, no yo. Sin esta linea, pasar de 64 entradas
/// ablacionables no daria un error: daria indices que se enmascaran solos y una medida que
/// contesta a otra pregunta. Es exactamente lo que pasaba (ver apaga_suelto).
const _: () = assert!(NCTX_MASCARA <= 64);
/// Donde empiezan las salidas INTERMEDIAS de la cadena ISSE: una por etapa menos la ultima, que
/// ya entra por IISSE. Van al final a proposito, y son GRATIS: NCTX era 153 y NCTXP 160, o sea
/// que el mezclador ya recorria SIETE ranuras de relleno a peso cero en cada producto escalar.
/// Meter seis ahi no añade ni una instruccion SIMD ni un byte de pesos — el mismo truco que
/// IDISP/IDISP2 en su dia. Que siga siendo gratis lo comprueba el compilador, abajo.
const IISSE_INT: usize = NTAB + 9 + NTAB + NTAB + NTAB;
const NISSE_INT: usize = NISSE - 1;
const NCTX: usize = IISSE_INT + NISSE_INT;
const IDMC: usize = NTAB;       // ranura del DMC
const IMM: usize = NTAB + 1;    // ranura del match model corto (6 bytes)
const IMM2: usize = NTAB + 2;   // ranura del match model largo (12 bytes)
const IISSE: usize = NTAB + 3;  // ranura de la salida de la cadena ISSE
const IMM3: usize = NTAB + 4;   // ranura del match model DISPERSO (zancada 2)
const IMM4: usize = NTAB + 5;   // ...zancada 4 (registros de enteros de 32 bits)
const IMM8: usize = NTAB + 6;   // ...zancada 8 (de 64 bits)
/// Dispersion de las predicciones: lo que un mezclador LINEAL no puede ver.
const IDISP: usize = NTAB + 7;
/// ...y la dispersion con el sentido de la media, para que discrepar hacia arriba y discrepar
/// hacia abajo no sean lo mismo.
const IDISP2: usize = NTAB + 8;
/// Tamaño de cada tabla, a medida. El orden 0 sólo tiene 256 contextos posibles: darle 16 MB
/// como a los demás era tirar memoria y ensuciar la caché para nada.
/// Ahora cuenta CUBOS de 16 bytes, no ranuras sueltas: cuatro bits menos para el mismo gasto.
/// Es el tamaño BASE, para bloques pequeños; los grandes lo suben (ver `bits_para`).
/// Tope de bits por modelo, deducido de la CARDINALIDAD REAL de su contexto.
///
/// El censo (VC_CENSO=1) destapó que varios modelos reservan 32 MiB para hashear dos bytes.
/// Un contexto de dos bytes tiene 65.536 valores posibles: da igual el tamaño del fichero,
/// esa tabla NO PUEDE llenarse nunca. sparse-paso8 era el caso extremo — 8 MiB, ocupación
/// 0,2%, para hashear UN byte.
///
/// 99 = sin tope (contexto de cardinalidad grande de verdad, que sí crece con el fichero).
/// Modelos que la cadena ISSE ya refina (ISSE_CTX = 2,3,4,5,6,7,14). Tenerlos ADEMÁS como
/// modelo directo es pagar dos veces por la misma información.
const DUPLICADOS: u64 = (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 7) | (1 << 14);

/// La PODA de la v2: diez modelos que aportan NEGATIVO —quitarlos hace el fichero mas
/// pequeno— validados en las cinco muestras, no en las dos con las que se detectaron.
///
/// Los de fila y columna (11,12,13,17,18,19) tambien salian negativos en sql y enwik8 y NO
/// estan aqui: se midieron sin csvjson, que es el dato tabular para el que existen. Podarlos
/// sale +0,264%. Medir en un subconjunto y generalizar es el error que mas veces se ha
/// repetido en este compresor.
const PODA_V2: u64 = (1 << 1)                                   // orden-1
    | (1 << 8) | (1 << 9) | (1 << 22) | (1 << 23) | (1 << 24)   // los dispersos
    | (1 << 15)                                                  // pareja-palabra
    | (1 << 30) | (1 << 31) | (1 << 32);                         // txt: trigrama, raiz, tri-raices

/// Que modelos apaga cada VERSION DE MODELO. Aqui esta la razon de que la version viaje en el
/// flujo: la mascara la aplican los dos lados, y si el descompresor usara otra distinta de la
/// que uso el compresor, saldria basura en silencio.
/// A partir de aqui la poda gana. Medido: el cruce esta sobre los 6 MiB y se pone en 8 para
/// podar solo donde la medida es clara. Con --max los tramos son de 32 MiB, asi que el
/// archivado real siempre cae de este lado.
const PODA_DESDE: usize = 8 << 20;

/// Las tasas de aprendizaje de cada version de modelo.
///
/// Van aqui y no en una constante suelta porque las aplican los DOS lados: el descompresor
/// tiene que aprender exactamente igual que el compresor, o las probabilidades divergen y sale
/// basura. Un archivo viejo se abre con las tasas de su epoca, no con las de hoy.
///
///   v1, v2  tope 1023, paso >>10   los valores originales, nunca medidos
///   v3      tope  127, paso >>12   el optimo medido: -1,111% en las cinco muestras
fn tasas_de_modelo(modelo: u8) -> (usize, u32) {
    match modelo {
        1 | 2 => (1023, 10),
        _ => (127, 12),
    }
}

fn apagados_de_modelo(modelo: u8, tam: usize) -> u64 {
    // 🔑 DESDE LA v7, LA DISPERSION SE APAGA. Lleva desde el 31-07 senalada —el 83,8% del peso
    // del mezclador y aportacion NEGATIVA— y hoy se ha re-medido con el binario del dia, por
    // racimo y con ciclos (etiqueta «V1 racimos con reloj»):
    //
    //     codigo  -0,145% bytes  -0,5% ciclos       sql  -0,482% bytes  +0,4% ciclos
    //
    // Mejora el ratio en las DOS muestras y el reloj no se mueve: no es un intercambio, es que
    // sobraba. Y se entiende por que: se calcula LEYENDO las predicciones de los demas, o sea que
    // es una funcion de las otras preguntas, no una pregunta nueva. Peso alto no es aportacion —
    // es que el mezclador le hace mucho caso, y si lo que dice es ruido correlacionado con el
    // resto, hacerle caso es justo lo que sobra.
    //
    // Va aqui y no en una variable de entorno POR LA MISMA RAZON QUE LA v6: `VC_DISP` no viaja en
    // el archivo, y una perilla que cambia la prediccion y no viaja produce archivos que no se
    // pueden reabrir. Aqui la decide la version, que si viaja.
    let disp = if modelo >= 7 { (1u64 << IDISP) | (1u64 << IDISP2) } else { 0 };
    disp | match modelo {
        1 => 0,                                    // la v1 solo quitaba los duplicados
        _ if tam >= PODA_DESDE => PODA_V2,
        _ => 0,                                    // bloque pequeno: los podados aun aciertan
    }
}

/// Qué modelos apaga cada perfil. El compresor elige uno y lo ESCRIBE en el archivo: el
/// descompresor lo lee en vez de deducirlo, así que no hay forma de que los dos lados
/// discrepen. (El intento anterior dejaba que el códec lo adivinara mirando el dato y falló
/// justo por ahí.)
/// VC_NUM: mascara de 4 bits para ablacionar los modelos numericos por separado.
/// 1=digito-encima 2=largo 4=delta 8=prefijo. Por defecto todos (15).
fn num_apagados() -> u64 {
    let m: u64 = std::env::var("VC_NUM").ok().and_then(|v| v.parse().ok()).unwrap_or(15);
    let mut a = 0u64;
    for k in 0..4 { if m >> k & 1 == 0 { a |= 1u64 << (INUM + k); } }
    a
}

/// Que familias de modelos extra usa este bloque. Se elige al COMPRIMIR (probando de verdad,
/// ver `elige_familia`) y se ESCRIBE en el flujo; el descompresor la lee.
///   0 = ninguna · 1 = numerica · 2 = texto · 3 = las dos
fn familia_apagados(f: u8) -> u64 {
    let mut a = 0u64;
    if f & 1 == 0 { for k in 0..4 { a |= 1u64 << (INUM + k); } }
    if f & 2 == 0 { for k in 0..4 { a |= 1u64 << (ITXT + k); } }
    a
}

/// VC_TXT: mascara de 4 bits para ablacionar la familia de texto.
/// 1=trigrama 2=raiz 4=trigrama-raices 8=skip-grama. Por defecto todos (15).
fn txt_apagados() -> u64 {
    let m: u64 = std::env::var("VC_TXT").ok().and_then(|v| v.parse().ok()).unwrap_or(15);
    let mut a = 0u64;
    for k in 0..4 { if m >> k & 1 == 0 { a |= 1u64 << (ITXT + k); } }
    a
}

/// VC_APAGA=k apaga SOLO el modelo k, para medir su aportacion marginal dejandolo fuera.
/// Herramienta de diagnostico: fuera de ella no hace nada.
/// VC_APAGAMASK: mascara decimal de modelos a apagar de golpe, para validar una poda
/// completa en vez de uno a uno.
/// Los 9 modelos DERIVADOS (34..42) YA SE PUEDEN APAGAR (31-07). Antes no, y pedirlo no hacia
/// nada Y NO LO DECIA: el archivo salia con los mismos bytes exactos, que se lee igual que «este
/// modelo no aporta nada». Paso de verdad pidiendo apagar los tres match-disp.
///
/// ⚠️ PERO NO SIGNIFICAN LO MISMO QUE LOS DE TABLA. Apagar un derivado le quita la VOZ ante el
/// mezclador; su computo sigue, porque varios se alimentan entre si. El numero que sale es
/// cuanto aporta su prediccion AL RATIO. Lo que costaria en TIEMPO quitarlo de verdad es otra
/// medida, en serie y en una sola maquina.
///
/// Importa mas de lo que parece: segun el volcado de pesos (bench/m4-pesos.tsv), los dos
/// modelos en los que MAS se apoya el mezclador —cadena ISSE al 100% y no-lineal:dispersion al
/// 83,8%— estan justo en ese rango. O sea que toda medida de aportacion por modelo hecha hasta
/// hoy cubria 34 de 43 y dejaba fuera los dos que mandan.
fn avisa_si_no_ablacionable(bits: u64, de_donde: &str) {
    // Pedir un indice que no existe SI se para: antes se colaba en silencio dentro de la
    // mascara y salia un archivo identico, que se lee como «no aporta nada».
    if bits >> NCTX_MASCARA != 0 {
        eprintln!("ERROR: {de_donde} pide apagar modelos por encima del {}, y solo hay {NCTX}.",
                  NCTX - 1);
        std::process::exit(2);
    }
    // Los derivados ya se pueden apagar, pero NO significan lo mismo: se les quita la voz ante
    // el mezclador y su computo sigue. Se dice en cada ejecucion para que nadie lea el numero
    // como un ahorro de tiempo.
    let der = bits >> NTAB;
    if der != 0 {
        let cuales: Vec<String> = (NTAB..NCTX_MASCARA)
            .filter(|&i| bits >> i & 1 != 0)
            .map(|i| format!("{i} ({})", nombre_ctx(i)))
            .collect();
        eprintln!("AVISO: {de_donde} apaga modelos DERIVADOS: {}", cuales.join(", "));
        eprintln!("       Se les quita la VOZ ante el mezclador; su computo SIGUE, porque varios");
        eprintln!("       se alimentan entre si. El numero mide aportacion AL RATIO, no tiempo.");
    }
}

/// M5: acumular la entropia de lo predicho para compararla con lo escrito. Apagado por
/// defecto; encenderlo mete un logaritmo por bit y eso si se nota.
///
/// Va con OnceLock y NO con un booleano que hay que poner al arrancar: la primera version lo
/// ponia DESPUES del bucle de codificacion, asi que no acumulaba nada y el guardia de flush()
/// se lo tragaba en silencio. Un interruptor que se lee solo la primera vez que hace falta no
/// se puede cablear al reves.
fn ent_on() -> bool {
    static E: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_ENTROPIA").is_ok())
}

/// VC_MM_VERIF=1 · ¿y si el `match` COMPROBARA que la coincidencia es real?
///
/// LO QUE PASA HOY. `mm_hashtab[mh]` se lee y se usa SIN COMPARAR NI UN BYTE. La tabla tiene
/// 2^21 = 2.097.152 ranuras y un tramo con `--max` es de 32 MiB, o sea hasta 33,5 M de
/// posiciones: **unas 16 posiciones por ranura**. Colisionar no es el caso raro, es lo normal.
/// Cuando colisiona, el modelo apunta a datos que no se parecen en nada y empieza a votar desde
/// ahi.
///
/// POR QUE PUEDE NO IMPORTAR, y por eso se mide en vez de arreglarlo a ciegas: el arranque falso
/// se corrige solo. `mm_len` empieza en 1, la confianza que vota es la MINIMA (`min(32)*64` = 64)
/// y al primer bit fallado `mm_len` vuelve a 0. Un match falso vive un puñado de bits y vota
/// flojo. La pregunta honesta no es «¿hay colisiones?» —las hay seguro— sino **cuánto cuesta el
/// ruido que meten mientras viven**.
///
/// Y hay un sospechoso esperando: `no-lineal:dispersion` se lleva el 83,8% del peso del mezclador
/// y APAGARLO MEJORA el archivo en texto. Si el ruido del match es de verdad ruido, ese peso
/// puede estar aprendiendo a compensarlo.
///
/// LA COMPROBACION no depende de como este ordenado `hist`: compara los 6 bytes que preceden al
/// CANDIDATO contra los 6 que preceden a la posicion ACTUAL, los dos dentro de `mm_buf`. Es
/// exactamente lo que el hash afirma —ni mas ni menos— y cuesta un memcmp de 6 bytes, y solo en
/// los arranques, no en cada byte.
///
/// LO QUE SALIO, sobre 4 MiB de cada tipo. Primero cuantos arranques eran mentira:
///     codigo 22,79% · sql 6,00% · logs 2,11% · csvjson 14,90% · bin 36,98% FALSOS
/// En binario mas de un tercio de los arranques apuntaban a datos que no se parecen en nada. Y
/// eso es el caso BUENO: con 4 MiB la tabla va a 2 posiciones por ranura, y con los tramos de
/// 32 MiB de `--max` va a 16.
///
/// Y despues los bytes, que es lo que decide:
///     codigo -0,0496% · sql -0,0403% · logs -0,0233% · csvjson -0,0798% · bin -0,2837%
///     ponderado por bytes: -0,1399%
/// Mejora los CINCO. Que sea monotono es lo que lo hace creible: un cambio que ayuda en unos
/// tipos y estorba en otros es una perilla que hay que elegir; uno que ayuda en todos es un
/// arreglo. Ida y vuelta OK.
///
/// LO QUE **NO** ERA, y se comprobo en vez de suponerlo: el censo de junturas sospechaba que
/// `no-lineal:dispersion` —83,8% del peso del mezclador, y apagarlo MEJORA el texto— estaba
/// aprendiendo a compensar este ruido. No. Apagarla gana -0,9201% en logs con el match de antes
/// y -0,8970% con el verificado: practicamente lo mismo. Los dos efectos son independientes.
/// Negativo, y se anota igual que uno positivo.
///
/// VC_MM_VERIF=1 lo fuerza, =0 lo apaga. Sin poner, manda la VERSION DE MODELO.
fn mm_verif_env() -> Option<bool> {
    static E: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_MM_VERIF").ok().map(|v| v != "0"))
}

/// Desde la v4 el `match` comprueba la coincidencia antes de creersela.
///
/// Va aqui, con lo demas que depende de la version, y NO en una constante suelta, porque esto lo
/// aplican LOS DOS LADOS: si el descompresor verificara y el compresor no —o al reves— los dos
/// predictores divergirian y saldria basura EN SILENCIO. Un archivo de la v3 se abre con el
/// match de la v3.
fn verifica_match_de_modelo(modelo: u8) -> bool { modelo >= 4 }

/// EL SALTO DE MATCH POR BYTE (F6·Q6). Con una racha confirmada de `UMBRAL_SALTO` bytes o mas, el
/// byte entero se codifica con el match mas UNA etapa SSE y se salta el resto del conjunto.
///
/// 🔑 POR QUE NO HACE FALTA SEÑALIZAR NADA EN EL ARCHIVO. La decision se toma AL EMPEZAR EL BYTE
/// y solo con `mm_len`, que es estado que el descompresor tiene identico en ese mismo instante:
/// ha decodificado exactamente los mismos bytes y ha corrido el mismo `mm_len`. Los dos lados
/// toman la misma rama sin hablar. Por eso esto NO toca el formato del contenedor.
///
/// ⚠️ Y POR ESO MISMO VA POR VERSION DE MODELO Y NO POR VARIABLE DE ENTORNO: cambia los bytes que
/// se emiten. `VC_TRES`/`VC_DISP`/`VC_PERFIL` no viajan en el archivo —comprimir con ellas y
/// extraer sin ellas falla— y esto tiene que viajar, porque un archivo v8 se sigue leyendo con el
/// camino de la v8.
///
/// ⚠️ ES `== 9`, NO `>= 9`, Y ESO ES DELIBERADO. El A/B de las ocho muestras (1fa46de, 17-08)
/// cerro la v9 EN NEGATIVO para --max: se queda IMPLEMENTADA y LEIBLE —un archivo escrito con
/// `VC_MODELO=9` tiene que poder abrirse— pero es una rama MUERTA del arbol de versiones. La
/// v10 parte de la v8, no de la v9. Con `>=` heredaria un camino que ya se midio y se descarto.
fn salta_de_modelo(modelo: u8) -> bool { modelo == 9 }

/// La racha a partir de la cual el byte se codifica solo con el match.
///
/// 32 y no 16 sale de la tabla de cobertura de la Fase 1 (`bench/f6q6-cobertura.tsv`): a >=16 se
/// cubre un 54,00% de los bytes del corpus pero el match acierta el 93,4% en enwik8; a >=32 se
/// cubre un 45,34% —cota de Amdahl 1,83x, de sobra para el falsador— y el acierto sube a 96,1%
/// en el peor caso y a 99%+ en los demas. Se compra el 8,7% de cobertura mas barato del corpus a
/// cambio del tramo donde el match miente mas, que es justo donde el salto costaria bytes.
///
/// ⚠️ Y EL VALOR QUE SE EMITE TIENE QUE SER UNO DE LOS MEDIDOS. Aqui quedo puesto un 512 —fuera
/// del conjunto {16, 32, 64} de `bench/f6q6-cobertura.tsv`— que es una constante que nadie ha
/// medido: la cobertura a 512 no esta en la tabla, asi que ni se sabe cuanto salta ni cuanto
/// cuesta, y el parrafo de arriba argumentaba por 32 mientras el codigo hacia otra cosa. Vale 32,
/// que es lo que la tabla sostiene.
const UMBRAL_SALTO: u32 = 32;

/// ¿Se calcula la vista de «los tres distintos»? Hasta la v5 si; desde la v6 NO.
///
/// MEDIDO EL 03-08 (etiqueta «V2 vistas por modelo» en el libro mayor), cuatro muestras, mismo
/// binario, control repetido byte a byte al principio y al final:
///
/// ```text
///     codigo  +0,043% bytes  -17,1% ciclos      csvjson  -0,132%  -24,7%
///     logs    +0,020%        -33,3%             sql      -0,181%  -20,0%
///     TOTAL   -0,049% BYTES  -24,3% CICLOS
/// ```
///
/// No es un intercambio: son 36 entradas del mezclador que se van y el archivo sale MAS PEQUEÑO
/// en dos de las cuatro muestras. No es que fueran malas preguntas: es que ya estaban contestadas
/// por la racha y por la historia de bits, y el mezclador perdona la redundancia en bytes pero la
/// cobra en tiempo. Las 36 ranuras siguen existiendo en el mezclador —a cero— porque quitarlas
/// cambiaria `NCTX` y con el la forma de los archivos VIEJOS, que hay que poder seguir abriendo.
fn tres_de_modelo(modelo: u8) -> bool { modelo < 6 }

/// Cuantos arranques del `match` son de verdad y cuantos son solo el mismo hash. Globales y no
/// campos del Predictor porque hay uno por tramo y lo que interesa es el total; se imprimen al
/// soltar el Predictor si VC_MM_STATS esta puesto, y la ultima linea lleva el acumulado.
pub static MM_CIERTOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static MM_FALSOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// COBERTURA DE MATCH (F6·Q6) — instrumento de MEDIDA, no de producto.
///
/// 🔑 QUE CONTESTA. Antes de escribir una linea del salto por byte hay que saber QUE FRACCION DE
/// LOS BYTES vive dentro de una racha de match confirmada. Si es pequeña, el techo del ahorro es
/// pequeño y no hay palanca — y eso se sabe con una pasada barata, no escribiendo el modelo.
///
/// ⚠️ VIVE DETRAS DE `--features cobertura` Y NO PUEDE MOVER UN SOLO BYTE. Solo LEE estado que ya
/// existe (`mm_len`, `mm_ptr`) y suma en contadores globales; no toca ninguna prediccion, ningun
/// contexto y ninguna tabla. La prueba no es este comentario: es el sha256 de la salida con y sin
/// la bandera, que tiene que ser IDENTICO (ver docs/f6-salto-de-match.md).
///
/// La racha se lee AL EMPEZAR EL BYTE, que es el instante en el que el salto tendria que decidir
/// —y el unico en el que compresor y descompresor tienen exactamente el mismo estado—.
#[cfg(feature = "cobertura")]
pub mod cobertura {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    /// Los tres umbrales del encargo. Un byte "esta cubierto a T" si al empezarlo la racha del
    /// match valia >= T.
    pub const UMBRAL: [u32; 3] = [16, 32, 64];

    pub static TOTAL: AtomicU64 = AtomicU64::new(0);
    /// bytes cubiertos por el match PRINCIPAL (el de 6 bytes, el unico que VERIFICA la
    /// coincidencia desde la v4: `verifica_match_de_modelo`).
    pub static CUB: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
    /// de esos, en cuantos el byte que el match predecia era EL BUENO. Es lo que decide si el
    /// salto se puede permitir: saltar el conjunto es apostar a esta cifra.
    pub static ACI: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
    /// bytes cubiertos por CUALQUIERA de los cinco match (el mejor de las cinco rachas). Cota
    /// superior: los otros cuatro no verifican, asi que enganchan mas y mienten mas.
    pub static CUB5: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

    /// Una linea por umbral, en TSV, para que la lea un guion sin parsear prosa.
    pub fn informe() -> String {
        let t = TOTAL.load(Relaxed);
        let mut s = format!("cobertura\ttotal_bytes\t{t}\n");
        for (i, u) in UMBRAL.iter().enumerate() {
            let (c, a, c5) = (CUB[i].load(Relaxed), ACI[i].load(Relaxed), CUB5[i].load(Relaxed));
            let f = |x: u64| if t == 0 { 0.0 } else { 100.0 * x as f64 / t as f64 };
            let ac = if c == 0 { 0.0 } else { 100.0 * a as f64 / c as f64 };
            s += &format!(
                "cobertura\tu{u}\tcub={c}\tcub_pct={:.4}\taci={a}\taci_pct={ac:.4}\tcub5={c5}\tcub5_pct={:.4}\n",
                f(c), f(c5));
        }
        s
    }
}

fn apaga_mascara() -> u64 {
    let m = std::env::var("VC_APAGAMASK").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    avisa_si_no_ablacionable(m, "VC_APAGAMASK");
    m
}

fn apaga_suelto() -> u64 {
    match std::env::var("VC_APAGA").ok().and_then(|v| v.parse::<u32>().ok()) {
        // Los de tabla y los 9 derivados. EL TOPE ES NCTX_MASCARA, NO NCTX, y la diferencia no
        // era cosmetica: la mascara es un u64 y `1u64 << k` con k>=64 en release NO desborda —se
        // enmascara a k&63—, asi que VC_APAGA=64 apagaba el modelo 0 y VC_APAGA=103 el 39, en
        // silencio y devolviendo un archivo perfectamente valido. Medido en codigo (300 KB):
        // 64 y 0 dan los MISMOS 37.040 bytes; 103 y 39 los mismos 38.148. El mensaje de error
        // invitaba al fallo — decia «solo hay 153 modelos», que es una promesa que un u64 no
        // puede cumplir.
        Some(k) if (k as usize) < NCTX_MASCARA => {
            let m = 1u64 << k;
            avisa_si_no_ablacionable(m, "VC_APAGA");
            m
        }
        Some(k) => {
            eprintln!("ERROR: VC_APAGA={k}. Ablacionables son las {NCTX_MASCARA} primeras entradas \
                       (0..{}). Las vistas racha/tres/det NO se apagan una a una: las gobiernan \
                       VC_RACHA, VC_TRES y VC_VISTAS.", NCTX_MASCARA - 1);
            std::process::exit(2);
        }
        _ => 0,
    }
}

/// EL PERFIL QUE SE ESCRIBE EN EL ARCHIVO. Se lee del entorno UNA vez, al comprimir.
///
/// 🚨 POR QUE EXISTE: hasta hoy el perfil NO VIAJABA. `Predictor::new` leia `VC_PERFIL` del
/// entorno en LOS DOS LADOS, y el archivo no guardaba cual se habia usado. Comprobado, no
/// razonado: `VC_PERFIL=2 vc-cm c` y despues `vc-cm d` **sin la variable** devuelve bytes
/// distintos **con exit 0 y sin una sola queja**. Con la variable puesta, correcto.
///
/// Es exactamente la trampa que P14 del camino tenia escrita —«una bandera que produce archivos
/// ilegibles sin avisar es una trampa esperando»— y la misma que ya obligo a marcar los archivos
/// ablacionados con el modelo 254. Dentro del archivador la pilla el blake3; el codec suelto, no.
///
/// Y ademas bloqueaba lo que el dueño pide: «versiones mas ligeras EN EL MENU». Un menu cuyas
/// opciones no se pueden volver a abrir no es un menu.
fn perfil_a_escribir() -> u8 {
    std::env::var("VC_PERFIL").ok().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0).min(15)
}

// ❌ LOS ORDENES CLASICOS NO SE QUITAN DEL DEFECTO, Y EL PORQUE VALE MAS QUE EL NUMERO (03-08).
//
// Medido por racimo sobre el defecto v7 (orden-0..5, orden-8, orden-12 apagados de golpe, que de
// uno en uno la redundancia no se ve). En TEXTO salen sobrando —mejoran los bytes y ahorran
// reloj—; en NO-TEXTO se pagan:
//
//     codigo  -0,013% bytes  -1,6% ciclos      bin  +0,480% bytes  -2,0% ciclos
//     sql     -0,050%        -3,1%             pdf  +0,112%        -2,4%
//     logs    -0,285%        -1,3%             png  +0,073%        -3,1%
//     csvjson -0,013%        -2,1%
//     enwik8  -0,001%        -2,2%
//
// EL SIGNO DEPENDE DEL TIPO, asi que esto NO es una decision global: es una del menu. Con las
// cinco muestras de texto delante la conclusion habria sido «fuera los ordenes» y habriamos
// perdido medio punto en binario — que es exactamente el error que este banco ya cazo el 02-08
// («sobre-generalice familia 3 siempre desde tres tipos de texto»).
//
// Donde va: al PERFIL o a la familia, que es la maquinaria que ya elige por tipo y ESCRIBE la
// eleccion en el archivo. No aqui.

/// Los modelos 11,12,13,17,18,19: columna y encima. El racimo `fila`.
const FILA: u64 = (1 << 11) | (1 << 12) | (1 << 13) | (1 << 17) | (1 << 18) | (1 << 19);

/// 🚨 RECIBE EL MODELO, Y NO ES UN CAPRICHO. El perfil VIAJA en el archivo desde que se arreglo
/// `perfil_a_escribir`, asi que cambiar lo que significa «perfil 2» rompe los archivos que ya
/// estan hechos con el: el descompresor apagaria modelos que el compresor tenia encendidos y
/// leeria las tablas fuera de sitio. Por eso lo nuevo entra por VERSION, igual que la v6 y la
/// v7, y el perfil 2 de un archivo viejo sigue significando lo que significaba.
fn apagados_de_perfil(perfil: u32, modelo: u8) -> u64 {
    match perfil {
        // POR DEFECTO fuera los duplicados: mejor ratio (-0,15% en el total) y -10% de tiempo.
        0 => DUPLICADOS,
        // Sólo para poder reproducir el comportamiento viejo y comparar.
        1 => 0,
        // Rápido: además fuera palabra, parejas y forma. Cuesta entre 1,5% y 6% de ratio.
        //
        // 🔑 Y DESDE LA v8, TAMBIEN `fila`. Medido el 03-08 sobre las cinco muestras de texto,
        // DENTRO del propio perfil 2 (etiqueta «V4 fila en el perfil rapido»):
        //
        //     codigo +1,995% / -15,5%    csvjson +1,984% / -14,9%    enwik8 +0,096% / -14,1%
        //     logs   +4,912% / -18,2%    sql     +0,542% / -16,7%    TOTAL +0,904% / -15,8%
        //
        // Un 15,8% de reloj por un 0,9% de bytes. Esto NO va al defecto —ahi el intercambio lo
        // decide el precio y el defecto no puede elegir por el usuario— pero es exactamente lo
        // que este perfil existe para hacer: ya declara que cuesta «entre 1,5% y 6% de ratio»,
        // y quien lo elige ya ha dicho que prefiere el reloj.
        _ => DUPLICADOS | (1u64 << 6) | (1u64 << 15) | (1u64 << 16) | (1u64 << 20) | (1u64 << 21)
             | if modelo >= 8 { FILA } else { 0 },
    }
}

const TAB_TOPE: [usize; NTAB] = [
    99, 99, 18, 99, 99, 99, 99, 99,   // 2 = orden-2, dos bytes
    99, 18, 99, 99, 99, 18,           // 9 = sparse d1+3, 13 = encima x byte
    99, 99, 99, 99, 99,
    18, 99, 99,                       // 19 = encima2 x byte
    12, 12, 18, 99,                   // 22,23 = un byte · 24 = dos bytes
    // NUMERICOS: la cardinalidad se conoce exacta, no hay que adivinarla.
    20, 16, 20, 20,                   // campo(6) x nd(5) x digito(4) x largo(5)
    99, 99, 99, 99,                   // TEXTO: cardinalidad de verdad, crecen con el fichero
    // ANIDAMIENTO. La huella sola tiene cardinalidad ACOTADA de verdad: la profundidad util de
    // un fichero real rara vez pasa de unas decenas, asi que 16 bits sobran. Cruzada con el byte
    // anterior se multiplica por 256 y necesita mas, pero tampoco «99»: no crece con el fichero
    // como el texto, crece con la ESTRUCTURA, que se repite.
    16, 20,
];
const TAB_BITS: [usize; NTAB] = [10, 14, 20, 20, 20, 20, 20, 20, 12, 20, 20, 16, 20, 20, 20, 20, 20, 16, 20, 20, 20, 20, 14, 18, 20, 20,
                                 20, 16, 20, 20,
                                 20, 20, 20, 20,
                                 16, 20];   // anidamiento, anidamiento x byte

/// Cuántos cubos darle a un modelo para un bloque de `tam` bytes.
///
/// Los modelos pequeños (orden 0, orden 1, sparse de un byte) tienen un número FIJO de
/// contextos posibles: crecerlos no sirve de nada y se quedan como están. Los que sí pueden
/// desbordarse crecen con el bloque, porque el desalojo constante es lo que les impide
/// aprender: un contexto que se ve mil veces no sirve si lo echan cada cien.
fn bits_para(base: usize, tam: usize) -> usize {
    if base < 18 { return base; }               // los acotados, ni tocarlos
    // El tope es una CONSTANTE, no la memoria libre de la máquina. Si dependiera de la
    // máquina, el mismo dato daría archivos distintos en dos ordenadores — y la salida
    // byte-idéntica entre arquitecturas es la condición dura del brief. Se paga un poco de
    // ratio a cambio de que el archivo sea el mismo en todas partes.
    //
    // CORRECCION (28-07): el razonamiento de arriba es correcto -el tope tiene que ser
    // CONSTANTE- pero de ahi salte a que fuera PEQUENO, y eso no lo exige nada. `+2` es tan
    // constante como `+6`. paq8px -8 usa 2.408 MB y nosotros 600: le mediamos con cuatro
    // veces nuestra memoria y luego concluiamos que nos ganaba "por arquitectura".
    // VC_MEM anade bits a todas las tablas grandes, y sigue siendo determinista.
    // i32, no usize: hace falta poder RESTAR bits para medir si sobra memoria. El suelo de 10
    // bits esta para que una variable de entorno mal escrita no deje una tabla en cuatro
    // ranuras y convierta el compresor en un generador de colisiones.
    let holgura: i32 = std::env::var("VC_MEM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let extra: i32 = if tam >= 6 << 20 { 2 } else if tam >= 1 << 20 { 1 } else { 0 };
    (base as i32 + extra + holgura).clamp(10, 26) as usize
}
/// LOS BITS EFECTIVOS DE LA TABLA `k` — UNA SOLA EXPRESION.
///
/// Antes esto estaba escrito TRES veces (reserva de `estados`, reserva de `memo` y calculo de
/// `tmask`) y el propio codigo avisaba al lado: «el MISMO tope que la reserva: si la mascara
/// fuera mayor, indexaria fuera». Tres copias de una invariante es la invariante esperando a
/// romperse; aqui hay una. El valor que devuelve es EXACTAMENTE el de antes: byte-identico.
///
/// ══ TRES NEGATIVOS MEDIDOS EL 17-08, Y EL TERCERO INVALIDA A LOS OTROS DOS ══════════════
///
/// El censo (`bench/registros/censo/`) enseña un reparto que parece absurdo: ~229 MiB en tablas
/// al 0,3-3,6% de ocupacion mientras `anidamiento` (34) y `anidamiento x byte` (35) salen al
/// 100,0%. Se probo mover memoria de las vacias a las llenas. Las tres vueltas, con su premisa:
///
///   F7.B1 · encoger 7 + agrandar 34/35   -> +0,009..0,082% bytes, +5,9..14,3% ciclos. PIERDE.
///   F7.B2 · solo encoger las 7           -> +0,009..0,082% bytes, ciclos aparentemente -8..-17%.
///   F7.B3 · solo encoger las 4 de 64 MiB -> +0,009..0,044% bytes, ciclos SIN RESOLVER.
///
/// 1. «100,0% de ocupacion» NO es ahogo cuando el contexto tiene cardinalidad ACOTADA. El
///    comentario de `TAB_TOPE[34]` ya lo decia -«la profundidad util de un fichero real rara vez
///    pasa de unas decenas»- y agrandarla no compro ni un byte, solo reloj. En una tabla hash con
///    contexto acotado, que se toquen todos los cubos solo dice que se tocaron: los mismos
///    contextos caen siempre en el mismo sitio y la colision es benigna.
///
/// 2. RESERVAR NO ES RESIDIR. Una tabla al 1,2% de 64 MiB no cuesta 64 MiB de cache: sus paginas
///    no se tocan nunca. Nunca hubo 229 MiB CALIENTES que recuperar. Agrandar las dos densas si
///    metio 63 MiB calientes contra 32 MiB de L3, y eso si se paga.
///
/// 3. 🚨 EL SUELO DE RUIDO. El «-8..-17% de ciclos» de F7.B2 NO ERA REAL. Se midio contra un
///    control de otro momento. Al repetir A/B/A/B pegado (F7.CTRL vs F7.B3) las dos vueltas
///    salieron con EL SIGNO CAMBIADO -codigo.10m: -3,17% y luego +9,98%- y el propio control,
///    MISMO BINARIO Y MISMA ENTRADA, disperso un 9,3% en codigo y un 13,3% en sql.
///    ⇒ Con la flota corriendo trabajo de cliente encima, en esta maquina NO SE PUEDE RESOLVER
///    un efecto de velocidad por debajo del ~10%. El suelo documentado del banco (1,25% de reloj,
///    2,96% de CPU, `bench/control-cero.sh`) se midio con la maquina EN REPOSO y no vale aqui.
///    Es la misma causa por la que el parrafo de velocidad del README ya salio mal dos veces.
///
/// ⇒ DECISION: el reparto NO entra al defecto. Cuesta bytes (poco, pero determinista y por tanto
/// real) y su ganancia de reloj no esta demostrada. La regla del dueño es que solo se comprime
/// mas si ademas se va mas rapido, y esto no cumple ninguna de las dos.
/// ⇒ LO QUE SI QUEDA MEDIDO Y VALE: encogiendo las cuatro tablas de 64 MiB, la RAM pico baja de
/// 1.385 a 1.159 MB en codigo y de 1.398 a 1.168 en enwik8 -reproducido en las dos vueltas-. Eso
/// es una fila del MENU (perfil rapido / `--price`), no del defecto, y espera a que haya una
/// medida de reloj con suelo de ruido decente para decidirse.
/// ⇒ PARA REABRIRLO hace falta primero el instrumento: `taskset` a un nucleo aislado, n>=5 y
/// mediana, o la maquina en reposo. Sin eso, cualquier numero de velocidad de esta caja miente.
fn bits_efectivos(k: usize, tam: usize) -> usize {
    bits_para(TAB_BITS[k], tam).min(TAB_TOPE[k])
}

/// LOS PESOS DE CADA TIPO DE CORCHETE en la huella de anidamiento: parentesis, corchetes,
/// llaves, angulos.
///
/// PRIMOS Y DISTINTOS ENTRE SI, y eso NO es decoracion: si dos tipos pesaran igual, `[(` y `((`
/// producirian la misma huella y el modelo no podria distinguirlos. Con primos distintos hace
/// falta una combinacion muy improbable para colisionar.
///
/// ⚠️ La IDEA es de `nestModel` de paq8px (GPL, leido el 01-08 fuera del repo: leer si, copiar
/// no); SUS pesos son 31/11/17/23 y estan afinados para SU mezcla de modelos. Estos hay que
/// barrerlos aqui — hasta que se barran, son de clase «A OJO» en bench/constantes.tsv.
const PESOS_ANID: [i32; 4] = [31, 11, 17, 23];

/// Los dos interruptores de las vistas extra: (a) mitad si es joven, (d) contexto determinista.
/// `VC_VISTAS=0` las apaga las dos (DEFECTO: la medida las refuto), `=1` solo (a), `=2` solo
/// (d), `=3` las dos. Existen SEPARADAS porque hay que atribuir a cada una lo suyo: meter las dos y
/// publicar un solo numero es el error que este banco lleva todo el dia cazando.
/// ¿La ranura recuerda el ultimo byte y su racha? `VC_RACHA=0` lo apaga.
static RACHA_ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(||
    std::env::var("VC_RACHA").map(|v| v != "0").unwrap_or(true));

/// ¿La ranura recuerda tambien el 2º y 3º byte distintos? `VC_TRES=0` lo apaga.
///
/// 🚨 DESDE LA v6 EL DEFECTO ES QUE NO. Lo decide `tres_de_modelo`, o sea LA VERSION DEL ARCHIVO,
/// no el entorno: cambia la prediccion en los dos lados y si cada lado eligiera por su cuenta
/// divergirian en silencio. Comprobado el 03-08: comprimir con `VC_TRES=0` y extraer sin la
/// variable falla con rc=1 —lo caza el blake3 del archivador, pero el codec suelto no—, que es la
/// misma trampa del `VC_PERFIL`. Esta variable queda SOLO como herramienta de medida.
static TRES_ON: std::sync::LazyLock<Option<bool>> = std::sync::LazyLock::new(||
    std::env::var("VC_TRES").ok().map(|v| v != "0"));

/// ⚠️ LOS DOS MULTIPLICADORES DE LAS VISTAS NUEVAS, Y SON «A OJO» HASTA QUE SE BARRAN.
/// Me los invente al escribir la vista, que es exactamente lo que bench/constantes.tsv marca
/// como la clase peligrosa. Salen a perilla para poder barrerlos.
/// ⚠️ Y hay una forma MEJOR que cualquier constante, que es lo que hacen ellos: en vez de un
/// multiplicador fijo, meter la situacion discreta en un MAPA APRENDIDO. Ver VC_MAPA.
fn peso_racha() -> i32 {
    static E: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_W_RACHA").ok().and_then(|v| v.parse().ok()).unwrap_or(186))
}
fn peso_tres() -> i32 {
    static E: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_W_TRES").ok().and_then(|v| v.parse().ok()).unwrap_or(420))
}
/// Como se codifica la racha en la vista: 0 = logaritmica topada (lo que puse a ojo),
/// 1 = lineal topada, 2 = solo «hay racha o no». Para saber cual manda de verdad.
/// ¿La confianza de la racha la APRENDE un mapa, o la fija el multiplicador que me invente?
/// `VC_MAPA=0` vuelve al multiplicador para poder comparar las dos formas.
fn mapa_aprendido() -> bool {
    static E: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_MAPA").map(|v| v != "0").unwrap_or(true))
}
fn forma_racha() -> u32 {
    static E: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_F_RACHA").ok().and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// ⚠️ EL DEFECTO ES 0, Y ANTES ERA 3 CON LA MEDIDA YA EN CONTRA.
///
/// Se puso a 3 «mientras se miden» —asi lo decia el comentario de arriba— y la medida vino en
/// contra: las dos juntas cuestan +1,386% en codigo. Publique que quedaban «apagadas por
/// defecto» y no lo estaban: el defecto seguia en 3. Ninguna cifra del banco lo delataba porque
/// TODOS los guiones de medida pasaban VC_VISTAS=0 a mano; se vio al comprimir con el entorno
/// LIMPIO y salir 132.577 donde el banco decia 130.764.
///
/// La leccion no es «acuerdate de bajar el defecto». Son dos: un experimento encendido por
/// defecto se convierte en el producto en cuanto dejas de mirarlo; y un banco cuyos guiones fijan
/// TODAS las variables es ciego a un defecto equivocado, porque mide lo que le mandas y no lo que
/// el usuario se encuentra. De ahi que el carril lleve ahora una tirada con el entorno limpio.
///
/// Vuelven a encenderse si la prueba de convergencia con dato DISTINTO demuestra que pierden por
/// arrancar en frio y no por la idea. Hasta entonces la carga de la prueba la tiene quien enciende.
static VISTAS_EXTRA: std::sync::LazyLock<(bool, bool)> = std::sync::LazyLock::new(|| {
    let v: u32 = std::env::var("VC_VISTAS").ok().and_then(|x| x.parse().ok()).unwrap_or(0);
    (v & 1 != 0, v & 2 != 0)
});

const MEM_BITS: usize = 20; // cubos de la cadena ISSE


// ---------- mezclador de DOS CAPAS ----------
// Un solo mezclador tiene que elegir sus pesos con UN criterio. Pero "de quién fiarse" no
// depende de una sola cosa: a veces manda el byte que estamos construyendo, a veces el byte
// anterior (no es lo mismo ir detrás de un salto de línea que dentro de un número), a veces
// la posición del bit, a veces si el modelo de coincidencias está vivo.
//
// La respuesta no es elegir el mejor criterio: es tener VARIOS mezcladores, cada uno
// especializado por un criterio distinto, y un mezclador final que aprenda a cuál hacer caso
// en cada momento. El primero de los cuatro es EXACTAMENTE el mezclador que teníamos, así que
// la capa final no puede hacerlo peor que antes: en el peor caso aprende a copiarlo.
const NMIX: usize = 4;
/// Juegos de pesos de cada mezclador de primera capa (uno por valor de su selector).
///
/// Hasta la v9 esto era UN array constante. Desde la v10 depende de la version del modelo,
/// porque cambiar cuantos juegos hay CAMBIA LOS BYTES QUE SE EMITEN: un archivo v8 tiene que
/// seguir abriendose con el mezclador de 512 juegos que lo escribio.
const NSEL: [usize; NMIX] = [512, 256, 32, 256];

/// v10: el mezclador 0 pasa de 512 a 2048 juegos de pesos. Los otros tres no se tocan.
const NSEL_V10: [usize; NMIX] = [2048, 256, 32, 256];

/// ⭐ MAS JUEGOS DE PESOS EN EL MEZCLADOR 0 (v10). MEDIDO EL 04-08, aplicado hoy.
///
/// −0,221% de bytes en texto5, las CINCO muestras a favor: codigo −0,359% · logs −0,278% ·
/// sql −0,235% · csvjson −0,233% · enwik8 −0,162%. Ciclos +0,137%, que alternan de signo por
/// muestra: ruido, y tenia que serlo — el producto escalar NO cambia de longitud (siguen siendo
/// `NCTXP` = 160 carriles), solo cambia QUE juego de pesos se indexa. Cuesta memoria de pesos
/// (2048x160x4 B = 1,3 MB contra 328 KB) y practicamente ningun ciclo.
///
/// 🔑 DE DONDE SALEN LOS DOS BITS, Y POR QUE NO SON «LISTOS». El experimento buscaba otra cosa:
/// meter en el selector la EVIDENCIA de las casillas que predicen (el foco 2 de vnervio). Eso
/// dio −0,118%. Pero el CONTROL —los mismos 2 bits rellenos con dos bits de un byte de la
/// ventana, misma tabla, mismo coste, cero evidencia— dio −0,221%, casi el doble. Lo que pagaba
/// era LA RESOLUCION del selector, no el criterio. Se aplica lo que se midio, no lo que se
/// queria medir, y por eso los bits son `hist[5] >> 6` y no una señal elegida.
///
/// ⚠️ Y la evidencia ademas costaba el triple de ciclos (+0,506%), porque obligaba a recorrer
/// los ~20 modelos en CADA BIT para contarla. Esta version no cuenta nada: lee dos bits.
///
/// ⚠️ POR QUE ESTUVO CATORCE DIAS SIN APLICAR. La ficha del 04-08 lo dejo en PENDIENTE por un
/// motivo que ya no existe: «exige subir MODELO y meterlo en MODELOS_LEIBLES —son DOS listas y
/// meterlo en una sola ya produjo archivos ilegibles una vez». El 17-08 (18b8d3c) esas dos
/// listas se DERIVARON de `MODELO_MAX`, asi que hoy subir la version es una linea y no puede
/// desincronizarse. El obstaculo se levanto y nadie volvio a por la mejora.
fn nsel_de_modelo(modelo: u8) -> [usize; NMIX] {
    if modelo >= 10 { NSEL_V10 } else { NSEL }
}
/// Selector de la capa final: posición de bit x modelo de coincidencias activo. Pequeño a
/// propósito — solo 64 pesos en total, así converge en las primeras decenas de KB.
const NSEL2: usize = 16;
/// NCTX redondeado al múltiplo de 8 que le toca, para que el mezclador vaya de ocho en ocho
/// sin resto. Las ranuras de relleno arrancan a peso 0 y su incremento es (0*err)>>10 = 0,
/// así que se quedan a 0 y no aportan nada al producto escalar: son neutras EXACTAS, no
/// aproximadamente neutras.
const NCTXP: usize = (NCTX + 7) & !7;
/// Tamaño del registro del volcado de predicciones (VC_VOLCAR), en bytes y por bit:
///   0 bit · 1 bpos · 2 sel2 · 3 relleno · 4..12 sel[4] (u16) · 12..32 cx[5] (u32) · 32.. st (i16)
/// El relleno del byte 3 no es decorativo: deja `st` alineado a 32, que es lo que permite leer el
/// bloque entero de golpe en el replay sin copiar byte a byte.
const REG_VOLCADO: usize = 32 + 2 * NCTX;
/// «Las seis entradas de la cadena ISSE no cuestan nada» NO es una promesa mia: si alguien añade
/// una septima, NCTXP pasa de 160 a 168, el producto escalar crece un bloque SIMD por bit y esto
/// deja de compilar. Entonces habra que volver a medir el coste, porque ya no seria gratis.
const _: () = assert!(NCTXP == 160);

/// Bloque de memoria alineado a 2 MiB y marcado para paginas grandes.
///
/// Existe por una medicion: 57 fallos de TLB por byte. 1,2 GB en paginas de 4 KB son 300.000
/// paginas para un TLB de ~1.500 entradas, o sea que casi cada acceso paga una caminata de
/// tablas de pagina. En paginas de 2 MiB son 600 y caben enteras.
///
/// La clave, y es lo que hizo fallar el intento anterior EN SILENCIO: el nucleo solo puede
/// promover a pagina grande un rango que EMPIEZA en un limite de 2 MiB. Un Vec normal no lo
/// esta. madvise sobre un rango desalineado no da error — acepta y no hace nada.
struct Tabla {
    p: *mut u8,
    n: usize,
    /// reservada con mmap (y por tanto se suelta con munmap, no con el asignador)
    mapeada: bool,
}

const HUGE: usize = 2 << 20;

impl Tabla {
    fn nueva(n: usize) -> Tabla {
        // Las pequenas, por el asignador normal: alinear 16 bytes a 2 MiB gastaria 2 MiB.
        if n < HUGE {
            let lay = std::alloc::Layout::from_size_align(n.max(1), 64).expect("layout");
            let p = unsafe { std::alloc::alloc_zeroed(lay) };
            if p.is_null() { std::alloc::handle_alloc_error(lay); }
            return Tabla { p, n, mapeada: false };
        }
        #[cfg(target_os = "linux")]
        {
            // mmap: memoria a cero y SIN TOCAR. Es la diferencia con alloc_zeroed, que con
            // alineacion sobredimensionada hace el memset a mano y crea las paginas pequenas
            // antes de que podamos pedir las grandes.
            let n = (n + HUGE - 1) & !(HUGE - 1);   // redondear a 2 MiB
            let p = unsafe {
                mmap_anon(std::ptr::null_mut(), n, 3 /*RW*/, 0x22 /*PRIVATE|ANONYMOUS*/, -1, 0)
            };
            if !p.is_null() && p as isize != -1 {
                // MADV_HUGEPAGE = 14, ANTES del primer acceso: ahora el fallo de pagina si
                // puede resolverse con una pagina de 2 MiB.
                unsafe { libc_madvise(p, n, 14) };
                if std::env::var("VC_HUGE_DIAG").is_ok() {
                    eprintln!("[huge] mmap dir=0x{:x} alineada={} tam={}",
                        p as usize, (p as usize) % HUGE == 0, n);
                }
                return Tabla { p, n, mapeada: true };
            }
        }
        // Si mmap no esta o falla, el camino de siempre: mas lento, igual de correcto.
        let lay = std::alloc::Layout::from_size_align(n.max(1), 64).expect("layout");
        let p = unsafe { std::alloc::alloc_zeroed(lay) };
        if p.is_null() { std::alloc::handle_alloc_error(lay); }
        Tabla { p, n, mapeada: false }
    }
}

#[cfg(target_os = "linux")]
unsafe fn libc_madvise(p: *mut u8, n: usize, adv: i32) -> i32 {
    unsafe extern "C" { fn madvise(addr: *mut core::ffi::c_void, len: usize, advice: i32) -> i32; }
    unsafe { madvise(p as *mut core::ffi::c_void, n, adv) }
}

#[cfg(target_os = "linux")]
unsafe fn mmap_anon(a: *mut u8, n: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8 {
    unsafe extern "C" {
        fn mmap(addr: *mut core::ffi::c_void, len: usize, prot: i32, flags: i32,
                fd: i32, off: i64) -> *mut core::ffi::c_void;
    }
    unsafe { mmap(a as *mut core::ffi::c_void, n, prot, flags, fd, off) as *mut u8 }
}

#[cfg(target_os = "linux")]
unsafe fn munmap_(p: *mut u8, n: usize) {
    unsafe extern "C" {
        fn munmap(addr: *mut core::ffi::c_void, len: usize) -> i32;
    }
    unsafe { munmap(p as *mut core::ffi::c_void, n) };
}

impl Drop for Tabla {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if self.mapeada { unsafe { munmap_(self.p, self.n) }; return; }
        let lay = std::alloc::Layout::from_size_align(self.n.max(1), 64).unwrap();
        unsafe { std::alloc::dealloc(self.p, lay) };
    }
}

impl std::ops::Deref for Tabla {
    type Target = [u8];
    fn deref(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.p, self.n) } }
}
impl std::ops::DerefMut for Tabla {
    fn deref_mut(&mut self) -> &mut [u8] { unsafe { std::slice::from_raw_parts_mut(self.p, self.n) } }
}
// El predictor vive dentro de un hilo del pool y no se comparte: el puntero crudo no lo hace
// menos seguro de lo que ya era un Vec.
unsafe impl Send for Tabla {}

/// Producto escalar de los pesos por las predicciones estiradas, en i64.
///
/// La versión AVX2 y la escalar dan EL MISMO i64, no uno parecido: los productos van a 64
/// bits sin desbordar y la suma de enteros es asociativa. Es lo que permite vectorizar en
/// un compresor que promete el mismo byte en x86 y en ARM.
#[inline]
fn punto(w: &[i32], st: &[i32; NCTXP], simd: bool) -> i64 {
    #[cfg(target_arch = "x86_64")]
    if simd {
        return unsafe { punto_avx2(w, st) };
    }
    let _ = simd;
    let mut d: i64 = 0;
    for i in 0..NCTXP { d += (w[i] as i64) * (st[i] as i64); }
    d
}

/// Un paso de descenso de gradiente sobre los pesos de un mezclador de capa 1.
#[inline]
fn actualiza_pesos(w: &mut [i32], st: &[i32; NCTXP], err: i32, simd: bool, dec: u32, lr: u32) {
    #[cfg(target_arch = "x86_64")]
    if simd {
        unsafe { actualiza_avx2(w, st, err, dec, lr) };
        return;
    }
    let _ = simd;
    if dec == 0 {
        for i in 0..NCTXP {
            let dw = (st[i] * err) >> lr;
            w[i] = (w[i] + dw).clamp(-(1 << 20), 1 << 20);
        }
    } else {
        // Decaimiento hacia cero. El desplazamiento aritmetico de un negativo redondea hacia
        // -infinito, asi que el decaimiento es simetrico en magnitud para ambos signos: un peso
        // positivo y su opuesto se acercan a cero al mismo ritmo. Importa para el determinismo
        // tanto como para que la regularizacion no tenga sesgo.
        for i in 0..NCTXP {
            let dw = (st[i] * err) >> lr;
            let v = w[i];
            let d = if v >= 0 { v >> dec } else { -((-v) >> dec) };
            w[i] = (v - d + dw).clamp(-(1 << 20), 1 << 20);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn punto_avx2(w: &[i32], st: &[i32; NCTXP]) -> i64 {
    use std::arch::x86_64::*;
    let mut acc = _mm256_setzero_si256();
    let mut i = 0;
    while i < NCTXP {
        let a = _mm256_loadu_si256(w.as_ptr().add(i) as *const __m256i);
        let b = _mm256_loadu_si256(st.as_ptr().add(i) as *const __m256i);
        // mul_epi32 coge las ranuras PARES de 32 bits, les extiende el signo y da 4 productos
        // de 64 bits. Para las impares las bajo a la posición par con un desplazamiento de 32:
        // los 32 bits de arriba quedan a cero pero mul_epi32 ni los mira.
        acc = _mm256_add_epi64(acc, _mm256_mul_epi32(a, b));
        acc = _mm256_add_epi64(acc, _mm256_mul_epi32(_mm256_srli_epi64(a, 32), _mm256_srli_epi64(b, 32)));
        i += 8;
    }
    let mut t = [0i64; 4];
    _mm256_storeu_si256(t.as_mut_ptr() as *mut __m256i, acc);
    t[0] + t[1] + t[2] + t[3]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn actualiza_avx2(w: &mut [i32], st: &[i32; NCTXP], err: i32, dec: u32, lr: u32) {
    use std::arch::x86_64::*;
    unsafe {
        let e = _mm256_set1_epi32(err);
        let lo = _mm256_set1_epi32(-(1 << 20));
        let hi = _mm256_set1_epi32(1 << 20);
        let mut i = 0;
        while i < NCTXP {
            let a = _mm256_loadu_si256(w.as_ptr().add(i) as *const __m256i);
            let b = _mm256_loadu_si256(st.as_ptr().add(i) as *const __m256i);
            let dw = _mm256_sra_epi32(_mm256_mullo_epi32(b, e), _mm_cvtsi32_si128(lr as i32));
            // decaimiento simetrico: |w| >> dec con el signo de w. Da EXACTAMENTE lo mismo que
            // la version escalar —lo que permite vectorizar en un compresor que promete el
            // mismo byte en x86 y en ARM.
            let v = if dec == 0 {
                a
            } else {
                let neg = _mm256_cmpgt_epi32(_mm256_setzero_si256(), a);
                let abs = _mm256_abs_epi32(a);
                let d = _mm256_srl_epi32(abs, _mm_cvtsi32_si128(dec as i32));
                let dsig = _mm256_sub_epi32(_mm256_xor_si256(d, neg), neg); // d con signo de a
                _mm256_sub_epi32(a, dsig)
            };
            let mut r = _mm256_add_epi32(v, dw);
            r = _mm256_max_epi32(r, lo);
            r = _mm256_min_epi32(r, hi);
            _mm256_storeu_si256(w.as_mut_ptr().add(i) as *mut __m256i, r);
            i += 8;
        }
    }
}

/// Nombres de los 26 modelos, para que el censo se pueda leer.
const NOMBRES: [&str; IRACHA] = [
    "orden-0", "orden-1", "orden-2", "orden-3", "orden-4", "orden-5", "palabra", "orden-8",
    "sparse-d2", "sparse-d1+3", "indirecto-1", "columna x sep", "encima x col", "encima x byte",
    "orden-12", "pareja-palabra", "pareja-minus", "columna2 x sep2", "encima2 x col2",
    "encima2 x byte", "forma-texto", "forma-texto-larga", "sparse-paso4", "sparse-paso8",
    "sparse-paso2y4", "indirecto-2",
    "num:digito-encima", "num:largo", "num:delta", "num:prefijo",
    "txt:trigrama", "txt:raiz", "txt:trigrama-raices", "txt:skip-grama",
    "anidamiento", "anidamiento x byte",
    "DMC", "match-6", "match-12", "cadena ISSE", "match-disp-2", "match-disp-4", "match-disp-8",
    "no-lineal:dispersion", "no-lineal:disp x media",
];

/// Los nombres de las entradas «determinista», una por contexto de tabla. Se generan del nombre
/// del contexto que refinan para que el censo se siga leyendo.
fn nombre_ctx(i: usize) -> String {
    if i < IRACHA { NOMBRES[i].to_string() }
    else if i < ITRES { format!("racha:{}", NOMBRES[i - IRACHA]) }
    else if i < IDET { format!("tres:{}", NOMBRES[i - ITRES]) }
    else if i < IISSE_INT { format!("det:{}", NOMBRES[i - IDET]) }
    // La etapa k de la cadena refina ISSE_CTX[k]: se nombra por el contexto que refina, no por
    // el numero de etapa, que no le dice nada a nadie.
    else { format!("isse:{}", NOMBRES[ISSE_CTX[i - IISSE_INT]]) }
}

/// Una etapa de calibración (APM/SSE): coge una probabilidad y la corrige según un contexto,
/// interpolando entre 33 puntos de la curva. Arranca en la identidad —no corrige nada— y va
/// aprendiendo qué sesgo tiene el predictor en cada contexto.
struct Apm {
    t: Vec<u16>,
    idx: usize,
}

impl Apm {
    fn nueva(n_contextos: usize, tb: &Tables) -> Apm {
        let mut t = vec![0u16; n_contextos * 33];
        for i in 0..t.len() {
            let x = (i % 33) as i32 * 128 - 2048;
            t[i] = (tb.squash(x) * 16) as u16;
        }
        Apm { t, idx: 0 }
    }

    #[inline]
    fn p(&mut self, pr: i32, cx: usize, tb: &Tables) -> i32 {
        let s = (tb.stretch(pr) + 2048).clamp(0, 4095);
        let lo = (s >> 7) as usize;
        let w = s & 127;
        self.idx = (cx * 33 + lo).min(self.t.len() - 2);
        let a = self.t[self.idx] as i32;
        let b = self.t[self.idx + 1] as i32;
        (a * (128 - w) + b * w) >> 11
    }

    #[inline]
    fn update(&mut self, bit: i32) {
        let g = bit * 65535;
        let a = self.t[self.idx] as i32;
        self.t[self.idx] = (a + ((g - a) >> 6)) as u16;
        let b = self.t[self.idx + 1] as i32;
        self.t[self.idx + 1] = (b + ((g - b) >> 6)) as u16;
    }
}

struct Predictor<'a> {
    t: &'a Tables,
    // por cada modelo: un ESTADO de historia de bits por ranura de contexto (1 byte, contra
    // los 4 que gastaba guardar probabilidad+conteo) ...
    estados: Vec<Tabla>,
    /// LA MEMORIA DE BYTE, en paralelo a `estados`: DOS bytes por cubo —el ultimo byte visto
    /// detras de ese contexto y cuantas veces seguidas—. Va aparte y no dentro del cubo de 16
    /// para no tocar el formato de `busca_cubo`, que ya esta medido y probado.
    memo: Vec<Tabla>,
    /// El cubo del PRIMER nibble de cada contexto. La memoria de byte es del BYTE, no del nibble:
    /// el cubo del segundo nibble depende del valor del primero, o sea que ya es otro contexto.
    bucket0: [u32; NTAB],
    /// máscara de cubos de cada modelo, que ahora depende del tamaño del bloque
    tmask: [u32; NTAB],
    imask: u32,
    /// Modelo INDIRECTO: para cada byte, los 4 últimos bytes que le siguieron.
    ind1: Vec<u32>,
    /// Indirecto de orden 2: lo que vino detrás de cada PAREJA de bytes. Más específico que
    /// el de orden 1 y, en datos con estructura, mucho más certero.
    ind2: Vec<u32>,
    /// Filas por SEPARADOR. El primero es el salto de línea y manda mientras las líneas
    /// tengan un tamaño razonable; los otros están para cuando no (volcados SQL con una sola
    /// sentencia de un mega, JSON en una línea, CSV sin saltos).
    sep_last: [u32; NSEP], sep_prev: [u32; NSEP], sep_n: [u32; NSEP],
    // ---- modelo numerico: memoria POR CAMPO, no por posicion de byte ----
    /// indice de campo dentro del registro en curso (cuantos separadores de campo llevamos)
    campo: u32,
    /// digitos emitidos del numero en curso; 0 = no estamos dentro de un numero
    nd: u32,
    /// valor que se esta construyendo
    num_cur: u64,
    /// valor, largo y salto del MISMO campo en el registro anterior
    num_ant: Vec<u64>, num_len: Vec<u8>, num_delta: Vec<i64>,
    /// que separador manda como registro y cual como campo (los calcula set_byte_contexts)
    e_rec: usize, e_campo: usize,
    /// Respaldo para datos binarios sin saltos de línea: ancho de rejilla deducido de que la
    /// distancia entre apariciones de un mismo byte se repita tres veces seguidas.
    cpos1: Vec<u32>, cpos2: Vec<u32>, cpos3: Vec<u32>,
    rlen: u32, rlen1: u32, rlen2: u32, rcount1: u32, rcount2: u32,
    // ... y un StateMap de 256 entradas COMPARTIDO por todos los contextos del modelo, que
    // es quien traduce estado -> probabilidad y quien de verdad aprende.
    /// StateMap de TODOS los modelos en un solo array: `sm[i*256+e] = (n<<16) | p`.
    ///
    /// Antes eran dos `Vec<Vec<u16>>`: por cada modelo y por cada bit había que perseguir dos
    /// cabeceras de Vec, comprobar dos veces los límites y tocar dos líneas de caché para leer
    /// dos números que siempre se usan juntos. Empaquetados en un u32 vienen en la misma
    /// línea, y con NTAB*256 entradas el array entero son 26 KB: cabe en L1.
    sm: Vec<u32>,
    /// EL MAPA DE LA RACHA. Misma mecanica que `sm` —visitas arriba, probabilidad abajo— pero
    /// indexado por (contexto, situacion de racha) en vez de por (contexto, estado de bits).
    /// Existe para que la confianza que merece una racha LA APRENDA EL DATO en vez de fijarla
    /// un multiplicador elegido a mano.
    run_sm: Vec<u32>,
    /// Que casilla de `run_sm` se consulto en este bit, por contexto. `usize::MAX` = no se
    /// consulto —no habia racha o no encajaba— y entonces no se actualiza: actualizar una
    /// casilla que no se uso la envenena con la respuesta de otra situacion.
    run_ix: [usize; NTAB],
    /// El mapa de los dos candidatos alternos, misma mecanica. 64 situaciones por contexto:
    /// 16 combinaciones de los dos votos x 4 cubos de posicion de bit.
    tres_sm: Vec<u32>,
    tres_ix: [usize; NTAB],
    est: [u8; NCTX],
    // Cadena ISSE. Cada etapa no vota en paralelo: REFINA la predicción que le llega,
    // usando un contexto más largo. Los pesos van indexados por el ESTADO de historia de
    // bits (no por el contexto): el estado es justo quien sabe cuánto fiarse de la entrada.
    isse_est: Vec<Tabla>,
    isse_w: Vec<Vec<i32>>,
    isse_idx: [u32; NISSE],
    isse_s: [u8; NISSE],
    isse_pin: [i32; NISSE],
    isse_pout: [i32; NISSE],

    dmc: Dmc,
    dt: Vec<i32>,           // dt[n] = 65536/(n+2): paso de aprendizaje por nº de observaciones
    /// VC_LIM: tope de observaciones del StateMap (por defecto 1023, el maximo que cabe).
    lim: usize,
    /// VC_ISSE_LR: cuanto corrige cada etapa de la cadena ISSE. Por defecto 6, Y SE QUEDA EN 6.
    ///
    /// ✅ MEDIDO EL 02-08, y el resultado es un NEGATIVO que vale mas que un positivo porque
    /// enseña donde no hay que fiarse del banco.
    ///
    /// El barrido de `bench/mezclador.sh` sobre 512 KB decia que 20 ganaba —0,244% en codigo,
    /// −0,336% en sql— y ademas la curva SE DABA LA VUELTA en 20-24, que es justo la señal que
    /// valida un barrido (la que le faltaba al del CDC). Todo apuntaba a un cambio bueno.
    ///
    /// Medido en serio, a 8 MiB y en los seis tipos, sale AL REVES:
    ///
    /// ```text
    ///     codigo +0,186%   csvjson +0,088%   logs +0,443%
    ///     sql    +0,034%   bin     +0,033%   pdfoffice −0,005%      TOTAL +0,028%
    /// ```
    ///
    /// Pierde en cinco de seis. 🔑 Y no es ruido: es que ESTA TASA DEPENDE DEL TAMAÑO. Con poco
    /// dato, adaptarse rapido compensa; con mucho, el paso largo mete ruido sobre estadistica que
    /// ya estaba bien estimada. Es la misma clase de dependencia que `PODA_DESDE` reconoce para
    /// las tablas — solo que ahi esta escrita y aqui no lo estaba.
    ///
    /// ⚠️ Un barrido de 512 KB no puede decidir sobre esta perilla, y por eso mezclador.sh barre
    /// ahora en DOS tamaños y avisa cuando el ganador cambia entre ellos.
    isse_lr: i32,
    /// VC_MIX_LR: desplazamiento del paso del mezclador. Mas alto = paso mas corto.
    ///
    /// ⚠️ EL COMENTARIO DECIA «por defecto 10» Y ES FALSO DESDE LA v3. No hay un defecto fijo: sale
    /// de `tasas_de_modelo(modelo).1`, que da 10 en v1/v2 y **12** de la v3 en adelante — o sea que
    /// hoy es 12. La frase se quedo escrita de cuando el 10 era verdad y nadie la volvio a mirar.
    ///
    /// Lo destapo `bench/mezclador.sh` en su primera ejecucion (02-08) y no por listo: porque
    /// compara contra el binario TAL CUAL SE EJECUTA, no contra el defecto declarado. El barrido
    /// puso `10 :+2,709%` en codigo y `+5,147%` en sql — es decir, la cifra que yo habia escrito
    /// como «el defecto» costaba un 5% frente al defecto de verdad.
    mix_lr: u32,
    cxt: [u32; NCTX],       // hash base de cada modelo para el byte actual
    idx: [u32; NCTX],       // ranura final dentro del cubo
    bucket: [u32; NTAB],    // cubo activo de cada modelo (se elige una vez por nibble)
    isse_bucket: [u32; NISSE],
    nib: u32,               // bits vistos del nibble en curso, como valor
    jnib: u32,              // cuántos son
    st: [i32; NCTXP],       // stretch de cada predicción (para el update del mixer)
    /// VC_DECAY=D: los pesos del mezclador decaen w>>D en cada actualizacion. 0 = apagado.
    decay: u32,
    /// VC_DISP=0 apaga la parte no lineal, para poder ablacionarla.
    disp_on: bool,
    /// ¿Estan IDISP/IDISP2 en la mascara de apagados? Si lo estan, calcular la dispersion es
    /// tirar el trabajo: el bucle de ablacion la borra a cero unas lineas mas abajo. Se resuelve
    /// UNA VEZ en el constructor y no en cada bit, que es donde se notaria mirarlo.
    disp_apagada: bool,
    /// ¿Se calcula «los tres distintos»? Lo decide la VERSION DEL ARCHIVO (`tres_de_modelo`), no
    /// el entorno, por el mismo motivo que `mm_verif`: los dos lados tienen que coincidir.
    tres_on: bool,
    /// ¿comprueba el `match` que la coincidencia es real, o se fía del hash? Sale de la VERSION
    /// DE MODELO, no de una constante: los dos lados tienen que hacer lo mismo.
    mm_verif: bool,
    /// Máscara de modelos apagados por el perfil.
    apagados: u64,
    /// VC_COSTEPOS=<fichero>: un f32 por byte de entrada con lo que costó codificarlo.
    /// Diagnóstico puro: no toca el predictor, sólo observa.
    costepos: Option<std::io::BufWriter<std::fs::File>>,
    /// bits acumulados del byte en curso, y cuántos llevamos
    cb: f64,
    cbn: u32,
    /// Censo activo (VC_CENSO=1). Fuera de él no se ejecuta nada de esto.
    censo: bool,
    /// VC_VOLCAR=<fichero> · EL VOLCADO DE PREDICCIONES, que es lo que hace barata la busqueda.
    ///
    /// POR QUE SE PUEDE. Ningun submodelo ve nunca la prediccion final: `self.pr` aparece en tres
    /// sitios y solo en tres —devolverla, la traza de coste, y `errf` para los pesos del SSE—. Los
    /// StateMap aprenden de `target = bit * 65535`; cada mezclador de capa 1 de SU PROPIA salida
    /// (`mp[k]`); la capa 2 de la suya (`mixer_p`); la cadena ISSE de la suya (`isse_pout[k]`). O
    /// sea que la trayectoria de `st[]` NO DEPENDE de como este configurado lo que hay aguas
    /// abajo: mezclador, calibracion y pesos pueden cambiar y `st[]` sale igual.
    ///
    /// Y de ahi la consecuencia util: volcando `st[]` UNA vez, cualquier configuracion aguas
    /// abajo se puede reproducir sin volver a correr los modelos —que es donde se va el tiempo—.
    /// El resultado no es una aproximacion ni un sustituto: `vc-replay` llama a ESTAS MISMAS
    /// funciones, asi que da EL MISMO numero de bits, y eso se comprueba.
    ///
    /// ⚠️ LO QUE ESTE VOLCADO NO CUBRE, Y HAY QUE DECIRLO. Se escribe desde `update`, o sea con
    /// `st[]` YA ablacionado y con la cadena ISSE YA resuelta. Por tanto sirve para VC_MIX_LR,
    /// VC_DECAY, VC_SSE y los pesos `sse_w`; NO sirve para VC_ISSE_LR (mueve `st[IISSE]`), ni
    /// para mascaras de ablacion, ni para nada que cambie los modelos. Eso sigue costando corrida
    /// entera. Un atajo que no dice donde acaba es como el que elegia configuraciones nunca
    /// corridas.
    volcar: Option<std::io::BufWriter<std::fs::File>>,
    /// Los contextos del SSE de este bit. Se calculan en `p()` y se vuelcan desde `update()`, que
    /// es donde se conoce el bit; guardarlos es mas barato que partir el registro en dos.
    cx_v: [u32; 5],
    /// VC_CORR=1 · ¿cuánto se pisan los modelos entre sí? (M3)
    ///
    /// Las cinco sumas de Pearson, acumuladas EN LÍNEA. Volcar las 43 predicciones por bit
    /// serían 275 GB sobre 200 MiB de entrada; estas sumas son 15 KB y dan el mismo número
    /// exacto. Se acumulan sobre `st[i]`, la predicción ESTIRADA, que es el dominio del
    /// mezclador: en probabilidad, dos modelos seguros y de acuerdo se pegan al 1 y la
    /// correlación sale inflada por el techo.
    corr: bool,
    corr_n: f64,
    corr_s: Vec<f64>,
    corr_sp: Vec<f64>,
    /// Coste en bits de cada modelo SI CODIFICARA ÉL SOLO. Diagnóstico, nunca en el camino
    /// de la probabilidad.
    coste: Vec<f64>,
    bits_vistos: u64,
    /// ¿Esta máquina tiene AVX2? Se mira UNA vez, no por bit. La respuesta cambia la
    /// velocidad y no cambia ni un byte de la salida — eso está verificado, no supuesto.
    simd: bool,
    // mezclador de dos capas: pesos 16.16
    w: Vec<Vec<i32>>,       // capa 1: un vector de pesos por mezclador
    sel: [usize; NMIX],     // selector activo de cada mezclador de capa 1
    mp: [i32; NMIX],        // salida de cada mezclador de capa 1 (dominio p, 0..4095)
    mst: [i32; NMIX],       // esa misma salida en dominio stretch: la entrada de la capa 2
    w2: Vec<i32>,           // capa 2
    sel2: usize,
    // APM/SSE final
    /// Cadena de calibración. La primera etapa es la que había; las demás la refinan con
    /// contextos distintos. Cuántas se usan lo dice VC_SSE (por defecto todas).
    sse: Vec<Apm>,
    n_sse: usize,
    /// Peso de cada etapa, por (etapa, posicion de bit). 65536 = hacerle caso del todo,
    /// 0 = ignorarla. Arranca en 3/4, que es la mezcla fija que habia antes.
    sse_w: Vec<i32>,
    /// Entrada y salida de cada etapa este bit, para el gradiente.
    sse_in: [i32; 8],
    sse_out: [i32; 8],
    mixer_p: i32,   // salida de la capa 2 ANTES del APM (el error del mixer se mide sobre esto)
    /// Cuantos juegos de pesos tiene cada mezclador de capa 1 EN ESTE archivo. Es un campo y no
    /// la constante porque depende de la version del modelo (v10 le da 2048 al mezclador 0), y
    /// tanto comprimir como descomprimir tienen que usar el mismo o los pesos no cuadran.
    nsel: [usize; NMIX],
    /// v10: hay dos bits mas en el selector del mezclador 0. Derivado del modelo una sola vez,
    /// igual que `salta`, para no volver a preguntarlo en cada bit.
    mas_juegos: bool,
    // historia
    hist: [u8; 8],
    word: u32,      // hash del run alfanumérico actual (contexto de palabra)
    word_prev: u32, // la palabra COMPLETA anterior: sin ella no hay parejas
    word_prev2: u32, // y la de antes: sin ella no hay TRIGRAMAS
    /// letras del run en curso, para poder sacarle la raiz cuando se cierre
    wbuf: [u8; WMAX], wlen: usize,
    /// raices de las dos palabras completas anteriores
    raiz_prev: u32, raiz_prev2: u32,
    /// Palabra en minúsculas y sólo letras. «The», «the» y «THE» pasan a ser el MISMO
    /// contexto, que es lo que son para predecir lo que viene detrás.
    wmin: u32,
    wmin_prev: u32,
    /// La FORMA del texto: cada byte se reduce a su clase —letra, dígito, espacio, signo— y
    /// las repeticiones se aplastan, así que "Pedro   123" y "Ana 7" comparten la forma
    /// `A a 0`. Un modelo de orden N ve bytes concretos; este ve el MOLDE, y el molde se
    /// repite muchísimo más que el contenido: cabeceras, tablas, listas de parámetros,
    /// líneas de log, declaraciones de variables.
    cgrp: u64,
    /// LA HUELLA DE LA PILA DE ANIDAMIENTO. Un solo entero que sube al abrir y baja al cerrar,
    /// con un peso PRIMO Y DISTINTO por tipo de corchete: asi el valor dice a la vez CUANTO
    /// hemos anidado y CON QUE. Si dos tipos pesaran igual, `[(` y `((` darian lo mismo.
    ///
    /// No se acota ni se satura a proposito: un fichero con los corchetes desbalanceados —que
    /// los hay, y muchos: plantillas, fragmentos, binarios— produce una deriva, y esa deriva es
    /// INFORMACION («llevamos rato dentro de algo que no cierra»), no un fallo. Lo que se hashea
    /// son los bits bajos, asi que la deriva no rompe nada.
    anid: i32,
    c0: u32,     // byte en construcción (1 + bits vistos)
    bpos: u32,
    /// PERILLA DE BANCO (F6·Q3). Primera posicion de `mm_buf` que se puede LEER.
    ///
    /// Vale 0 siempre salvo con `VC_CEBA` puesto y `VC_CEBA_VENTANA=0`, o sea que fuera del
    /// banco todas las guardas que la miran son `x >= 0` y no cambian ni un byte. Es la unica
    /// forma de separar las DOS cosas que hoy se suman en el +1,4% de partir en frames: las
    /// TABLAS frias (que el cebado arregla) y las COINCIDENCIAS que cruzan la frontera (que solo
    /// arregla dar historia). Con la ventana cerrada el modelo llega entrenado pero NO puede leer
    /// un solo byte del frame anterior; con la ventana abierta, si.
    vent_min: usize,
    // match model
    mm_buf: Vec<u8>,
    mm_hashtab: Vec<u32>,
    mm_ptr: u32,
    mm_len: u32,
    mm_pred_bit: i32,
    mm2_hashtab: Vec<u32>,
    mm2_ptr: u32,
    mm2_len: u32,
    mm2_pred_bit: i32,
    /// Match model DISPERSO: la clave son seis bytes tomados de dos en dos. Engancha donde
    /// los otros dos no pueden — registros binarios, donde lo que se repite es el campo y no
    /// la retahíla.
    mm3_hashtab: Vec<u32>,
    mm3_ptr: u32,
    mm3_len: u32,
    mm3_pred_bit: i32,
    /// Zancadas 4 y 8, detrás de VC_MMEXTRA mientras se miden.
    mm_extra: u32,
    mm4_hashtab: Vec<u32>, mm4_ptr: u32, mm4_len: u32, mm4_pred_bit: i32,
    mm8_hashtab: Vec<u32>, mm8_ptr: u32, mm8_len: u32, mm8_pred_bit: i32,
    /// F6·Q6 · ¿esta VERSION DE MODELO salta? Sale de `salta_de_modelo`, o sea del byte que
    /// viaja en el archivo, igual que `mm_verif`: los dos lados tienen que coincidir.
    salta: bool,
    /// ¿se esta saltando EL BYTE EN CURSO? Se fija al empezarlo y no cambia hasta el siguiente.
    salto: bool,
    /// La unica etapa que corre en un byte saltado. Calibra la prediccion cruda del match.
    sse_salto: Apm,
    /// F6·Q6, solo con `--features cobertura`: la foto del estado del match AL EMPEZAR el byte
    /// que se esta codificando. Se toma al final de `set_byte_contexts` y se cobra al cerrarse el
    /// byte, que es cuando se sabe si el match acerto. No entra en ninguna prediccion.
    #[cfg(feature = "cobertura")]
    cob_racha: u32,
    #[cfg(feature = "cobertura")]
    cob_racha5: u32,
    #[cfg(feature = "cobertura")]
    cob_pred: i32,
    pr: i32,
}

/// Busca el cubo de este contexto dentro de su conjunto de CUATRO.
///
/// Con un solo cubo por hash, el contexto que llega echa al que estaba, sea quien sea: se
/// desaloja igual de rápido uno recién nacido que uno que lleva mil observaciones y predice
/// bien. Con cuatro vías hay dónde elegir, y se echa al que MENOS sabe.
///
/// Cuatro cubos de 16 bytes son 64: exactamente una línea de caché. Así que mirar en cuatro
/// sitios en vez de en uno **no cuesta ni un fallo de memoria más** — es el mismo acceso.
#[inline]
fn busca_cubo(tabla: &mut [u8], conj: usize, firma: u8, t: &Tables) -> u32 {
    let mut peor = conj;
    let mut menos = u16::MAX;
    for v in 0..4 {
        let b = conj + (v << 4);
        if tabla[b] == firma { return b as u32; }
        // "Cuánto sabe" un cubo = cuántas observaciones acumula su estado raíz.
        let e = tabla[b + 1] as usize;
        let info = t.h.n0[e] as u16 + t.h.n1[e] as u16;
        if info < menos { menos = info; peor = b; }
    }
    for k in 0..16 { tabla[peor + k] = 0; }
    tabla[peor] = firma;
    peor as u32
}

impl<'a> Predictor<'a> {
    fn new(t: &'a Tables, tam: usize, familia: u8, modelo: u8, perfil: u8) -> Self {
        let extra: u32 = std::env::var("VC_MMEXTRA").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        // UNA sola mascara para todo. Si la reserva de tablas y el mezclador usaran mascaras
        // distintas, un modelo "apagado" tendria tabla de 16 bytes y el otro lado lo
        // indexaria como si fuera entera: eso no es peor ratio, es leer fuera de sitio.
        //
        // 🚨 EL PERFIL LLEGA POR ARGUMENTO, NO DEL ENTORNO. Leerlo aqui dentro significaba que
        // el descompresor consultaba SU entorno en vez del archivo: comprimir con VC_PERFIL=2 y
        // descomprimir sin la variable daba bytes distintos, con exit 0. Ver `perfil_a_escribir`.
        let apag_total = apagados_de_perfil(perfil as u32, modelo)
            | apagados_de_modelo(modelo, tam)
            | familia_apagados(familia) | num_apagados() | txt_apagados() | apaga_suelto() | apaga_mascara();
        // El reparto de juegos se resuelve AQUI, una vez, porque de el depende el tamaño de los
        // vectores de pesos que se reservan tres lineas mas abajo.
        let nsel = nsel_de_modelo(modelo);
        Predictor {
            t,
            nsel,
            mas_juegos: modelo >= 10,
            estados: {
                // Un modelo apagado no necesita tabla. Son ~324 MB con entradas de 10 MB.
                let apag = apag_total;
                (0..NTAB).map(|k| {
                    if apag >> k & 1 != 0 { return Tabla::nueva(16); }
                    Tabla::nueva((1usize << bits_efectivos(k, tam)) * 16)
                }).collect()
            },
            memo: {
                let apag = apag_total;
                (0..NTAB).map(|k| {
                    if apag >> k & 1 != 0 || !*RACHA_ON { return Tabla::nueva(4); }
                    // 4 bytes por cubo contra los 16 del estado: +25% de memoria de tablas.
                    // [0]=ultimo byte  [1]=racha  [2]=2º distinto  [3]=3º distinto
                    Tabla::nueva((1usize << bits_efectivos(k, tam)) * 4)
                }).collect()
            },
            bucket0: [0; NTAB],
            // La máscara cuenta CONJUNTOS de 4 cubos, no cubos sueltos: mismo gasto de
            // memoria, cuatro sitios donde caber en vez de uno.
            tmask: {
                let mut m = [0u32; NTAB];
                // el MISMO tope que la reserva: si la máscara fuera mayor, indexaría fuera
                for k in 0..NTAB { m[k] = (1u32 << (bits_efectivos(k, tam) - 2)) - 1; }
                m
            },
            imask: (1u32 << (bits_para(MEM_BITS, tam) - 2)) - 1,
            ind1: vec![0u32; 256],
            ind2: vec![0u32; 1 << 16],
            sep_last: [0; NSEP], sep_prev: [0; NSEP], sep_n: [0; NSEP],
            campo: 0, nd: 0, num_cur: 0,
            num_ant: vec![0; NCAMPO], num_len: vec![0; NCAMPO], num_delta: vec![0; NCAMPO],
            e_rec: 0, e_campo: 0,
            cpos1: vec![0u32; 256], cpos2: vec![0u32; 256], cpos3: vec![0u32; 256],
            rlen: 0, rlen1: 0, rlen2: 0, rcount1: 0, rcount2: 0,
            // 256 situaciones por contexto: 16 cubos de racha x 4 de posicion de bit x 2 de
            // «¿difiere el 2º?» x 2 del bit predicho. Arranca a mitad y sin visitas.
            run_sm: vec![1u32 << 15; NTAB * 256],
            run_ix: [usize::MAX; NTAB],
            tres_sm: vec![1u32 << 15; NTAB * 64],
            tres_ix: [usize::MAX; NTAB],
            sm: {
                // n arranca a 0 (los 16 de arriba), p al prior del estado (los 16 de abajo).
                let mut v = vec![0u32; NTAB * 256];
                for i in 0..NTAB { for e in 0..256 { v[(i << 8) | e] = t.h.prior(e) as u32; } }
                v
            },
            est: [0; NCTX],
            isse_est: (0..NISSE).map(|_| Tabla::nueva((1usize << bits_para(MEM_BITS, tam)) * 16)).collect(),
            // peso inicial: w0 = 1.0 en 16.16 (deja pasar la predicción tal cual) y sesgo 0.
            // Así una etapa nueva NO estropea lo que ya venía bien; solo puede mejorarlo.
            isse_w: (0..NISSE).map(|_| (0..256).flat_map(|_| [1 << 16, 0]).collect()).collect(),
            isse_idx: [0; NISSE], isse_s: [0; NISSE],
            isse_pin: [0; NISSE], isse_pout: [0; NISSE],
            dmc: Dmc::new(),
            dt: (0..1024).map(|n| 65536 / (n as i32 + 2)).collect(),
            // Se acota a 1023 porque `dt` tiene 1024 entradas: un VC_LIM mayor indexaria
            // fuera. El limite del array es el limite de verdad, no una opinion.
            // El defecto sale de la VERSION DE MODELO; las variables de entorno solo estan
            // para medir y siguen mandando cuando se ponen. El tope se acota a 1023 porque
            // `dt` tiene 1024 entradas: un valor mayor indexaria fuera del array.
            lim: std::env::var("VC_LIM").ok().and_then(|v| v.parse().ok())
                .unwrap_or(tasas_de_modelo(modelo).0).min(1023),
            isse_lr: std::env::var("VC_ISSE_LR").ok().and_then(|v| v.parse().ok()).unwrap_or(6),
            mix_lr: std::env::var("VC_MIX_LR").ok().and_then(|v| v.parse().ok())
                .unwrap_or(tasas_de_modelo(modelo).1).clamp(6, 16),
            cxt: [0; NCTX], idx: [0; NCTX], st: [0; NCTXP],
            decay: std::env::var("VC_DECAY").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
            disp_on: std::env::var("VC_DISP").map(|v| v != "0").unwrap_or(true),
            disp_apagada: (apag_total >> IDISP & 1 != 0) && (apag_total >> IDISP2 & 1 != 0),
            mm_verif: mm_verif_env().unwrap_or(verifica_match_de_modelo(modelo)),
            tres_on: TRES_ON.unwrap_or(tres_de_modelo(modelo)),
            apagados: apag_total,
            censo: std::env::var("VC_CENSO").is_ok(),
            corr: std::env::var("VC_CORR").is_ok(),
            corr_n: 0.0,
            corr_s: vec![0.0; NCTX],
            corr_sp: vec![0.0; NCTX * NCTX],
            costepos: std::env::var("VC_COSTEPOS").ok().map(|f| {
                // APEND, no create: si el modelo se re-instancia por bloque, un create
                // truncaria y solo sobreviviria el ultimo trozo.
                let fh = std::fs::OpenOptions::new().append(true).create(true).open(&f)
                    .expect("VC_COSTEPOS");
                eprintln!("[costepos] modelo instanciado");
                std::io::BufWriter::new(fh)
            }),
            cb: 0.0, cbn: 0,
            coste: vec![0.0; NCTX],
            bits_vistos: 0,
            #[cfg(target_arch = "x86_64")] simd: std::arch::is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))] simd: false,
            bucket: [0; NTAB], isse_bucket: [0; NISSE], nib: 0, jnib: 0,
            w: {
                // EL HUECO DEL MEDIO. Los pesos arrancaban a CERO: 50.688 numeros que el
                // mezclador aprendia desde la nada EN CADA FRAME. Con frames de 8 MiB, un
                // arbol de 50 MB paga ese arranque siete veces.
                //
                // Pre-entrenarlos fuera de linea y traerlos hechos cuesta CERO en ejecucion
                // —es el valor inicial de un array— y es determinista: los mismos numeros en
                // x86 y en ARM. Nadie lo hace: paq8px y zpaq inicializan a una constante y dan
                // por hecho que "ya aprendera". Aprende, si. Cada vez desde cero.
                let mut v: Vec<Vec<i32>> = (0..NMIX).map(|k| vec![0i32; nsel[k] * NCTXP]).collect();
                if let Ok(f) = std::env::var("VC_LOAD_W") {
                    if let Ok(b) = std::fs::read(&f) {
                        let mut o = 0usize;
                        for k in 0..NMIX {
                            for j in 0..nsel[k] * NCTXP {
                                if o + 4 > b.len() { break; }
                                v[k][j] = i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
                                o += 4;
                            }
                        }
                    }
                }
                v
            },
            sel: [0; NMIX], mp: [2048; NMIX], mst: [0; NMIX],
            // la capa final arranca en "media aritmética de los cuatro": neutra y sin sesgo
            // hacia ninguno hasta que la evidencia diga otra cosa.
            w2: vec![(1 << 16) / NMIX as i32; NSEL2 * NMIX], sel2: 0,
            sse: vec![
                Apm::nueva(1024, t),      // 1: byte anterior  (la de siempre)
                Apm::nueva(256, t),       // 2: byte parcial en curso
                Apm::nueva(1 << 16, t),   // 3: los dos bytes previos
                Apm::nueva(1 << 9, t),    // 4: estado del match x posicion de bit
                Apm::nueva(1 << 16, t),   // 5: forma del texto reciente
            ],
            n_sse: std::env::var("VC_SSE").ok().and_then(|v| v.parse().ok()).unwrap_or(5).clamp(1, 5),
            // 49152/65536 = 3/4, la mezcla 3:1 de siempre.
            //
            // ⚠️ ESE 3/4 YA NO SIGNIFICA LO QUE SIGNIFICABA. Venia de cuando la mezcla con la
            // etapa era FIJA; al hacerse aprendida (`sse_w` se mueve con el gradiente), dejo de
            // ser «la mezcla» y paso a ser solo EL PUNTO DE PARTIDA — y nadie volvio a
            // preguntarse si el punto de partida bueno es el mismo que era la respuesta buena.
            //
            // VC_SSE_W0 existe para poder preguntarlo. 🚨 ES SOLO PARA MEDIR: como `VC_DISP`, no
            // viaja en el archivo, asi que comprimir con un valor y descomprimir sin la variable
            // da un archivo que no se puede reabrir. Si el barrido dice que hay que moverlo, se
            // mueve por VERSION DE MODELO, que si viaja — no cambiando este numero.
            sse_w: vec![std::env::var("VC_SSE_W0").ok().and_then(|v| v.parse().ok())
                        .unwrap_or(49152); 5 * 8],
            sse_in: [0; 8], sse_out: [0; 8],
            mixer_p: 2048,
            hist: [0; 8], word: 0, word_prev: 0, wmin: 0, wmin_prev: 0, cgrp: 0, anid: 0, c0: 1, bpos: 0,
            word_prev2: 0, wbuf: [0; WMAX], wlen: 0, raiz_prev: 0, raiz_prev2: 0,
            vent_min: 0,
            mm_buf: Vec::with_capacity(tam + 1),
            mm_hashtab: vec![0u32; 1 << 21],
            mm_ptr: 0, mm_len: 0, mm_pred_bit: 0,
            mm2_hashtab: vec![0u32; 1 << 22],
            mm2_ptr: 0, mm2_len: 0, mm2_pred_bit: 0,
            mm3_hashtab: vec![0u32; 1 << 22],
            mm3_ptr: 0, mm3_len: 0, mm3_pred_bit: 0,
            // Las dos zancadas van ENCENDIDAS por defecto: ganan en las cinco muestras sin
            // excepcion. VC_MMEXTRA=0 las apaga, para poder reproducir y comparar.
            mm_extra: extra,
            mm4_hashtab: if extra & 1 != 0 { vec![0u32; 1 << 22] } else { Vec::new() },
            mm4_ptr: 0, mm4_len: 0, mm4_pred_bit: 0,
            mm8_hashtab: if extra & 2 != 0 { vec![0u32; 1 << 22] } else { Vec::new() },
            mm8_ptr: 0, mm8_len: 0, mm8_pred_bit: 0,
            salta: salta_de_modelo(modelo),
            salto: false,
            // 256 contextos = racha en cubos logaritmicos (4 bits) x posicion de bit (3) x el
            // bit que el match espera (1). Es la tabla entera que se paga por byte saltado.
            sse_salto: Apm::nueva(1 << 12, t),
            #[cfg(feature = "cobertura")]
            cob_racha: 0,
            #[cfg(feature = "cobertura")]
            cob_racha5: 0,
            #[cfg(feature = "cobertura")]
            cob_pred: -1,
            pr: 2048,
            volcar: std::env::var("VC_VOLCAR").ok().map(|f| {
                use std::io::Write;
                // ⚠️ UN VOLCADO POR EJECUCION. `Predictor::new` se llama una vez por BLOQUE; si
                // la entrada se trocea, el segundo bloque abriria el mismo fichero con `create`
                // y borraria lo del primero, dejando un volcado corto que parece entero. Se
                // detecta y se para: un volcado a medias reproduce numeros que no son.
                static YA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if YA.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    panic!("VC_VOLCAR: esta entrada se comprime en MAS DE UN BLOQUE y el volcado \
solo cubre uno. Usa una muestra que quepa en un bloque, o vuelca por bloque con nombres \
distintos. Un volcado truncado da un replay que cuadra consigo mismo y con nada mas.");
                }
                let mut w = std::io::BufWriter::with_capacity(1 << 22, std::fs::File::create(&f)
                    .unwrap_or_else(|e| panic!("VC_VOLCAR: no puedo crear {f}: {e}")));
                // LA CABECERA ES EL CANDADO. Si el replay lee un volcado de otra version del
                // modelo tiene que negarse, no dar un numero. Y no basta con las dimensiones:
                // van tambien familia/modelo/perfil porque `tasas_de_modelo(modelo)` fija el
                // `mix_lr` POR DEFECTO — o sea que arrancar el replay con otro modelo cambia la
                // linea base sin que nada lo cante. Es el mismo error que destapo mezclador.sh
                // cuando el defecto declarado (10) no era el del binario (12), y ahi costo un 5%.
                w.write_all(b"VCST0002").unwrap();
                for v in [NCTX as u32, NMIX as u32, REG_VOLCADO as u32, 5u32] {
                    w.write_all(&v.to_le_bytes()).unwrap();
                }
                w.write_all(&(tam as u64).to_le_bytes()).unwrap();
                w.write_all(&[familia, modelo, perfil, 0u8]).unwrap();
                w
            }),
            cx_v: [0; 5],
        }
    }

    // arranca el modelado de un nuevo byte: fija los hashes de contexto
    fn set_byte_contexts(&mut self) {
        let h = &self.hist;
        // órdenes 0..5 (hash de los últimos k bytes)
        let mut hash = |data: &[u8]| -> u32 {
            let mut x: u32 = 0x811C9DC5;
            for &b in data { x = (x ^ b as u32).wrapping_mul(0x01000193); }
            x
        };
        // ❌ NEGATIVO MEDIDO (03-08): COMPARTIR ESTA PASADA SALE MAS CARO. NO LO «ARREGLES».
        //
        // Los seis ordenes son rangos ANIDADOS y cada uno se hashea DESDE CERO: 1+2+3+4+5+8 = 23
        // lecturas y 23 pasos por byte para informacion metida una dentro de otra. Es la P2 del
        // camino, y parece un desperdicio de libro. Se implemento la version que lo comparte
        // —doblar FNV del byte MAS NUEVO al mas viejo, con lo que el estado tras k bytes ES el
        // contexto de orden k: 8 pasos en vez de 23— y se midio contra esta con el MISMO binario
        // (VC_MODELO=5 contra 6, etiqueta «P2 hash compartido» en el libro mayor):
        //
        //     codigo  -0,001% bytes  +0,6% ciclos      sql  -0,004%  +7,2%      logs  0,000%  +1,3%
        //     TOTAL   -0,002% bytes  +3,1% CICLOS  <- compartir es MAS LENTO
        //
        // POR QUE, que es lo que hay que recordar: estas seis cadenas son INDEPENDIENTES entre
        // si, y el procesador las solapa. La version compartida las convierte en UNA cadena de
        // ocho pasos donde cada uno espera al anterior — y la multiplicacion de FNV tiene varios
        // ciclos de latencia. El trabajo «repetido» era gratis porque iba en paralelo; quitar la
        // repeticion lo puso en serie. 23 pasos solapados salen mas baratos que 8 encadenados.
        //
        // Y la escala lo confirma: son ~23 pasos sobre ~82.000 instrucciones POR BYTE. Aunque
        // hubieran salido gratis, no se puede ganar el 0,03% de nada. Lo caro esta en el
        // mezclador (159 entradas) y en los 36 fallos de cache por nibble, no aqui.
        //
        // La ventana `hist` son 8 bytes = un u64 en registro: «volver a leerla» no es un acceso
        // a memoria. La regla «si ya se leyo, reutilizalo» sigue siendo buena — pero apunta a la
        // BUSQUEDA EN TABLA, no a esto.
        self.cxt[0] = 0;
        self.cxt[1] = hash(&h[7..8]).wrapping_mul(2 + 1);
        self.cxt[2] = hash(&h[6..8]).wrapping_mul(3);
        self.cxt[3] = hash(&h[5..8]).wrapping_mul(5);
        self.cxt[4] = hash(&h[4..8]).wrapping_mul(7);
        self.cxt[5] = hash(&h[3..8]).wrapping_mul(11);
        self.cxt[6] = self.word.wrapping_mul(0x9E3779B1).wrapping_add(0x1234567); // contexto de palabra
        self.cxt[7] = hash(&h[0..8]).wrapping_mul(13);        // orden 8
        // SPARSE de paso fijo. Los datos numéricos —imágenes médicas, tablas astronómicas,
        // series de medidas— van en registros de 2, 4 u 8 bytes: ahí el byte que predice no
        // es el de al lado, es el que está EXACTAMENTE un registro atrás. Un orden N no puede
        // verlo porque arrastra también los bytes de en medio, que son ruido.
        self.cxt[22] = hash(&h[4..5]).wrapping_mul(29);           // paso 4
        self.cxt[23] = hash(&h[0..1]).wrapping_mul(31);           // paso 8
        self.cxt[24] = hash(&[h[6], h[4]]).wrapping_mul(37);      // pasos 2 y 4
        self.cxt[25] = {
            let k = ((h[6] as usize) << 8) | h[7] as usize;
            (self.ind2[k] ^ (k as u32).wrapping_mul(0x9E3779B1)).wrapping_mul(0x01000193)
        };
        // ---- FAMILIA DE TEXTO ----
        // 30 — trigrama de palabras. Abarca 15-20 bytes: MAS ALLA del orden-12, o sea que no
        // es redundante con nada de lo que ya hay. Es el contexto que usa cualquier modelo de
        // lenguaje y el que nos faltaba.
        self.cxt[ITXT] = self.word_prev2.wrapping_mul(0x9E3779B1)
            ^ self.word_prev.wrapping_mul(0x85EBCA6B)
            ^ self.word.wrapping_mul(0x27220A95);
        // 31 — raiz de la palabra anterior + la parcial en curso. "running", "runs" y "run"
        // dejan de ser tres contextos distintos con un tercio de datos cada uno.
        self.cxt[ITXT + 1] = self.raiz_prev.wrapping_mul(0x165667B1)
            ^ self.word.wrapping_mul(0x2545F491);
        // 32 — el trigrama otra vez, pero por raices: inmune a la morfologia.
        self.cxt[ITXT + 2] = self.raiz_prev2.wrapping_mul(0x27220A95)
            ^ self.raiz_prev.wrapping_mul(0x9E3779B1)
            ^ self.word.wrapping_mul(0x165667B1);
        // 33 — skip-grama: se salta la palabra de en medio. "el ___ gato" predice igual de
        // bien tanto si en medio va "pequeno" como "negro", y el trigrama no puede verlo.
        self.cxt[ITXT + 3] = self.word_prev2.wrapping_mul(0x2545F491)
            ^ self.word.wrapping_mul(0x85EBCA6B);
        self.cxt[15] = self.word_prev.wrapping_mul(0x2545F491)
            ^ self.word.wrapping_mul(0x9E3779B1);            // pareja de palabras
        self.cxt[16] = self.wmin.wrapping_mul(0x27220A95)
            ^ self.wmin_prev.wrapping_mul(0x165667B1);       // pareja en minúsculas
        self.cxt[8] = hash(&h[6..7]).wrapping_mul(17);        // sparse: SOLO el byte a distancia 2
        self.cxt[9] = hash(&[h[7], h[5]]).wrapping_mul(19);   // sparse: distancias 1 y 3, saltando la 2
        // indirecto: la clave no es el byte anterior, sino la retahíla de bytes que
        // históricamente vinieron detrás de él.
        self.cxt[10] = (self.ind1[h[7] as usize] ^ (h[7] as u32).wrapping_mul(0x9E3779B1))
            .wrapping_mul(0x01000193);
        // orden 12: sale del buffer del match model, que ya guarda el bloque entero. No hace
        // falta ampliar la historia corta para mirar más atrás.
        let nb = self.mm_buf.len();
        // `vent_min` es 0 fuera del banco, o sea `nb - 12 >= 0`: la guarda no existe.
        self.cxt[14] = if nb >= 12 && nb - 12 >= self.vent_min {
            hash(&self.mm_buf[nb - 12..nb]).wrapping_mul(23)
        } else { 0 };
        // ---- fila: columna actual y byte de ENCIMA ----
        // Si hay saltos de línea, la fila la marcan ellos: columna = bytes desde el último
        // salto, y "encima" = esa misma columna de la fila anterior. Si no los hay pero sí
        // rejilla (binario), se cae al ancho fijo deducido.
        let n = self.mm_buf.len() as u32;
        // ¿Quién está marcando los registros aquí? Por defecto el salto de línea. Si llevamos
        // 4 KB sin uno, es que este fichero no va por líneas y hay que buscar el separador que
        // sí esté partiendo el dato en trozos de tamaño de registro.
        let mut e = 0usize;
        if self.sep_n[0] < 2 || n - self.sep_last[0] >= 4096 {
            // Se elige el hueco MÁS GRANDE que siga siendo de tamaño de registro, no el
            // separador más frecuente. En un volcado SQL la coma aparece muchísimo más que el
            // paréntesis, pero lo que separa REGISTROS es el paréntesis: sólo alineando por él
            // el "byte de encima" cae en el mismo campo de la tupla anterior. Ir a lo más
            // frecuente alinea por campo y no dice casi nada.
            let mut mejor_hueco = 0u32;
            for k in 1..NSEP {
                if self.sep_n[k] < 8 { continue; }
                let hueco = n / self.sep_n[k];
                if (8..=2048).contains(&hueco) && hueco > mejor_hueco {
                    mejor_hueco = hueco;
                    e = k;
                }
            }
        }
        let col_l = n - self.sep_last[e];
        let (col, arriba) = if self.sep_n[e] >= 2 && col_l < 4096 {
            let ai = self.sep_prev[e] + col_l;
            let ab = if ai < self.sep_last[e] && (ai as usize) >= self.vent_min {
                self.mm_buf[ai as usize] } else { 0 };
            (col_l, ab)
        } else if self.rlen > 1 && n >= self.rlen && (n - self.rlen) as usize >= self.vent_min {
            (n % self.rlen, self.mm_buf[(n - self.rlen) as usize])
        } else { (0, 0) };
        // El separador entra en el contexto: la columna 7 de una fila y la columna 7 de una
        // tupla no son la misma cosa y no deben compartir estadística.
        self.cxt[11] = col.wrapping_mul(0x9E3779B1) ^ (e as u32).wrapping_mul(0x85EBCA6B);
        self.cxt[12] = (arriba as u32).wrapping_mul(0x27220A95) ^ col.wrapping_mul(0x165667B1);
        self.cxt[13] = (arriba as u32).wrapping_mul(0x2545F491) ^ (h[7] as u32).wrapping_mul(0x9E3779B1);
        // ---- segundo alineamiento, por el separador MÁS FRECUENTE ----
        // No compite con el anterior: uno marca el REGISTRO (la fila, la tupla) y este el
        // CAMPO dentro de él. Medidos por separado, cada uno gana en un sitio distinto —el
        // grueso en volcados SQL, el fino en código— así que van los dos.
        let mut e2 = 0usize;
        let mut mejor_n = 0u32;
        for k in 0..NSEP {
            if self.sep_n[k] < 8 { continue; }
            let hueco = n / self.sep_n[k];
            if (4..=2048).contains(&hueco) && self.sep_n[k] > mejor_n { mejor_n = self.sep_n[k]; e2 = k; }
        }
        let d2 = n - self.sep_last[e2];
        let (col2, arriba2) = if self.sep_n[e2] >= 2 && d2 < 4096 {
            let ai = self.sep_prev[e2] + d2;
            (d2, if ai < self.sep_last[e2] && (ai as usize) >= self.vent_min {
                self.mm_buf[ai as usize] } else { 0 })
        } else { (0, 0) };
        self.cxt[17] = col2.wrapping_mul(0x27220A95) ^ (e2 as u32).wrapping_mul(0x165667B1);
        self.cxt[18] = (arriba2 as u32).wrapping_mul(0x9E3779B1) ^ col2.wrapping_mul(0x2545F491);
        self.cxt[19] = (arriba2 as u32).wrapping_mul(0x85EBCA6B) ^ (h[7] as u32).wrapping_mul(0x27220A95);
        // ---- MODELO NUMERICO: alineado por CAMPO, no por byte ----
        // Aqui ya se sabe quien marca registro (e) y quien campo (e2). Primero se digiere el
        // byte que acaba de cerrarse, y luego se arman los contextos del que viene.
        self.e_rec = e;
        self.e_campo = e2;
        let ub = h[7];
        if ub.is_ascii_digit() {
            self.num_cur = self.num_cur.saturating_mul(10) + (ub - b'0') as u64;
            self.nd += 1;
        } else {
            if self.nd > 0 {
                // el numero se acaba de cerrar: pasa a ser "el de arriba" de este campo
                let c = (self.campo as usize).min(NCAMPO - 1);
                if self.num_len[c] > 0 {
                    self.num_delta[c] = self.num_cur as i64 - self.num_ant[c] as i64;
                }
                self.num_ant[c] = self.num_cur;
                self.num_len[c] = self.nd.min(19) as u8;
                self.num_cur = 0;
                self.nd = 0;
            }
            if ub == SEPS[self.e_rec] { self.campo = 0; }
            else if ub == SEPS[self.e_campo] { self.campo = self.campo.saturating_add(1); }
        }
        {
            let c = (self.campo as usize).min(NCAMPO - 1);
            let nd = self.nd.min(31);
            let cc = (self.campo.min(63)) as u32;
            // Solo tiene sentido dentro de un numero o justo donde puede empezar uno. Fuera de
            // ahi el contexto se colapsa a 0 y el mezclador aprende a no hacerle caso.
            let activo = self.nd > 0 || !ub.is_ascii_alphanumeric();
            let la = self.num_len[c] as u32;
            let va = self.num_ant[c];
            // 26 — el digito de ENCIMA, alineado por campo. Es el arreglo directo: los modelos
            // de fila miran el byte a la misma DISTANCIA EN BYTES, que se desalinea en cuanto
            // un campo cambia de ancho.
            self.cxt[INUM] = if activo {
                let d = digito_en(va, la, nd);
                (cc.wrapping_mul(0x9E3779B1) ^ nd.wrapping_mul(0x85EBCA6B)
                    ^ d.wrapping_mul(0x27220A95) ^ la.wrapping_mul(0x165667B1))
                    .wrapping_mul(0x01000193)
            } else { 0 };
            // 27 — cuantos digitos va a tener. Por columna es casi constante, y saber si el
            // numero SIGUE o se ACABA aqui es media prediccion del byte.
            self.cxt[INUM + 1] = if activo {
                (cc.wrapping_mul(0x2545F491) ^ nd.wrapping_mul(0x9E3779B1)
                    ^ la.wrapping_mul(0x85EBCA6B)).wrapping_mul(0x01000193)
            } else { 0 };
            // 28 — extrapolacion por SALTO: el valor esperado es el anterior mas el ultimo
            // salto. En una columna de IDs autoincrementales o de marcas de tiempo esto acierta
            // el numero entero, no solo su forma.
            self.cxt[INUM + 2] = if activo && la > 0 {
                let ve = (va as i64).saturating_add(self.num_delta[c]).max(0) as u64;
                let mut le = 1u32; let mut x = ve / 10;
                while x > 0 { le += 1; x /= 10; }
                let d = digito_en(ve, le, nd);
                (cc.wrapping_mul(0x165667B1) ^ nd.wrapping_mul(0x27220A95)
                    ^ d.wrapping_mul(0x9E3779B1) ^ le.wrapping_mul(0x2545F491))
                    .wrapping_mul(0x01000193)
            } else { 0 };
            // 29 — los digitos que YA llevamos de este numero, por columna. Captura formatos
            // fijos (codigos postales, referencias, decimales de precio) sin saber cual es.
            self.cxt[INUM + 3] = if activo {
                let pre = (self.num_cur % 1000) as u32;
                (cc.wrapping_mul(0x85EBCA6B) ^ nd.wrapping_mul(0x165667B1)
                    ^ pre.wrapping_mul(0x9E3779B1)).wrapping_mul(0x01000193)
            } else { 0 };
        }
        // ANIDAMIENTO. `cxt[IANID]` es la huella sola: predice lo que suele venir A ESTA
        // PROFUNDIDAD. `cxt[IANID+1]` la cruza con el byte anterior, que es lo que distingue una
        // coma dentro de un array de una coma dentro de una llamada — se codifican muy distinto
        // y para un modelo de orden 1 son el mismo contexto.
        let an = self.anid as u32;
        self.cxt[IANID] = an.wrapping_mul(0x9E3779B1) ^ an.rotate_left(13).wrapping_mul(0x85EBCA6B);
        self.cxt[IANID + 1] = an.wrapping_mul(0x27220A95) ^ (h[7] as u32).wrapping_mul(0x165667B1);
        self.cxt[20] = ((self.cgrp & 0xFFFF_FFFF) as u32).wrapping_mul(0x9E3779B1);
        self.cxt[21] = ((self.cgrp >> 24) as u32).wrapping_mul(0x85EBCA6B)
            ^ (self.cgrp as u32).wrapping_mul(0x2545F491);
        // match model: actualiza puntero por hash de últimos 6 bytes
        let mh = hash(&h[2..8]) & ((1 << 21) - 1);
        let cand = self.mm_hashtab[mh as usize];
        let pos = self.mm_buf.len() as u32;
        if self.mm_len == 0 && cand > 0 && (cand as usize) < self.mm_buf.len() {
            // ¿es la misma historia, o solo el mismo hash? Ver `mm_verif()`: la tabla tiene 2^21
            // ranuras para hasta 33,5 M de posiciones, asi que colisionar es lo normal.
            let (c, n) = (cand as usize, self.mm_buf.len());
            let cierto = c >= 6 && n >= 6 && self.mm_buf[c - 6..c] == self.mm_buf[n - 6..n];
            use std::sync::atomic::Ordering::Relaxed;
            if cierto { MM_CIERTOS.fetch_add(1, Relaxed); } else { MM_FALSOS.fetch_add(1, Relaxed); }
            if cierto || !self.mm_verif {
                self.mm_ptr = cand;
                self.mm_len = 1;
            }
        }
        self.mm_hashtab[mh as usize] = pos;
        // el largo: 12 bytes. Engancha menos, pero cuando engancha casi nunca miente.
        if nb >= 12 {
            let mh2 = (hash(&self.mm_buf[nb - 12..nb]) & ((1 << 22) - 1)) as usize;
            let cand2 = self.mm2_hashtab[mh2];
            if self.mm2_len == 0 && cand2 > 0 && (cand2 as usize) < nb {
                self.mm2_ptr = cand2;
                self.mm2_len = 1;
            }
            self.mm2_hashtab[mh2] = pos;
        }
        // el disperso: seis bytes de dos en dos, o sea doce de historia mirando los pares
        if nb >= 13 {
            let b = &self.mm_buf;
            let clave = [b[nb - 2], b[nb - 4], b[nb - 6], b[nb - 8], b[nb - 10], b[nb - 12]];
            let mh3 = (hash(&clave) & ((1 << 22) - 1)) as usize;
            let cand3 = self.mm3_hashtab[mh3];
            if self.mm3_len == 0 && cand3 > 0 && (cand3 as usize) < nb {
                self.mm3_ptr = cand3;
                self.mm3_len = 1;
            }
            self.mm3_hashtab[mh3] = pos;
        }
        if self.mm_extra & 1 != 0 && nb >= 25 {
            let b = &self.mm_buf;
            let clave = [b[nb - 4], b[nb - 8], b[nb - 12], b[nb - 16], b[nb - 20], b[nb - 24]];
            let mh = (hash(&clave) & ((1 << 22) - 1)) as usize;
            let cand = self.mm4_hashtab[mh];
            if self.mm4_len == 0 && cand > 0 && (cand as usize) < nb { self.mm4_ptr = cand; self.mm4_len = 1; }
            self.mm4_hashtab[mh] = pos;
        }
        if self.mm_extra & 2 != 0 && nb >= 49 {
            let b = &self.mm_buf;
            let clave = [b[nb - 8], b[nb - 16], b[nb - 24], b[nb - 32], b[nb - 40], b[nb - 48]];
            let mh = (hash(&clave) & ((1 << 22) - 1)) as usize;
            let cand = self.mm8_hashtab[mh];
            if self.mm8_len == 0 && cand > 0 && (cand as usize) < nb { self.mm8_ptr = cand; self.mm8_len = 1; }
            self.mm8_hashtab[mh] = pos;
        }
        #[cfg(feature = "cobertura")]
        self.cob_foto();
        // ── F6·Q6 · LA DECISION DEL SALTO, y va AQUI por una razon exacta ──────────────────────
        // Este es el final del byte anterior y el principio del siguiente: `mm_len` ya esta
        // avanzado y todavia no se ha codificado ni un bit del byte que viene. Es el ultimo
        // instante en el que compresor y descompresor tienen literalmente el mismo estado, y por
        // eso los dos toman la misma rama sin que nada viaje en el archivo.
        self.salto = self.salta
            && self.mm_len >= UMBRAL_SALTO
            && (self.mm_ptr as usize) < self.mm_buf.len();
        // Los cubos son la parte cara del byte —41 sondas de tabla que fallan en cache— y en un
        // byte saltado no los mira nadie: `p()` no entra al conjunto y la memoria de racha
        // tampoco se actualiza. Saltarlos ES el ahorro; dejarlos seria pagar el billete y no
        // subirse. Al primer byte no saltado se vuelven a fijar aqui mismo.
        if !self.salto { self.fija_cubos(); }
    }

    /// F6·Q6 · la foto del match justo antes de codificar el byte siguiente. SOLO LEE.
    #[cfg(feature = "cobertura")]
    fn cob_foto(&mut self) {
        self.cob_racha = self.mm_len;
        self.cob_racha5 = self.mm_len.max(self.mm2_len).max(self.mm3_len)
            .max(self.mm4_len).max(self.mm8_len);
        self.cob_pred = if self.mm_len > 0 && (self.mm_ptr as usize) < self.mm_buf.len() {
            self.mm_buf[self.mm_ptr as usize] as i32
        } else { -1 };
    }

    /// PERILLA DE BANCO (F6·Q3): cierra la ventana sobre lo ya visto, dejando las TABLAS.
    ///
    /// Se llama UNA vez, en la frontera entre el cebador y el frame que se mide. Despues de
    /// esto el predictor sigue sabiendo todo lo que aprendio —cubos, StateMaps, pesos del
    /// mezclador, DMC, cadena ISSE, indirectos, contadores de separador— y no puede leer NI UN
    /// BYTE de lo anterior:
    ///   · las tablas de los cinco match models se borran, asi que ningun candidato puede
    ///     apuntar al cebador (y de paso ningun `mm_ptr` vivo sobrevive a la frontera);
    ///   · `vent_min` corta las otras dos lecturas de `mm_buf` que hay en el modelo: el
    ///     contexto de orden 12 y el «byte de encima» de los dos modelos de fila.
    ///
    /// `mm_buf` NO se trunca a proposito: `sep_last`/`sep_prev` guardan posiciones ABSOLUTAS
    /// dentro de el, y truncarlo las dejaria apuntando fuera. Cortar por lectura y no por
    /// tamaño es lo unico que no obliga a re-basar media docena de campos a mano.
    fn corta_ventana(&mut self) {
        self.vent_min = self.mm_buf.len();
        for h in [&mut self.mm_hashtab, &mut self.mm2_hashtab, &mut self.mm3_hashtab,
                  &mut self.mm4_hashtab, &mut self.mm8_hashtab] {
            for v in h.iter_mut() { *v = 0; }
        }
        self.mm_ptr = 0; self.mm_len = 0; self.mm_pred_bit = 0;
        self.mm2_ptr = 0; self.mm2_len = 0; self.mm2_pred_bit = 0;
        self.mm3_ptr = 0; self.mm3_len = 0; self.mm3_pred_bit = 0;
        self.mm4_ptr = 0; self.mm4_len = 0; self.mm4_pred_bit = 0;
        self.mm8_ptr = 0; self.mm8_len = 0; self.mm8_pred_bit = 0;
    }

    /// Elige el cubo de cada modelo. Se llama DOS veces por byte: al empezar el byte y al
    /// llegar a la mitad. En medio, los 15 estados del nibble se leen del mismo cubo.
    fn fija_cubos(&mut self) {
        // La semilla distingue el nibble alto del bajo: en el bajo entra lo que ya sabemos
        // del byte, que es justo el contexto que faltaba.
        let semilla = if self.bpos == 0 { 0x9E3779B1u32 } else { self.c0.wrapping_mul(0x85EBCA6B) };

        // ---- PASADA 1: calcular las direcciones y PEDIRLAS TODAS A LA VEZ ----
        //
        // Antes esto era un solo bucle que calculaba una direccion y la tocaba acto seguido:
        // ~41 fallos de cache EN SERIE, cada uno esperando al anterior. El procesador puede
        // tener muchos fallos en vuelo simultaneos, pero no puede adivinar la siguiente
        // direccion porque sale de un hash que aun no se ha calculado.
        //
        // Partiendolo en dos pasadas, las 41 peticiones se solapan y se paga UNA espera en
        // vez de cuarenta y una.
        let mut conj_t = [0usize; NTAB];
        let mut tag_t = [0u8; NTAB];
        for i in 0..NTAB {
            if self.apagados >> i & 1 != 0 { continue; }
            let hh = self.cxt[i].wrapping_add(semilla).wrapping_mul(0x2545F491);
            conj_t[i] = (((hh >> 8) & self.tmask[i]) as usize) << 6;
            tag_t[i] = (hh & 0xFF) as u8;
            #[cfg(target_arch = "x86_64")]
            unsafe {
                // Una PISTA, no una lectura: no puede fallar, no puede leer fuera (la
                // direccion ya esta enmascarada por tmask) y no cambia ni un bit de la salida.
                std::arch::x86_64::_mm_prefetch(
                    self.estados[i].as_ptr().add(conj_t[i]) as *const i8,
                    std::arch::x86_64::_MM_HINT_T0,
                );
            }
        }
        let mut conj_i = [0usize; NISSE];
        let mut tag_i = [0u8; NISSE];
        for k in 0..NISSE {
            let hh = self.cxt[ISSE_CTX[k]].wrapping_add(semilla).wrapping_mul(0xC2B2AE35);
            conj_i[k] = (((hh >> 8) & self.imask) as usize) << 6;
            tag_i[k] = (hh & 0xFF) as u8;
            #[cfg(target_arch = "x86_64")]
            unsafe {
                std::arch::x86_64::_mm_prefetch(
                    self.isse_est[k].as_ptr().add(conj_i[k]) as *const i8,
                    std::arch::x86_64::_MM_HINT_T0,
                );
            }
        }

        // ---- PASADA 2: resolver, ya con las lineas de camino ----
        for i in 0..NTAB {
            if self.apagados >> i & 1 != 0 { continue; }
            self.bucket[i] = busca_cubo(&mut self.estados[i], conj_t[i], tag_t[i], self.t);
            if self.bpos == 0 { self.bucket0[i] = self.bucket[i]; }
        }
        for k in 0..NISSE {
            self.isse_bucket[k] = busca_cubo(&mut self.isse_est[k], conj_i[k], tag_i[k], self.t);
        }
        self.nib = 0;
        self.jnib = 0;
    }

    /// F6·Q6 · EL BYTE SALTADO: el match manda y una sola etapa SSE le pone el precio.
    ///
    /// Lo que NO se hace aqui es el punto entero: ni un StateMap, ni la cadena ISSE, ni los cuatro
    /// mezcladores, ni las cinco etapas de calibracion, ni las 53 entradas del conjunto. Lo que SI
    /// se hace, y no es negociable, es mantener el `pred_bit` de los CINCO match: es lo que consume
    /// el corte de racha del `update`, o sea el estado que gobierna el propio salto y el que los
    /// modelos se encontraran al volver. Dejarlos rancios seria salir del salto con cinco rachas
    /// que no significan nada.
    fn p_salto(&mut self) -> i32 {
        let bp = self.bpos;
        let esperado = self.mm_buf[self.mm_ptr as usize];
        let exp = ((esperado as u32) >> (7 - bp)) & 1;
        self.mm_pred_bit = exp as i32;
        let nb = self.mm_buf.len();
        let pb = |len: u32, ptr: u32| -> i32 {
            if len > 0 && (ptr as usize) < nb {
                (((self.mm_buf[ptr as usize] as u32) >> (7 - bp)) & 1) as i32
            } else { -1 }
        };
        self.mm2_pred_bit = pb(self.mm2_len, self.mm2_ptr);
        self.mm3_pred_bit = pb(self.mm3_len, self.mm3_ptr);
        self.mm4_pred_bit = pb(self.mm4_len, self.mm4_ptr);
        self.mm8_pred_bit = pb(self.mm8_len, self.mm8_ptr);
        // La entrada de la etapa: la misma confianza que el match le da al mezclador en el camino
        // normal (`mm_len.min(32) * 64`), para que la etapa vea la misma escala que ya conoce.
        let conf = (self.mm_len.min(32) * 64) as i32;
        let pr0 = self.t.squash(if exp == 1 { conf } else { -conf });
        // La racha en cubos logaritmicos: de 32 a 33 no hay informacion nueva, de 32 a 1000 si.
        let rb = (32 - self.mm_len.leading_zeros()).min(15) as usize;
        // EL ACUERDO DE LOS OTROS CUATRO. Es la unica señal de conjunto que sobrevive al salto y
        // sale gratis: los cuatro `pred_bit` ya estan calculados dos lineas mas arriba porque el
        // corte de racha los necesita. Cinco match apuntando al mismo bit no es lo mismo que uno
        // solo, y sin esto la etapa no puede distinguirlos.
        let e = exp as i32;
        let acuerdo = (self.mm2_pred_bit == e) as usize + (self.mm3_pred_bit == e) as usize
            + (self.mm4_pred_bit == e) as usize + (self.mm8_pred_bit == e) as usize;
        let cx = (acuerdo << 8) | (rb << 4) | ((bp as usize) << 1) | exp as usize;
        let pa = { let t = self.t; self.sse_salto.p(pr0, cx, t) };
        self.pr = pa.clamp(1, 4094);
        self.pr
    }

    fn p(&mut self) -> i32 {
        if self.salto { return self.p_salto(); }
        // índice de cada modelo = cxt ^ c0 (bits parciales)
        let c0 = self.c0;
        // Ranura dentro del cubo: el nodo del árbol binario del nibble. 1, luego 2-3, luego
        // 4-7, luego 8-15. Nunca 0, que es donde vive la firma.
        let nodo = (1u32 << self.jnib) + self.nib;
        for i in 0..NTAB {
            // Un modelo apagado deja su stretch a 0: el mezclador no le suma nada y su peso
            // no se mueve, así que es exactamente como si no existiera.
            if self.apagados >> i & 1 != 0 { self.st[i] = 0; self.est[i] = 0; self.idx[i] = 0; continue; }
            self.idx[i] = self.bucket[i] + nodo;
            let e = self.estados[i][self.idx[i] as usize];
            self.est[i] = e;
            let pr = (self.sm[(i << 8) | e as usize] & 0xFFFF) as i32; // 0..65535
            let mut v = self.t.stretch(pr >> 4); // a 12-bit para stretch
            // (a) A LA MITAD SI EL CONTEXTO ES JOVEN. Idea de paq8px (`st >> contextIsYoung`).
            // Un contexto con dos observaciones o menos da una probabilidad que el StateMap ha
            // heredado, no aprendido: vale, pero no tanto como una con cuarenta. El mezclador
            // no puede saberlo —su peso es global para el contexto, no por observacion— asi que
            // se le dice aqui, donde el estado esta a mano y no cuesta ni un acceso.
            let (nn0, nn1) = (self.t.h.n0[e as usize] as u32, self.t.h.n1[e as usize] as u32);
            if VISTAS_EXTRA.0 && nn0 + nn1 <= 2 { v >>= 1; }
            self.st[i] = v;
            // (d) EL TERMINO DEL CONTEXTO DETERMINISTA, y es la unica de sus cuatro vistas que
            // un escalado NO puede reproducir: no es la misma cifra en otra escala, es OTRA
            // FUNCION del estado. Vale 0 mientras el contexto haya visto los dos bits, y solo
            // despierta cuando la historia dice que uno de los dos NUNCA ha pasado. Separa
            // «probable» de «no ha ocurrido jamas», que para el mezclador son cosas distintas y
            // hoy le llegan mezcladas en un solo numero.
            // LA VISTA DE RACHA. Si los bits que llevamos del byte en curso coinciden con los
            // del byte recordado, ese byte predice el siguiente bit; la confianza sube con la
            // racha. Es una prediccion de BYTE dentro de un codec de BITS, y la historia de bits
            // no puede tenerla: aquella dice «en este nodo suele salir 1», esta dice «detras de
            // este contexto viene la 'e', y lleva cuarenta veces».
            self.st[IRACHA + i] = if !*RACHA_ON { 0 } else {
                let m = self.bucket0[i] as usize / 4; // 4 bytes por cubo contra 16 del estado
                let (ult, racha) = (self.memo[i][m], self.memo[i][m + 1]);
                // ¿encaja lo que llevamos del byte con el recordado?
                // `c0` es «1 + bits vistos», asi que los bits limpios son c0 sin ese 1 de arriba.
                let c0b = self.c0 - (1 << self.bpos);
                if racha > 0 && (self.bpos == 0 || (ult as u32 >> (8 - self.bpos)) == c0b) {
                    let bit = (ult >> (7 - self.bpos)) as i32 & 1;
                    // La confianza NO es lineal en la racha: de 1 a 2 repeticiones el salto de
                    // informacion es enorme y de 40 a 41 es nada. Logaritmica, y topada a 11
                    // para no darle al mezclador un numero que le tape a los demas.
                    if mapa_aprendido() {
                        // LA SITUACION, no la confianza: la racha en cubos logaritmicos agrupa
                        // situaciones parecidas para que la tabla no tenga una visita por
                        // casilla. El logaritmo sigue aqui, pero ya no ES la respuesta: es el
                        // indice. La respuesta la pone el dato.
                        let rb = (32 - (racha as u32).leading_zeros()).min(15) as usize;
                        let bp = match self.bpos { 0 => 0usize, 1 => 1, 2..=4 => 2, _ => 3 };
                        let dif = (self.memo[i][m + 2] != ult) as usize;
                        let j = i * 256 | (rb << 4) | (bp << 2) | (dif << 1) | bit as usize;
                        self.run_ix[i] = j;
                        self.t.stretch(((self.run_sm[j] & 0xFFFF) as i32) >> 4)
                    } else {
                        let conf = match forma_racha() {
                            1 => (racha as i32).min(11),
                            2 => 4,
                            _ => (32 - (racha as u32).leading_zeros()).min(11) as i32,
                        };
                        if bit == 1 { conf * peso_racha() } else { -conf * peso_racha() }
                    }
                } else { self.run_ix[i] = usize::MAX; 0 }
            };
            // LOS TRES DISTINTOS. La racha dice «viene el mismo»; esto dice «viene uno de
            // estos tres». Se suman los votos de los candidatos que ENCAJAN con lo que llevamos
            // del byte, y se pesa por CUANTOS encajan: si los tres coinciden en el bit siguiente
            // la señal es fuerte; si se contradicen, se cancelan solos.
            self.st[ITRES + i] = if !self.tres_on || !*RACHA_ON { 0 } else {
                let m = self.bucket0[i] as usize / 4;
                let c0b = self.c0 - (1 << self.bpos);
                // La SITUACION, no la confianza: cuantos de los dos candidatos encajan con lo
                // que llevamos del byte, y que dicen. Es un numero de 0..8 —dos candidatos, cada
                // uno ausente/vota-0/vota-1— que se mete en el mapa igual que la racha. La
                // primera version multiplicaba el voto por un 420 que me invente; el barrido
                // dice que el multiplicador optimo era otro y el mapa gana a los dos.
                let mut sit = 0usize;
                for (p, k) in [2usize, 3].iter().enumerate() {
                    let b = self.memo[i][m + k];
                    let v = if b == 0 { 0usize }
                        else if self.bpos == 0 || (b as u32 >> (8 - self.bpos)) == c0b {
                            1 + ((b >> (7 - self.bpos)) & 1) as usize
                        } else { 0 };
                    sit |= v << (p * 2);
                }
                if sit == 0 { self.tres_ix[i] = usize::MAX; 0 }
                else if mapa_aprendido() {
                    let bp = match self.bpos { 0 => 0usize, 1 => 1, 2..=4 => 2, _ => 3 };
                    let j = i * 64 | (sit << 2) | bp;
                    self.tres_ix[i] = j;
                    self.t.stretch(((self.tres_sm[j] & 0xFFFF) as i32) >> 4)
                } else {
                    let voto = (sit & 3) as i32 % 3 - 1 + ((sit >> 2) as i32 % 3 - 1);
                    self.tres_ix[i] = usize::MAX;
                    voto * peso_tres()
                }
            };
            self.st[IDET + i] = if !VISTAS_EXTRA.1 { 0 }
                else if nn0 == 0 && nn1 > 0 { (pr >> 4) - 2048 }
                else if nn1 == 0 && nn0 > 0 { 2048 - (pr >> 4) }
                else { 0 };
        }
        // match model: predice el siguiente bit del byte apuntado
        let mm_st = if self.mm_len > 0 && (self.mm_ptr as usize) < self.mm_buf.len() {
            let predicted = self.mm_buf[self.mm_ptr as usize];
            let bit_pos = self.bpos;
            let exp_bit = ((predicted as u32) >> (7 - bit_pos)) & 1;
            self.mm_pred_bit = exp_bit as i32;
            let conf = (self.mm_len.min(32) * 64) as i32;
            if exp_bit == 1 { conf } else { -conf }
        } else { self.mm_pred_bit = -1; 0 };
        self.st[IDMC] = self.t.stretch(self.dmc.p());
        self.idx[IDMC] = 0;
        self.st[IMM] = mm_st;
        self.idx[IMM] = 0;
        let mm2_st = if self.mm2_len > 0 && (self.mm2_ptr as usize) < self.mm_buf.len() {
            let predicted = self.mm_buf[self.mm2_ptr as usize];
            let exp_bit = ((predicted as u32) >> (7 - self.bpos)) & 1;
            self.mm2_pred_bit = exp_bit as i32;
            let conf = (self.mm2_len.min(32) * 64) as i32;
            if exp_bit == 1 { conf } else { -conf }
        } else { self.mm2_pred_bit = -1; 0 };
        self.st[IMM2] = mm2_st;
        self.idx[IMM2] = 0;
        // El disperso engancha por zancadas, así que sus aciertos son más cortos: se le da
        // menos confianza por acierto (32 en vez de 64) para que no grite más de lo que sabe.
        let mm3_st = if self.mm3_len > 0 && (self.mm3_ptr as usize) < self.mm_buf.len() {
            let predicted = self.mm_buf[self.mm3_ptr as usize];
            let exp_bit = ((predicted as u32) >> (7 - self.bpos)) & 1;
            self.mm3_pred_bit = exp_bit as i32;
            let conf = (self.mm3_len.min(32) * 32) as i32;
            if exp_bit == 1 { conf } else { -conf }
        } else { self.mm3_pred_bit = -1; 0 };
        self.st[IMM3] = mm3_st;
        self.idx[IMM3] = 0;
        // Apagadas dejan su stretch a 0: el mezclador no les suma nada y su peso no se mueve.
        let mm4_st = if self.mm4_len > 0 && (self.mm4_ptr as usize) < self.mm_buf.len() {
            let exp_bit = ((self.mm_buf[self.mm4_ptr as usize] as u32) >> (7 - self.bpos)) & 1;
            self.mm4_pred_bit = exp_bit as i32;
            let conf = (self.mm4_len.min(32) * 32) as i32;
            if exp_bit == 1 { conf } else { -conf }
        } else { self.mm4_pred_bit = -1; 0 };
        self.st[IMM4] = mm4_st;
        self.idx[IMM4] = 0;
        let mm8_st = if self.mm8_len > 0 && (self.mm8_ptr as usize) < self.mm_buf.len() {
            let exp_bit = ((self.mm_buf[self.mm8_ptr as usize] as u32) >> (7 - self.bpos)) & 1;
            self.mm8_pred_bit = exp_bit as i32;
            let conf = (self.mm8_len.min(32) * 32) as i32;
            if exp_bit == 1 { conf } else { -conf }
        } else { self.mm8_pred_bit = -1; 0 };
        self.st[IMM8] = mm8_st;
        self.idx[IMM8] = 0;

        // ---- cadena ISSE ----
        // Base: el ICM de orden 1. Cada etapa la refina con un contexto más largo. Encadenar
        // en vez de mezclar en plano es lo que hace que un orden alto sirva para CORREGIR al
        // bajo cuando tiene poca evidencia, en vez de competir con él.
        let mut pc = self.st[1];
        let int_on = *ISSE_INT_ON;
        for k in 0..NISSE {
            let ix = self.isse_bucket[k] + nodo;
            self.isse_idx[k] = ix;
            let e = self.isse_est[k][ix as usize];
            self.isse_s[k] = e;
            let w0 = self.isse_w[k][e as usize * 2] as i64;
            let w1 = self.isse_w[k][e as usize * 2 + 1] as i64;
            self.isse_pin[k] = pc;
            pc = (((w0 * pc as i64) + (w1 * 64)) >> 16).clamp(-2047, 2047) as i32;
            self.isse_pout[k] = pc;
            // Las intermedias, si la perilla esta puesta. Apagada se quedan a 0 desde el
            // constructor y no vuelven a tocarse: peso quieto y aporte cero al producto escalar,
            // el mismo apagado limpio que usa `apagados`.
            if int_on && k < NISSE_INT { self.st[IISSE_INT + k] = pc; }
        }
        // La salida FINAL de la cadena. Las intermedias van aparte y bajo perilla (ISSE_INT_ON).
        self.st[IISSE] = pc;
        self.idx[IISSE] = 0;

        // ---- LA PARTE NO LINEAL: cuanto se pelean entre ellos ----
        //
        // Un mezclador lineal solo sabe hacia donde tira cada modelo, no CUANTO se contradicen.
        // Aqui se le da: si todos coinciden, la mezcla puede apostar fuerte; si estan repartidos,
        // conviene quedarse cerca del 50% aunque la suma ponderada diga otra cosa.
        //
        // Va en los huecos 41 y 42, que ya existian a peso cero para cuadrar el multiplo de 8
        // del SIMD: no cuesta ni una instruccion mas.
        // ⚠️ `&& !self.disp_apagada` — SE CALCULABA PARA BORRARLO. Desde la v7 las ranuras
        // IDISP/IDISP2 entran en la mascara de apagados (ver `familia_apagados`), asi que este
        // bloque hacia un min/max/suma sobre 43 entradas EN CADA BIT y treinta lineas mas abajo
        // el bucle de ablacion ponia las dos a cero. Trabajo tirado, bit a bit, desde la v7.
        //
        // Que quitarlo no puede cambiar la salida no es un argumento, esta MEDIDO (03-08, Mac,
        // 512 KiB de codigo):  v6 18.084 · v6+VC_DISP=0 18.066 · v7 18.066 · v7+VC_DISP=0 18.066
        // — o sea `v7 ≡ v6 + VC_DISP=0` byte a byte. La perilla ya no podia mover nada; lo unico
        // que quedaba era el coste de calcularlo.
        //
        // Lo destapo el control de «¿llegan de verdad las perillas al binario?» antes de lanzar
        // un barrido, no un perfilador. `mezclador.sh` llevaba barriendo VC_DISP —dos
        // compresiones por muestra y por talla— sobre una perilla que no podia informar de nada,
        // y su aviso de «empatan» no saltaba porque solo avisa cuando el defecto declarado NO
        // esta entre los empatados, y aqui estaba.
        if self.disp_on && !self.disp_apagada {
            let mut mn = i32::MAX;
            let mut mx = i32::MIN;
            let mut suma = 0i64;
            let mut n = 0i64;
            for i in 0..IDISP {
                // SIN el `i < NTAB` de antes. La condicion se escribio cuando los derivados no
                // se podian apagar, asi que apagar uno le quitaba la voz ante el mezclador pero
                // su valor seguia entrando AQUI — y la dispersion si votaba. La aportacion
                // medida salia subestimada. Lo encontro el CAMINO (P13), no un test.
                if self.apagados >> i & 1 != 0 { continue; }
                let v = self.st[i];
                if v < mn { mn = v; }
                if v > mx { mx = v; }
                suma += v as i64;
                n += 1;
            }
            let disp = if n > 0 { ((mx - mn) >> 1).clamp(0, 2047) } else { 0 };
            let media = if n > 0 { (suma / n) as i32 } else { 0 };
            self.st[IDISP] = disp;
            // el signo de la media multiplica: discrepar cuando la mayoria dice "1" no es lo
            // mismo que discrepar cuando dice "0".
            self.st[IDISP2] = ((media as i64 * disp as i64) >> 11).clamp(-2047, 2047) as i32;
        } else {
            self.st[IDISP] = 0;
            self.st[IDISP2] = 0;
        }

        // ---- ABLACION DE LOS DERIVADOS (34..42) ----
        //
        // Aqui, y no antes. Todos los `st[]` estan ya calculados, incluida la dispersion, que se
        // calcula LEYENDO los `st` de los demas: apagar un derivado mas arriba movería a otros
        // dos sin que nadie se enterara, y la medida diria otra cosa de la que pregunta.
        //
        // ⚠️ ESTO QUITA LA VOZ, NO EL COMPUTO. El modelo se sigue calculando —varios derivados se
        // alimentan entre si— asi que lo que se mide es CUANTO APORTA SU PREDICCION AL RATIO, no
        // lo que se ahorraria en tiempo quitandolo. Lo segundo es otra medida, en serie.
        // ⚠️ HASTA NCTX_MASCARA, NO HASTA NCTX. Con NCTX (153) este bucle desplazaba un u64 por
        // 64..152, que en release se enmascara: apagar el modelo 0 callaba ADEMAS las entradas 64
        // y 128 —de otros dos modelos— sin que nadie se enterara. O sea que toda ablacion movia
        // tres entradas y el numero se leia como si moviera una.
        if self.apagados >> NTAB != 0 {
            for i in NTAB..NCTX_MASCARA {
                if self.apagados >> i & 1 != 0 { self.st[i] = 0; }
            }
        }

        // ---- capa 1: cuatro mezcladores, cuatro criterios de especialización ----
        let b1 = self.hist[7] as usize;
        let b2 = self.hist[6] as usize;
        // en qué régimen está el modelo de coincidencias: muerto / recién enganchado / firme / muy firme
        let lb = match self.mm_len { 0 => 0, 1..=7 => 1, 8..=31 => 2, _ => 3 };
        // el de siempre: 8 bits del byte a medias + 1 bit de «hay coincidencia» = 512 juegos.
        self.sel[0] = ((c0 as usize) & 255) | (((self.mm_len > 0) as usize) << 8);
        // v10: dos bits mas de la ventana => 2048 juegos. Ver `nsel_de_modelo` para el por que
        // de que sean estos dos bits y no una señal con criterio.
        if self.mas_juegos {
            self.sel[0] |= ((self.hist[5] >> 6) as usize) << 9;
        }
        self.sel[1] = b1;                                                          // byte anterior
        self.sel[2] = (self.bpos as usize) * 4 + lb;                               // bit x confianza del match
        self.sel[3] = ((b1 >> 4) << 4) | (b2 >> 4);                                // dos bytes, a lo bruto
        self.sel2 = (self.bpos as usize) * 2 + ((self.mm_len > 0) as usize);

        // ---- los contextos de la cadena de calibración ----
        // Se calculan AQUI, antes de mezclar, y viven en `self.cx_v`. Antes se calculaban en
        // mitad de la cadena, con un `lb` recalculado identico al de arriba. Subirlos no cambia
        // ni un byte —no dependen de nada de la mezcla— y es lo que permite que la cola entera
        // sea un metodo aparte que `vc-replay` puede ejecutar tal cual.
        self.cx_v = [
            self.hist[7] as u32,
            (self.c0 & 0xFF) as u32,
            ((self.hist[7] as u32) << 8) | self.hist[6] as u32,
            ((lb << 7) | ((self.mm2_len > 0) as usize) << 6
                | ((self.mm3_len > 0) as usize) << 5 | (self.bpos as usize) << 2
                | ((self.mm4_len > 0) as usize) << 1 | (self.mm8_len > 0) as usize) as u32,
            ((((self.cgrp as usize) ^ ((self.cgrp >> 16) as usize)) & 0xFFFF)) as u32,
        ];
        self.mezcla()
    }

    /// LA COLA DEL PREDICTOR — capa 1, capa 2 y la cadena de calibracion.
    ///
    /// Esta en su propio metodo por un motivo que no es estetico: `vc-replay` ejecuta ESTE
    /// CODIGO, no una copia suya. Un replay que re-implementa la aritmetica es justo el modo de
    /// fallo caro que ya hemos pagado dos veces —el «oraculo» que resulto ser un bug, el atajo
    /// que elegia configuraciones nunca corridas—: cuadra consigo mismo y con nada mas.
    /// Compartiendo el metodo, el replay NO PUEDE divergir aunque se toque la mezcla, porque se
    /// toca para los dos a la vez.
    ///
    /// Lee `st`, `sel`, `sel2`, `cx_v`. Escribe `mp`, `mst`, `mixer_p`, `sse_in`, `sse_out`,
    /// `pr`. No toca ni un modelo, y esa es la propiedad que hace exacto al replay.
    ///
    /// Su pareja es [`Predictor::actualiza_mezcla`]: las dos juntas son todo lo que hay aguas
    /// abajo de `st[]`, y por tanto todo lo que un volcado permite explorar barato.
    fn mezcla(&mut self) -> i32 {
        let mut dot2: i64 = 0;
        let simd = self.simd;
        for k in 0..NMIX {
            let base = self.sel[k] * NCTXP;
            let dot = punto(&self.w[k][base..base + NCTXP], &self.st, simd);
            let pk = self.t.squash((dot >> 16) as i32);
            self.mp[k] = pk;
            let s = self.t.stretch(pk);
            self.mst[k] = s;
            dot2 += (self.w2[self.sel2 * NMIX + k] as i64) * (s as i64);
        }
        // ---- capa 2: a cuál de los cuatro hacer caso aquí y ahora ----
        let p1 = self.t.squash((dot2 >> 16) as i32);
        self.mixer_p = p1;

        // ---- cadena de calibración ----
        // Cada etapa corrige la salida de la anterior con un contexto distinto. La mezcla
        // 3:1 con su entrada es lo que impide que una etapa mal informada estropee lo que
        // ya venía bien: como mucho puede mover la predicción un cuarto del camino.
        let mut pr = p1;
        let bp = self.bpos as usize;
        for k in 0..self.n_sse {
            let cxk = self.cx_v[k] as usize;
            let pa = {
                let (t, sse) = (self.t, &mut self.sse[k]);
                sse.p(pr, cxk, t)
            };
            // Peso aprendido: si la etapa k no aporta, su peso cae a cero y deja de tocar
            // nada. Seguro por construccion — el peor caso es perder el tiempo, no ratio.
            self.sse_in[k] = pr;
            self.sse_out[k] = pa;
            let w = self.sse_w[k * 8 + bp];
            pr = pr + (((pa - pr) as i64 * w as i64) >> 16) as i32;
        }
        self.pr = pr.clamp(1, 4094);
        self.pr
    }

    /// EL APRENDIZAJE DE LA COLA — pareja de [`Predictor::mezcla`], y por el mismo motivo:
    /// `vc-replay` la ejecuta tal cual en vez de re-implementarla.
    ///
    /// Aqui esta la propiedad que hace posible todo el atajo, y conviene leerla despacio: los
    /// tres bloques aprenden de errores DISTINTOS y ninguno mira aguas arriba. La capa 1 sobre
    /// `mp[k]` (lo suyo), la capa 2 sobre `mixer_p` (lo suyo), la calibracion sobre `pr` (lo
    /// suyo). Ninguno toca un StateMap, un cubo ni la cadena ISSE. Por eso cambiar `mix_lr`,
    /// `decay`, `n_sse` o `sse_w` no puede mover `st[]` ni un bit, y por eso un volcado de `st[]`
    /// vale para cualquier combinacion de los cuatro.
    fn actualiza_mezcla(&mut self, bit: i32) {
        // capa 1: cada uno aprende sobre SU PROPIA salida, no sobre la del conjunto. Es lo que
        // los mantiene distintos: si todos persiguieran el error final acabarían siendo el
        // mismo mezclador cuatro veces.
        let simd = self.simd;
        for k in 0..NMIX {
            let err = (bit << 12) - self.mp[k];
            let base = self.sel[k] * NCTXP;
            let (w, st) = (&mut self.w[k][base..base + NCTXP], &self.st);
            actualiza_pesos(w, st, err, simd, self.decay, self.mix_lr);
        }
        // capa 2: aprende el reparto de confianza entre los cuatro. Paso más corto (>>13) que
        // la capa 1 porque sus entradas ya son predicciones fuertes y son solo 64 pesos.
        let err2 = (bit << 12) - self.mixer_p;
        let base2 = self.sel2 * NMIX;
        for k in 0..NMIX {
            let dw = (self.mst[k] * err2) >> 13;
            self.w2[base2 + k] = (self.w2[base2 + k] + dw).clamp(-(1 << 20), 1 << 20);
        }
        // update de la cadena de calibración
        for k in 0..self.n_sse { self.sse[k].update(bit); }
        // ...y sus pesos. El error se mide sobre la salida FINAL de la cadena, y cada etapa
        // se mueve en la direccion en que habria acercado esa salida al bit real.
        let errf = (bit << 12) - self.pr;
        let bp = self.bpos as usize;
        for k in 0..self.n_sse {
            let d = self.sse_out[k] - self.sse_in[k];
            let dw = ((errf as i64 * d as i64) >> 13) as i32;
            let i = k * 8 + bp;
            self.sse_w[i] = (self.sse_w[i] + dw).clamp(0, 65536);
        }
    }

    /// Devuelve la cola a su estado de arranque y le fija una configuracion.
    ///
    /// POR QUE EXISTE. La primera version del replay creaba un `Predictor` entero por
    /// configuracion, y medido salio que eso era el grueso del coste: `Predictor::new` reserva y
    /// pone a cero mas de 100 MB de tablas de modelos —los cinco `mm_hashtab` solos son 72 MB—
    /// que la cola NO TOCA. Reutilizando el predictor y reiniciando solo la cola, lo que se paga
    /// por configuracion es lo que de verdad cuesta la configuracion.
    ///
    /// ⚠️ Tiene que dejar la cola EXACTAMENTE como la deja el constructor, o la primera
    /// configuracion de una tanda mediria distinto que las demas y el barrido saldria torcido de
    /// un modo dificil de ver. Los valores de aqui son los mismos del struct literal, y estan
    /// duplicados a proposito con esta nota al lado.
    fn reinicia_cola(&mut self, cfg: &CfgMezcla, w0: &[Vec<i32>], t: &'a Tables) {
        for k in 0..NMIX { self.w[k].copy_from_slice(&w0[k]); }
        for x in self.w2.iter_mut() { *x = (1 << 16) / NMIX as i32; }
        self.sse = vec![
            Apm::nueva(1024, t), Apm::nueva(256, t), Apm::nueva(1 << 16, t),
            Apm::nueva(1 << 9, t), Apm::nueva(1 << 16, t),
        ];
        let w0sse = cfg.sse_w0.unwrap_or(49152);
        for x in self.sse_w.iter_mut() { *x = w0sse; }
        self.mp = [2048; NMIX];
        self.mst = [0; NMIX];
        self.sse_in = [0; 8];
        self.sse_out = [0; 8];
        self.mixer_p = 2048;
        self.pr = 2048;
        if let Some(x) = cfg.mix_lr { self.mix_lr = x; }
        if let Some(x) = cfg.decay { self.decay = x; }
        if let Some(x) = cfg.n_sse { self.n_sse = x.clamp(1, 5); }
    }

    /// Cuánta memoria gasta cada modelo, cuánta usa, y cuánto se le hace caso.
    /// Los pares que se pisan. Sale por stderr en formato de tabla para que lo lea el banco.
    fn correlacion(&self, entrada: usize) {
        let n = self.corr_n;
        if n < 2.0 { eprintln!("CORR: no hay bits suficientes"); return }
        let sp = |i: usize, j: usize| -> f64 {
            let (a, b) = if i <= j { (i, j) } else { (j, i) };
            self.corr_sp[a * NCTX + b]
        };
        // Pearson a partir de las sumas. La desviacion se saca de la diagonal.
        let sd: Vec<f64> = (0..NCTX)
            .map(|i| (sp(i, i) / n - (self.corr_s[i] / n).powi(2)).max(0.0).sqrt())
            .collect();
        let r = |i: usize, j: usize| -> f64 {
            let d = sd[i] * sd[j];
            // Un modelo que NUNCA se mueve tiene desviacion cero y no correlaciona con nada.
            // Devolver 0 seria mentir por omision: se marca aparte.
            if d <= 0.0 { return f64::NAN }
            ((sp(i, j) / n) - (self.corr_s[i] / n) * (self.corr_s[j] / n)) / d
        };
        eprintln!("\nCORRELACION ENTRE MODELOS  ({} bytes, {} bits)", entrada, n as u64);
        eprintln!("# PARES QUE SE PISAN — dos modelos que predicen lo mismo son uno y medio,");
        eprintln!("# no dos: cuestan el doble de busquedas en tabla y aportan poco mas que uno.");
        eprintln!("#r\ta\tb\tnombre_a\tnombre_b\tapagado_a\tapagado_b");
        let mut pares: Vec<(f64, usize, usize)> = Vec::new();
        let mut quietos: Vec<usize> = Vec::new();
        for i in 0..NCTX {
            if sd[i] <= 0.0 { quietos.push(i); continue }
            for j in (i + 1)..NCTX {
                if sd[j] <= 0.0 { continue }
                let v = r(i, j);
                if v.is_finite() { pares.push((v, i, j)); }
            }
        }
        pares.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap());
        let ap = |i: usize| if i < NTAB && (self.apagados >> i & 1) != 0 { "si" } else { "no" };
        for (v, i, j) in pares.iter().take(40) {
            eprintln!("CORR\t{:.4}\t{}\t{}\t{}\t{}\t{}\t{}",
                      v, i, j, NOMBRES[*i], NOMBRES[*j], ap(*i), ap(*j));
        }
        let n95 = pares.iter().filter(|(v, ..)| v.abs() > 0.95).count();
        let n90 = pares.iter().filter(|(v, ..)| v.abs() > 0.90).count();
        eprintln!("RESUMEN\t{}\tpares\t{}\tpor encima de 0,95\t{}\tpor encima de 0,90",
                  pares.len(), n95, n90);
        if !quietos.is_empty() {
            eprintln!("QUIETOS\t{}\t{}", quietos.len(),
                      quietos.iter().map(|&i| NOMBRES[i]).collect::<Vec<_>>().join(" "));
            eprintln!("# Un modelo que no se mueve NUNCA no correlaciona con nada, y eso no es");
            eprintln!("# «independiente»: es que no predice. Se dice aparte para no contarlo como");
            eprintln!("# aportacion.");
        }
    }

    fn censo(&self, entrada: usize) {
        eprintln!("\nCENSO DE TABLAS  ({} bytes de entrada, {} bits codificados)", entrada, self.bits_vistos);
        eprintln!("{:>3}  {:<22} {:>5} {:>9} {:>12} {:>12} {:>7} {:>9} {:>9} {:>9}",
                  "#", "entrada", "bits", "MiB", "ranuras", "usadas", "ocup%", "bpb-solo", "|w|", "bits/MiB");
        let mut mib_total = 0.0f64;
        for i in 0..NTAB {
            let tab = &self.estados[i];
            let ranuras = tab.len() / 16;
            let usadas = (0..ranuras).filter(|&r| tab[r * 16..r * 16 + 16].iter().any(|&b| b != 0)).count();
            let mib = tab.len() as f64 / (1024.0 * 1024.0);
            mib_total += mib;
            // cuánto se apoya el mezclador 0 en este modelo
            let w: i64 = (0..self.nsel[0]).map(|sx| (self.w[0][sx * NCTXP + i]).unsigned_abs() as i64).sum();
            let wm = w as f64 / self.nsel[0] as f64;
            let bpb = self.coste[i] / (self.bits_vistos.max(1) as f64 / 8.0);
            // cuántos bits AHORRA frente a no predecir nada (1 bit por bit), por MiB gastado
            let ahorro = (self.bits_vistos as f64) - self.coste[i];
            eprintln!("{:>3}  {:<22} {:>5} {:>9.1} {:>12} {:>12} {:>6.1}% {:>9.4} {:>9.0} {:>9.0}",
                      i, nombre_ctx(i), TAB_BITS[i], mib, ranuras, usadas,
                      100.0 * usadas as f64 / ranuras as f64, bpb, wm, ahorro / mib.max(1e-9));
        }
        // ⚠️ AQUI SE MORIA EL CENSO. `NOMBRES` tiene IRACHA (45) entradas y este bucle llega a
        // NCTX (153): en cuanto pisaba la 45 —la primera vista de racha— el compresor PANICABA
        // con «index out of bounds» y devolvia «Error: worker panic». O sea que el censo, que es
        // de donde salio el recorte de memoria de las tablas, llevaba enseñando 45 de 153
        // entradas y matando la ejecucion en la 46. `nombre_ctx()` existia justamente para esto
        // y este sitio no la usaba.
        for i in NTAB..NCTX {
            let w: i64 = (0..self.nsel[0]).map(|sx| (self.w[0][sx * NCTXP + i]).unsigned_abs() as i64).sum();
            let bpb = self.coste[i] / (self.bits_vistos.max(1) as f64 / 8.0);
            eprintln!("{:>3}  {:<22} {:>5} {:>9} {:>12} {:>12} {:>7} {:>9.4} {:>9.0}",
                      i, nombre_ctx(i), "-", "-", "-", "-", "-", bpb, w as f64 / self.nsel[0] as f64);
        }
        eprintln!("TOTAL tablas de contexto: {:.1} MiB (+ DMC {} MiB)", mib_total,
                  (DMC_NODOS * 12) >> 20);
    }

    fn update(&mut self, bit: i32) {
        // ── EL VOLCADO ─────────────────────────────────────────────────────────────────────────
        // Va AQUI y no en `p()` por una razon y no por comodidad: aqui se conoce el bit, y el bit
        // es la mitad del registro. Volcarlo en `p()` obligaria a partir cada muestra en dos
        // escrituras que luego hay que volver a casar, y un registro casado a mano es un sitio
        // donde meter un desfase de uno sin enterarse.
        //
        // `st[]` viene ya ablacionado y con la cadena ISSE resuelta, que es EXACTAMENTE lo que
        // consume la capa 1 dos lineas mas abajo en `p()`. O sea que lo que se vuelca es la
        // entrada real del mezclador, no una reconstruccion de ella.
        if self.volcar.is_some() {
            let mut buf = [0u8; REG_VOLCADO];
            buf[0] = bit as u8;
            buf[1] = self.bpos as u8;
            buf[2] = self.sel2 as u8;
            for k in 0..NMIX {
                buf[4 + k * 2..6 + k * 2].copy_from_slice(&(self.sel[k] as u16).to_le_bytes());
            }
            for k in 0..5 {
                buf[12 + k * 4..16 + k * 4].copy_from_slice(&self.cx_v[k].to_le_bytes());
            }
            for i in 0..NCTX {
                let v = self.st[i];
                // ⚠️ NO SE TRUNCA EN SILENCIO. Todo `st` sale de squash/stretch o de un clamp a
                // ±2047, asi que i16 sobra de largo; pero si algun dia una entrada nueva se sale
                // del rango, un `as i16` la envolveria y el replay cuadraria con un numero
                // inventado. Se para y se dice.
                if v < i16::MIN as i32 || v > i16::MAX as i32 {
                    panic!("VC_VOLCAR: st[{i}]={v} no cabe en i16. El volcado tendria que \
cambiar de tipo; truncar aqui daria un replay que cuadra con datos falsos.");
                }
                buf[32 + i * 2..34 + i * 2].copy_from_slice(&(v as i16).to_le_bytes());
            }
            use std::io::Write;
            self.volcar.as_mut().unwrap().write_all(&buf).expect("VC_VOLCAR: escribiendo");
        }
        if self.costepos.is_some() {
            use std::io::Write;
            let pr = (self.pr.clamp(1, 4094) as f64) / 4096.0;
            let q = if bit == 1 { pr } else { 1.0 - pr };
            self.cb -= q.log2();
            self.cbn += 1;
            if self.cbn == 8 {
                let v = (self.cb as f32).to_le_bytes();
                let w = self.costepos.as_mut().unwrap();
                let _ = w.write_all(&v);
                let _ = w.flush();
                self.cb = 0.0;
                self.cbn = 0;
            }
        }
        if self.corr {
            // La diagonal guarda sum(x²), asi que no hace falta un vector aparte. El bucle es
            // triangular: la correlacion es simetrica y calcular la mitad de abajo seria pagar
            // dos veces por el mismo numero.
            self.corr_n += 1.0;
            for i in 0..NCTX {
                let a = self.st[i] as f64;
                self.corr_s[i] += a;
                for j in i..NCTX { self.corr_sp[i * NCTX + j] += a * self.st[j] as f64; }
            }
        }
        if self.censo {
            // coste en bits de cada modelo si codificara él solo. La probabilidad viene del
            // stretch que ya tenemos calculado, así que no se toca nada del predictor.
            self.bits_vistos += 1;
            for i in 0..NCTX {
                let pr = self.t.squash(self.st[i]).clamp(1, 4094) as f64 / 4096.0;
                let q = if bit == 1 { pr } else { 1.0 - pr };
                self.coste[i] -= q.log2();
            }
        }
        // ── F6·Q6 · EN UN BYTE SALTADO NO APRENDE NADIE MAS QUE LA ETAPA ──────────────────────
        //
        // Este `else` es LA MITAD DEL AHORRO. `p()` se ahorra preguntar al conjunto; esto se
        // ahorra enseñarle, que cuesta lo mismo o mas: un StateMap por modelo, la cadena ISSE
        // entera y los cuatro mezcladores con sus 160 carriles, ocho veces por byte.
        //
        // ⚠️ Lo que se paga a cambio: las tablas NO APRENDEN de los bytes saltados. Es una
        // decision, no un descuido —el descompresor hace exactamente lo mismo, asi que los dos
        // lados siguen viendo las mismas tablas— pero significa que al salir de una racha larga
        // los modelos vuelven sin haber visto ese tramo. Ese es el coste en bytes que mide el
        // falsador (2), y es la razon de que el umbral sea 32 y no 16.
        if self.salto {
            self.sse_salto.update(bit);
        } else {
            // (a) el StateMap del modelo aprende la traducción estado -> probabilidad, con tasa
            //     adaptativa 1/n: rápido al principio, estable al madurar.
            // (b) la ranura de contexto avanza su historia de bits.
            let target = bit * 65535;
            for i in 0..NTAB {
                if self.apagados >> i & 1 != 0 { continue; }
                let e = self.est[i] as usize;
                let j = (i << 8) | e;
                let v = self.sm[j];
                let n = (v >> 16) as usize;
                let cur = (v & 0xFFFF) as i32;
                // `& 0xFFFF` trunca exactamente igual que el `as u16` que había antes.
                let np = (cur + (((target - cur) * self.dt[n]) >> 16)) as u32 & 0xFFFF;
                // El tope decide cuanto se CONGELA un contexto maduro: con n topado, el paso 1/n
                // deja de bajar y el modelo conserva capacidad de reaccion. Un tope alto favorece
                // al dato estacionario; uno bajo, al que cambia.
                let nn = if n < self.lim { n as u32 + 1 } else { n as u32 };
                self.sm[j] = (nn << 16) | np;
                // EL MAPA DE LA RACHA aprende igual que `sm`: paso 1/n con tope. Solo se toca la
                // casilla que SE CONSULTO —si no habia racha no hay nada que aprender ahi, y
                // actualizarla la envenenaria con la respuesta de otra situacion.
                let jr = self.run_ix[i];
                if jr != usize::MAX {
                    let v = self.run_sm[jr];
                    let n = (v >> 16) as usize;
                    let cur = (v & 0xFFFF) as i32;
                    let np = (cur + (((target - cur) * self.dt[n]) >> 16)) as u32 & 0xFFFF;
                    let nn = if n < self.lim { n as u32 + 1 } else { n as u32 };
                    self.run_sm[jr] = (nn << 16) | np;
                    self.run_ix[i] = usize::MAX;
                }
                let jt = self.tres_ix[i];
                if jt != usize::MAX {
                    let v = self.tres_sm[jt];
                    let n = (v >> 16) as usize;
                    let cur = (v & 0xFFFF) as i32;
                    let np = (cur + (((target - cur) * self.dt[n]) >> 16)) as u32 & 0xFFFF;
                    let nn = if n < self.lim { n as u32 + 1 } else { n as u32 };
                    self.tres_sm[jt] = (nn << 16) | np;
                    self.tres_ix[i] = usize::MAX;
                }
                let ix = self.idx[i] as usize;
                self.estados[i][ix] = self.t.h.nex[e][bit as usize];
            }
            // update de la cadena ISSE: cada etapa aprende sobre SU propia salida
            for k in 0..NISSE {
                let err = (bit * 4095 - self.t.squash(self.isse_pout[k])) * self.isse_lr;
                let e = self.isse_s[k] as usize;
                let w0 = self.isse_w[k][e * 2] + (((err * self.isse_pin[k]) + (1 << 12)) >> 13);
                let w1 = self.isse_w[k][e * 2 + 1] + ((err + 16) >> 5);
                self.isse_w[k][e * 2] = w0.clamp(-(1 << 19), 1 << 19);
                self.isse_w[k][e * 2 + 1] = w1.clamp(-(1 << 19), 1 << 19);
                let ix = self.isse_idx[k] as usize;
                self.isse_est[k][ix] = self.t.h.nex[e][bit as usize];
            }

            self.actualiza_mezcla(bit);
        }

        // match model: si acertó el bit, sigue; si no, rompe
        if self.mm_len > 0 && self.mm_pred_bit != bit { self.mm_len = 0; }
        if self.mm2_len > 0 && self.mm2_pred_bit != bit { self.mm2_len = 0; }
        if self.mm3_len > 0 && self.mm3_pred_bit != bit { self.mm3_len = 0; }
        if self.mm4_len > 0 && self.mm4_pred_bit != bit { self.mm4_len = 0; }
        if self.mm8_len > 0 && self.mm8_pred_bit != bit { self.mm8_len = 0; }

        // El DMC es otro modelo: en un byte saltado tampoco anda. Su nodo se queda donde estaba
        // y sigue igual en los dos lados, que es lo unico que la correccion exige.
        if !self.salto { self.dmc.update(bit, self.bpos); }
        // avanza estado de bit
        self.c0 = (self.c0 << 1) | bit as u32;
        self.nib = (self.nib << 1) | bit as u32;
        self.jnib += 1;
        self.bpos += 1;
        // A mitad de byte se re-fijan los cubos con lo que ya se sabe del byte. Saltado, no hay
        // a quien darselos: `fija_cubos` es la parte cara y aqui es donde se ahorra la segunda
        // de las dos veces por byte.
        if self.bpos == 4 && !self.salto { self.fija_cubos(); }
        if self.bpos == 8 {
            let byte = (self.c0 & 0xFF) as u8;
            // F6·Q6: se cobra la foto tomada al empezar ESTE byte. Solo suma contadores.
            #[cfg(feature = "cobertura")]
            {
                use std::sync::atomic::Ordering::Relaxed;
                cobertura::TOTAL.fetch_add(1, Relaxed);
                for (i, u) in cobertura::UMBRAL.iter().enumerate() {
                    if self.cob_racha >= *u {
                        cobertura::CUB[i].fetch_add(1, Relaxed);
                        if self.cob_pred == byte as i32 { cobertura::ACI[i].fetch_add(1, Relaxed); }
                    }
                    if self.cob_racha5 >= *u { cobertura::CUB5[i].fetch_add(1, Relaxed); }
                }
            }
            // LA MEMORIA DE BYTE. Se actualiza con el cubo del PRIMER nibble, que es el que
            // identifica al contexto a nivel de byte: el del segundo ya depende del valor del
            // primero, o sea que es otro contexto.
            // ⚠️ `!self.salto` es OBLIGATORIO aqui, y no por ahorrar: la memoria de racha se
            // indexa por `bucket0`, y en un byte saltado `fija_cubos` no ha corrido, asi que
            // `bucket0` es el del byte ANTERIOR. Escribir con el corrompe la memoria de otro
            // contexto. O corren los dos o no corre ninguno.
            if *RACHA_ON && !self.salto {
                for i in 0..NTAB {
                    if self.apagados >> i & 1 != 0 { continue }
                    let m = self.bucket0[i] as usize / 4;
                    if m + 3 >= self.memo[i].len() { continue }
                    if self.memo[i][m] == byte {
                        // satura en 255: de 255 a 256 repeticiones no hay informacion nueva y
                        // dar la vuelta al contador la habria destruido entera.
                        if self.memo[i][m + 1] < 255 { self.memo[i][m + 1] += 1; }
                    } else {
                        // COLA DE TRES DISTINTOS, no de tres ultimos: si el byte ya estaba en la
                        // cola sube al frente y los de delante bajan; si no, entra y el mas viejo
                        // se cae. Guardar «los tres ultimos» a secas daria [a,a,a] en cuanto haya
                        // una racha, que es justo el caso que la racha YA cubre.
                        let (b2, b3) = (self.memo[i][m + 2], self.memo[i][m + 3]);
                        let viejo = self.memo[i][m];
                        if byte == b2 { self.memo[i][m + 2] = viejo; }
                        else { self.memo[i][m + 3] = b2; self.memo[i][m + 2] = viejo; }
                        let _ = b3;
                        self.memo[i][m] = byte;
                        self.memo[i][m + 1] = 1;
                    }
                }
            }
            self.mm_buf.push(byte);
            if self.mm_len > 0 {
                self.mm_ptr += 1;
                if (self.mm_ptr as usize) < self.mm_buf.len() && self.mm_len < 65535 { self.mm_len += 1; }
                else if self.mm_ptr as usize >= self.mm_buf.len() { self.mm_len = 0; }
            }
            if self.mm2_len > 0 {
                self.mm2_ptr += 1;
                if (self.mm2_ptr as usize) < self.mm_buf.len() && self.mm2_len < 65535 { self.mm2_len += 1; }
                else if self.mm2_ptr as usize >= self.mm_buf.len() { self.mm2_len = 0; }
            }
            if self.mm3_len > 0 {
                self.mm3_ptr += 1;
                if (self.mm3_ptr as usize) < self.mm_buf.len() && self.mm3_len < 65535 { self.mm3_len += 1; }
                else if self.mm3_ptr as usize >= self.mm_buf.len() { self.mm3_len = 0; }
            }
            if self.mm4_len > 0 {
                self.mm4_ptr += 1;
                if (self.mm4_ptr as usize) < self.mm_buf.len() && self.mm4_len < 65535 { self.mm4_len += 1; }
                else if self.mm4_ptr as usize >= self.mm_buf.len() { self.mm4_len = 0; }
            }
            if self.mm8_len > 0 {
                self.mm8_ptr += 1;
                if (self.mm8_ptr as usize) < self.mm_buf.len() && self.mm8_len < 65535 { self.mm8_len += 1; }
                else if self.mm8_ptr as usize >= self.mm_buf.len() { self.mm8_len = 0; }
            }
            let pos = self.mm_buf.len() as u32;
            // La clase del byte; los tramos iguales se aplastan en uno solo para que la
            // forma no dependa de cuántos espacios o cuántas letras seguidas haya.
            let g = if byte.is_ascii_lowercase() { b'a' }
                else if byte.is_ascii_uppercase() { b'A' }
                else if byte.is_ascii_digit() { b'0' }
                else if byte == b' ' || byte == b'\t' { b' ' }
                else { byte };
            if (self.cgrp & 0xFF) as u8 != g { self.cgrp = (self.cgrp << 8) | g as u64; }
            // LA PILA DE ANIDAMIENTO. Sube al abrir, baja al cerrar, con peso propio por tipo.
            self.anid = self.anid.wrapping_add(match byte {
                b'(' => PESOS_ANID[0],  b')' => -PESOS_ANID[0],
                b'[' => PESOS_ANID[1],  b']' => -PESOS_ANID[1],
                b'{' => PESOS_ANID[2],  b'}' => -PESOS_ANID[2],
                b'<' => PESOS_ANID[3],  b'>' => -PESOS_ANID[3],
                _ => 0,
            });
            for k in 0..NSEP {
                if byte == SEPS[k] {
                    self.sep_prev[k] = self.sep_last[k];
                    self.sep_last[k] = pos;
                    self.sep_n[k] = self.sep_n[k].saturating_add(1);
                }
            }
            // respaldo binario: si la distancia entre apariciones de este byte se repite tres
            // veces seguidas, vota como ancho de rejilla; el candidato con 12 votos se adopta.
            let c = byte as usize;
            let r = pos.wrapping_sub(self.cpos1[c]);
            if r > 1 && r < 65536
                && r == self.cpos1[c].wrapping_sub(self.cpos2[c])
                && r == self.cpos2[c].wrapping_sub(self.cpos3[c])
            {
                if r == self.rlen1 { self.rcount1 += 1; }
                else if r == self.rlen2 { self.rcount2 += 1; }
                else if self.rcount1 > self.rcount2 { self.rlen2 = r; self.rcount2 = 1; }
                else { self.rlen1 = r; self.rcount1 = 1; }
            }
            if self.rcount1 > 12 && self.rlen != self.rlen1 { self.rlen = self.rlen1; self.rcount1 = 0; self.rcount2 = 0; }
            if self.rcount2 > 12 && self.rlen != self.rlen2 { self.rlen = self.rlen2; self.rcount1 = 0; self.rcount2 = 0; }
            self.cpos3[c] = self.cpos2[c]; self.cpos2[c] = self.cpos1[c]; self.cpos1[c] = pos;

            // el indirecto aprende: "detrás de <byte anterior> vino <este byte>"
            let prev = self.hist[7] as usize;
            self.ind1[prev] = (self.ind1[prev] << 8) | byte as u32;
            let par = ((self.hist[6] as usize) << 8) | self.hist[7] as usize;
            self.ind2[par] = (self.ind2[par] << 8) | byte as u32;
            for k in 0..7 { self.hist[k] = self.hist[k + 1]; }
            self.hist[7] = byte;
            // contexto de palabra: acumula en runs alfanuméricos, se reinicia fuera
            if byte.is_ascii_alphabetic() {
                self.wmin = (self.wmin ^ (byte | 0x20) as u32).wrapping_mul(0x01000193);
            } else if self.wmin != 0 {
                self.wmin_prev = self.wmin;
                self.wmin = 0;
            }
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                self.word = (self.word ^ byte as u32).wrapping_mul(0x01000193);
                if self.wlen < WMAX { self.wbuf[self.wlen] = byte; self.wlen += 1; }
            } else {
                // se acaba una palabra: pasa a ser "la anterior"
                if self.word != 0 {
                    // la raiz se calcula UNA vez, al cerrar la palabra, no en cada bit
                    let r = raiz(&self.wbuf[..self.wlen]);
                    self.raiz_prev2 = self.raiz_prev;
                    self.raiz_prev = r;
                    self.word_prev2 = self.word_prev;
                    self.word_prev = self.word;
                }
                self.word = 0;
                self.wlen = 0;
            }
            self.c0 = 1; self.bpos = 0;
            self.set_byte_contexts();
        }
    }
}

// ---------- codificador aritmético binario de 32 bits (carryless, estilo lpaq) ----------
struct Encoder { x1: u32, x2: u32, out: Vec<u8>, ent: f64, bits: u64 }
impl Encoder {
    fn new() -> Self { Encoder { x1: 0, x2: 0xFFFF_FFFF, out: Vec::new(), ent: 0.0, bits: 0 } }
    fn encode(&mut self, bit: i32, p: i32) {
        // p = P(bit=1) en 12 bits (1..4094)
        // M5: la ENTROPIA de lo que el modelo predijo, que es el minimo teorico que podria
        // ocupar esto. Compararla con los bytes que salen de verdad da la perdida del
        // codificador. Solo se acumula si VC_ENTROPIA esta puesta: en el camino normal esto
        // es una comparacion contra un bool y no se toca ni un flotante.
        if ent_on() {
            let pr = if bit == 1 { p as f64 / 4096.0 } else { 1.0 - p as f64 / 4096.0 };
            self.ent += -pr.max(1e-12).log2();
            self.bits += 1;
        }
        let range = (self.x2 - self.x1) as u64;
        let xmid = self.x1 + ((range * p as u64) >> 12) as u32;
        if bit == 1 { self.x2 = xmid; } else { self.x1 = xmid + 1; }
        while (self.x1 ^ self.x2) & 0xFF00_0000 == 0 {
            self.out.push((self.x2 >> 24) as u8);
            self.x1 <<= 8; self.x2 = (self.x2 << 8) | 0xFF;
        }
    }
    fn flush(mut self) -> Vec<u8> {
        // emite bytes suficientes para desambiguar
        self.out.push((self.x1 >> 24) as u8);
        self.out.push((self.x1 >> 16) as u8);
        self.out.push((self.x1 >> 8) as u8);
        self.out.push(self.x1 as u8);
        if ent_on() && self.bits == 0 {
            eprintln!("[M5] se pidio la entropia y no se codifico NI UN BIT: el interruptor no \
llego al codificador. Esto es un fallo de cableado, no una medida de cero.");
        }
        if ent_on() && self.bits > 0 {
            let teorico = self.ent / 8.0;
            let real = self.out.len() as f64;
            eprintln!("[M5] bits={} entropia={:.0} B  salida={:.0} B  perdida={:+.4}% ({:.4} bits/bit de sobrecoste)",
                      self.bits, teorico, real, 100.0 * (real - teorico) / teorico,
                      (real * 8.0 - self.ent) / self.bits as f64);
        }
        self.out
    }
}
struct Decoder<'a> { x1: u32, x2: u32, x: u32, inp: &'a [u8], pos: usize }
impl<'a> Decoder<'a> {
    fn new(inp: &'a [u8]) -> Self {
        let mut x = 0u32;
        let mut pos = 0;
        for _ in 0..4 { x = (x << 8) | (*inp.get(pos).unwrap_or(&0) as u32); pos += 1; }
        Decoder { x1: 0, x2: 0xFFFF_FFFF, x, inp, pos }
    }
    fn decode(&mut self, p: i32) -> i32 {
        let range = (self.x2 - self.x1) as u64;
        let xmid = self.x1 + ((range * p as u64) >> 12) as u32;
        let bit = if self.x <= xmid { 1 } else { 0 };
        if bit == 1 { self.x2 = xmid; } else { self.x1 = xmid + 1; }
        while (self.x1 ^ self.x2) & 0xFF00_0000 == 0 {
            self.x1 <<= 8; self.x2 = (self.x2 << 8) | 0xFF;
            self.x = (self.x << 8) | (*self.inp.get(self.pos).unwrap_or(&0) as u32);
            self.pos += 1;
        }
        bit
    }
}

/// Comprime un bloque con el modelo de mezcla de contextos. Todo aritmetica entera:
/// la salida es byte-identica en cualquier arquitectura.
/// Cuanto ocupa `data` con esa familia. Sin cabecera y sin censo: solo se compara.
fn tam_sonda(t: &Tables, data: &[u8], familia: u8) -> usize {
    let mut pred = Predictor::new(t, data.len(), familia, modelo_a_escribir(), perfil_a_escribir());
    pred.set_byte_contexts();
    let mut enc = Encoder::new();
    for &byte in data {
        for k in (0..8).rev() {
            let bit = ((byte >> k) & 1) as i32;
            let p = pred.p();
            enc.encode(bit, p);
            pred.update(bit);
        }
    }
    enc.flush().len()
}

/// Elige familia COMPRIMIENDO una rebanada con cada una, no adivinando por estadisticas.
///
/// La rebanada sale de un cuarto del bloque, no del principio: las cabeceras y el arranque en
/// frio no se parecen al resto del fichero, y elegir por ellos es elegir por la excepcion.
///
/// El empate se rompe SIEMPRE hacia la familia mas baja (`<`, no `<=`), para que el mismo
/// dato de la misma decision en cualquier maquina. Determinista es un requisito duro aqui, no
/// una preferencia.
/// La familia que se usa cuando NO se sondea. Las dos extra encendidas: gana en 83 de los 92
/// ficheros del banco. ⚠️ No es «la mejor»: es que la diferencia entre las cuatro es 0,005%
/// —12 bytes sobre 223.000 en jpg—, asi que «la mejor» no es una categoria util aqui.
///
/// ❌ NEGATIVO MEDIDO 02-08: «QUE LA ELIJA EL CARRIL» NO FUNCIONA, Y LA CAUSA IMPORTA.
///
/// Al fijarla en 3 quedaba `bin` pagando 0,227% y 0,275% en dos de sus tres cortes —el dato
/// binario prefiere la familia 0, que ademas es la mas barata— asi que parecia obvio pasarle el
/// carril al codec y darle 0 a `LANE_OTHER`. Implementado y medido con la ida y vuelta entera:
///
/// ```text
///     bin --max      389.674  ->  389.779   +0,027%   <- SUBE, no baja
///     png --max    2.707.421  -> 2.707.051  -0,014%
///     dup --max    2.870.616  -> 2.870.365  -0,009%
///     los demas                              0,000% o dentro del ruido
///     neto sobre 31,4 MB: -545 B  =  -0,0017%, o sea NADA
/// ```
///
/// 🔑 POR QUE FALLA: el carril que llega al codec NO es el del fichero. `lane_for(ruta)` mira la
/// extension, pero lo que se le pasa al codec es el carril del TROZO, decidido con
/// `parece_texto_bytes(&trozo)` trozo a trozo (`vc-core/src/lib.rs`, `add_chunk`). Un `.bin` no
/// entra entero por `LANE_OTHER`: sus trozos se clasifican uno a uno, y muchos parecen texto. El
/// 0,275% de `bin` estaba medido sobre el TIPO comprimido de una pieza, y esa medida no viaja al
/// sitio donde se toma la decision.
///
/// Se revierte. Para recuperar ese 0,275% haria falta una señal que describa el TROZO, no el
/// fichero — y eso es otra medida, no un parametro que pasar.
const FAMILIA_FIJA: u8 = 3;

/// `VC_SONDA=1` vuelve a encender la sonda de familias. **Apagada por defecto desde el 02-08.**
///
/// 🚨 NO ES UN AJUSTE, ES QUE NO COMPRA NADA. Medido en serie contra la familia fija:
///
/// ```text
///     tipo    bloque    con sonda   familia fija    reloj      bytes sonda / fija
///     codigo  256 KiB     16,43 s        3,57 s   +360,2%       28.597 / 28.596
///     codigo    1 MiB     20,05 s       13,28 s    +51,0%       66.861 / 66.860
///     codigo    8 MiB     88,37 s       80,96 s     +9,2%     562.353 / 562.353
///     sql     256 KiB     16,16 s        3,51 s   +360,4%       26.162 / 26.161
///     sql       1 MiB     19,52 s       13,28 s    +47,0%       63.725 / 63.725
/// ```
///
/// Cuatro veces y media el reloj para comprar UN BYTE de 28.596 (0,0035%), y a 8 MiB CERO bytes
/// por un 9,2%. Antes de apagarla se intento ACOTARLA —que la sonda no pase de 1/N del bloque—
/// y el barrido de esa constante es lo que cerro la pregunta:
///
/// ```text
///     bloque    frac      reloj vs fija     bytes vs fija
///      1 MiB       8            +50,2%              -1 B
///      1 MiB      32            +19,0%              -1 B
///      1 MiB      64             +6,0%             +18 B
///      4 MiB    8..64      +6,7% a +13,6%            0 B
/// ```
///
/// No hay fraccion buena: las que conservan el byte cuestan un 19-50% y la que abarata pierde
/// 18. La respuesta no era afinar la constante, era que la pregunta no vale su precio.
///
/// ⚠️ LO QUE CUESTA, DICHO: la familia NO es estable en 4 de los 9 tipos (`bench/familia.log`),
/// asi que fijarla se equivoca a veces. El coste medido de equivocarse es **<=0,062%** y casi
/// siempre 0,000%. Se cambia bytes por reloj a razon de 0,06% contra 360%.
///
/// ⚠️ Y ESTO NO ROMPE NINGUN ARCHIVO: la familia VIAJA en el flujo desde siempre. Cambiar como
/// la ELIGE el compresor no toca al descompresor, que la lee.
fn sondear() -> bool {
    std::env::var("VC_SONDA").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
}

fn elige_familia(data: &[u8]) -> u8 {
    if let Some(f) = std::env::var("VC_FAM").ok().and_then(|v| v.parse().ok()) { return f; }
    if !sondear() { return FAMILIA_FIJA; }
    const SONDA: usize = 128 << 10;
    if data.len() < 8 << 10 { return 0; }
    let m: &[u8] = if data.len() <= 2 * SONDA {
        data
    } else {
        let ini = data.len() / 4;
        &data[ini..ini + SONDA]
    };
    let t = Tables::new();
    let mut mejor = 0u8;
    let mut mejor_tam = usize::MAX;
    for f in 0..4u8 {
        let tam = tam_sonda(&t, m, f);
        if tam < mejor_tam { mejor_tam = tam; mejor = f; }
    }
    mejor
}

/// Version del modelo con la que se escribe. SUBE cada vez que cambia algo que altere una
/// sola prediccion: modelos nuevos, contextos distintos, otra cascada, otro mezclador.
///
/// NO es la version del contenedor. Cambiar el modelo no debe invalidar un archivo: el byte
/// viaja dentro y `descomprimir` elige el codigo correcto. Cuando salga la v2, la v1 se queda
/// compilada aqui dentro — son ~50 KB de binario por generacion, que es nada comparado con
/// decirle a alguien que sus respaldos ya no se abren.
///
///   v4 (2026-07-31) — el `match` COMPRUEBA la coincidencia antes de creersela. Ver
///   `verifica_match_de_modelo`. -0,1399% ponderado, y mejora los cinco tipos.
///
/// ⚠️⚠️ Y AQUI HAY UNA DEUDA QUE HAY QUE DECIR EN VOZ ALTA: «v5» YA SIGNIFICA DOS COSAS.
///
/// Entre el 01-08 y el 02-08 cambiaron CUATRO defectos que alteran todas las predicciones —LIM,
/// DMC_UMBRAL, DMC_UMBRAL2 y ahora ISSE_INT_ON— y ninguno subio este byte. Un archivo escrito
/// ayer por la mañana y uno escrito hoy llevan los dos el 5 y NO son el mismo codec.
///
/// Y subir el byte NO lo arreglaria, que es lo que hay que entender: estas cuatro son
/// `LazyLock` con un defecto fijo, no se despachan por version. `descomprimir_con(blob, 5)`
/// construiria el Predictor con los valores de HOY aunque el archivo diga 5. El byte de version
/// solo vale para lo que de verdad se despacha por el —`tasas_de_modelo`,
/// `apagados_de_modelo`, `verifica_match_de_modelo`— y estas no estan ahi.
///
/// COMPROBADO, no razonado: comprimir con `VC_DMC_U1=64` (el defecto de ayer) y extraer con el
/// binario de hoy da «Error: indice corrupto: blake3 no coincide». O sea que el contenedor SI
/// lo caza —no hay corrupcion silenciosa— pero el mensaje dice «archivo dañado» cuando lo que
/// pasa es «lo escribio otro codec».
///
/// SE ACEPTA A SABIENDAS porque el producto no ha salido y la compatibilidad no frena nada. Lo
/// que NO se puede hacer es salir asi: antes de publicar, o estas constantes se despachan por
/// version como las tasas, o el mensaje de error tiene que distinguir «dañado» de «otro modelo».
///   v10 (2026-08-18) — MAS JUEGOS DE PESOS EN EL MEZCLADOR 0. El selector de la capa 1 pasa de
///   9 a 11 bits (512 -> 2048 juegos) con dos bits de la ventana. Medido el 04-08: −0,221% en
///   texto5, cinco de cinco muestras a favor, ciclos dentro del ruido. Ver `nsel_de_modelo`.
///   ⚠️ PARTE DE LA v8, NO DE LA v9: la v9 cerro en negativo y es rama muerta (ver
///   `salta_de_modelo`). Defecto de escritura pendiente de la re-medida con el codigo de hoy.
///
///   v9 (2026-08-17) — EL SALTO DE MATCH POR BYTE (F6·Q6). Con una racha de match confirmada de
///   `UMBRAL_SALTO` bytes o mas, el byte entero se codifica con el match mas UNA etapa SSE y se
///   salta el resto del conjunto. Ver `salta_de_modelo` y docs/f6-salto-de-match.md.
///   🚫 MEDIDA Y RECHAZADA (17-08, 1fa46de). El A/B corrio y su falsador (2) —que el coste en
///   bytes no pase del 1% en ninguna de las ocho muestras— NO se cumple: +3,340% agregado, cinco
///   de las ocho por encima del 1%, y logs +98,021%. Queda IMPLEMENTADA y LEIBLE para que un
///   archivo escrito con `VC_MODELO=9` se pueda seguir abriendo, pero es una RAMA MUERTA del
///   arbol de versiones: la v10 parte de la v8 (ver `salta_de_modelo`, que por eso es `== 9`).
///   ⚠️ Este parrafo decia «no se ha medido todavia» dos dias despues de medirlo. Un comentario
///   caducado en el sitio donde se decide que medir cuesta una tanda entera repetida.
///
/// 🔑 SON DOS NUMEROS Y HACEN DOS COSAS DISTINTAS, y de los dos sale UNA SOLA fuente para las dos
/// listas:
///   · `MODELO_MAX` — la ultima version IMPLEMENTADA. Manda en lo que este binario sabe LEER, y
///     es de donde salen `MODELOS_LEIBLES` y el reparto de `descomprimir`.
///   · `MODELO`     — la version con la que se ESCRIBE por defecto. Nunca por encima de la otra.
///
/// Separarlos es lo que permite implementar una version, poder abrir sus archivos y medirla ANTES
/// de que empiece a salir en los archivos de todo el mundo. Mientras `MODELO < MODELO_MAX` la
/// version nueva existe, se lee y se escribe a peticion (`VC_MODELO=N`, ver `modelo_a_escribir`);
/// el defecto no se mueve. Subir el defecto es una linea, y se hace con la medida delante.
pub const MODELO_MAX: u8 = 10;
/// ⭐ SUBE A 10 EL 18-08, CON LA MEDIDA DELANTE. v8 -> v10 da **-0,211%** de bytes sobre las
/// cinco muestras de `texto5r`, **las cinco a favor** (codigo -0,359% · logs -0,277% ·
/// sql -0,234% · enwik8 -0,162% · csvjson-r -0,153%), con el reloj sin tocar porque el producto
/// escalar no cambia de longitud. Cuatro de las cinco reproducen la medida del 04-08 a la
/// tercera cifra. Ver docs/f7-mas-juegos-mezclador.md y la fila `v10-mas-juegos`.
pub const MODELO: u8 = 10;

/// Escribir por defecto con una version que este binario no sabe leer seria emitir archivos que
/// ni el mismo puede abrir. Se caza al COMPILAR, no en produccion.
const _: () = assert!(MODELO >= 1 && MODELO <= MODELO_MAX);

/// Modelos que este binario sabe LEER. Escribir siempre con `MODELO`; leer, con cualquiera de
/// estos. Quitar uno de la lista es romper archivos: solo se hace tras una ventana de
/// obsolescencia y con `vcompresor migrar` disponible.
///
/// 🚨 SE DERIVA DE `MODELO_MAX`, Y ESO ES EL PUNTO ENTERO. Esto eran DOS listas a mano y la trampa
/// ya mordio dos veces: subir la version en una y no en la otra produce archivos ILEGIBLES —se
/// escribe un byte de modelo que el propio binario rechaza— y lo cazo el guardia, no una prueba.
/// Derivandola no se puede volver a desincronizar: subir `MODELO_MAX` añade su version aqui sola.
///
/// ⚠️ SE DERIVA DE `MODELO_MAX` Y NO DE `MODELO` porque lo que se LEE y lo que se ESCRIBE no son
/// la misma pregunta. Una version implementada pero aun no puesta de defecto tiene que ser
/// LEIBLE —si no, un archivo escrito con `VC_MODELO=9` no lo abriria ni el binario que lo
/// escribio— aunque el defecto siga siendo la anterior.
///
/// Que la lista sea 1..=MODELO_MAX dice «este binario sabe leer TODAS las versiones anteriores»,
/// que es la promesa del formato: cambiar el modelo no invalida un archivo, porque el byte viaja
/// dentro y `descomprimir` elige el codigo correcto.
const N_LEIBLES: usize = MODELO_MAX as usize;
const MODELOS_LEIBLES_ARR: [u8; N_LEIBLES] = {
    let mut a = [0u8; N_LEIBLES];
    let mut i = 0;
    while i < N_LEIBLES { a[i] = (i + 1) as u8; i += 1; }
    a
};
pub const MODELOS_LEIBLES: &[u8] = &MODELOS_LEIBLES_ARR;

/// VC_MODELO=N escribe con la version N en vez de con el defecto (`MODELO`). Herramienta de
/// medida: es la unica forma de comparar dos versiones de modelo sobre el mismo dato con el MISMO
/// binario. Se acota a lo que este binario sabe leer (`MODELOS_LEIBLES`, o sea 1..=`MODELO_MAX`),
/// para no escribir un archivo que ni el mismo pueda abrir despues.
///
/// 🔑 Y ES TAMBIEN COMO SE ELIGE UNA VERSION IMPLEMENTADA QUE AUN NO ES EL DEFECTO: hoy `MODELO`
/// es 8 y `MODELO_MAX` es 9, asi que `VC_MODELO=9` es el camino a la v9. No es una perilla nueva
/// ni una excepcion a «nada que cambie bytes viaja por variable de entorno»: lo que decide el
/// codec al descomprimir sigue siendo el byte que VIAJA EN EL ARCHIVO; la variable solo elige que
/// numero se escribe ahi.
/// La version de modelo con la que se ESCRIBE.
///
/// ⚠️ CON ABLACION ACTIVA SE ESCRIBE UNA VERSION QUE NADIE SABE LEER. Medido el 31-07: un
/// archivo comprimido con `VC_APAGA=35` y descomprimido normalmente devolvia un fichero del
/// TAMAÑO EXACTO con el 99,9% de los bytes mal, sin error, sin aviso y con codigo de salida 0.
/// El primer byte distinto estaba en el offset 16. Un respaldo hecho asi se restauraria como
/// basura y nadie se enteraria hasta necesitarlo.
///
/// La ablacion cambia QUE MODELOS entran, o sea que cambia todas las predicciones, y el
/// descompresor no tiene forma de saberlo: la mascara no viaja en el archivo. Escribir 254
/// convierte la corrupcion silenciosa en un ERROR LIMPIO —«modelo desconocido»— sin tocar el
/// formato y sin cambiar un solo byte del tamaño comprimido, que es lo que las medidas miden.
///
/// No se prohibe comprimir: las medidas de ablacion NECESITAN escribir el archivo para pesarlo.
/// Lo que se impide es que ese archivo se pueda confundir con uno bueno.
const MODELO_ABLACIONADO: u8 = 254;

/// PERILLA DE BANCO (F6·Q3) · `VC_CEBA=<ruta>`: los bytes con los que se CEBA el predictor
/// antes de codificar la entrada. Se modelan igual que un frame normal —el predictor aprende de
/// ellos— y NO se emite ni un bit por ellos, asi que lo que pesa el archivo es exactamente lo que
/// cuesta el frame que se mide arrancando en caliente.
///
/// `VC_CEBA_VENTANA=1` ademas deja que los modelos que miran hacia atras LEAN los bytes del
/// cebador (ventana de solo lectura sobre el frame anterior). Sin ella —el defecto— el cebador
/// deja las tablas y nada mas: ver `corta_ventana`.
///
/// 🚨 SE MARCA CON EL MODELO 254, POR LA MISMA RAZON QUE LA ABLACION (ver `MODELO_ABLACIONADO`).
/// El cebador NO viaja en el archivo, asi que el descompresor arrancaria con las tablas vacias y
/// devolveria basura del tamaño exacto, con exit 0 y sin una queja. Escribir 254 convierte esa
/// corrupcion silenciosa en un error limpio —«no se puede descomprimir»— sin tocar el formato del
/// contenedor, sin añadir una version de modelo y sin mover un solo byte del tamaño comprimido,
/// que es justo lo que la medida mide. Con la perilla apagada no se lee nada de esto.
fn ceba_ruta() -> Option<String> {
    match std::env::var("VC_CEBA") { Ok(v) if !v.is_empty() => Some(v), _ => None }
}

fn ceba_ventana() -> bool {
    std::env::var("VC_CEBA_VENTANA").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
}

fn modelo_a_escribir() -> u8 {
    if apaga_mascara() != 0 || apaga_suelto() != 0 { return MODELO_ABLACIONADO; }
    if ceba_ruta().is_some() { return MODELO_ABLACIONADO; }
    match std::env::var("VC_MODELO").ok().and_then(|v| v.parse::<u8>().ok()) {
        Some(m) if MODELOS_LEIBLES.contains(&m) => m,
        _ => MODELO,
    }
}

/// Cabecera de un volcado de predicciones (VC_VOLCAR).
pub struct Volcado {
    pub nctx: usize,
    pub nmix: usize,
    pub reg: usize,
    pub tam: u64,
    pub familia: u8,
    pub modelo: u8,
    pub perfil: u8,
    pub bits: usize,
    pub datos: Vec<u8>,
}

pub const CAB_VOLCADO: usize = 8 + 4 * 4 + 8 + 4;

/// Las perillas de la cola del predictor. `None` = deja el defecto del binario.
///
/// ⚠️ SOLO ESTAS. Son exactamente las que viven aguas abajo de `st[]`, y por eso son las unicas
/// que un volcado puede explorar. VC_ISSE_LR NO esta y no puede estar: mueve `st[IISSE]`, o sea
/// que cambia el propio volcado. Tampoco las mascaras de ablacion ni nada de los modelos. Poner
/// aqui una perilla de mas seria justo el atajo que elige configuraciones que nunca se corrieron.
#[derive(Clone, Copy, Default, Debug)]
pub struct CfgMezcla {
    pub mix_lr: Option<u32>,
    pub decay: Option<u32>,
    pub n_sse: Option<usize>,
    /// Peso inicial de TODAS las etapas de calibracion (por defecto 49152 = 3/4).
    pub sse_w0: Option<i32>,
}

/// Lee un volcado y comprueba que es de ESTE binario. Si no lo es, se niega.
pub fn lee_volcado(ruta: &str) -> Result<Volcado, String> {
    let b = std::fs::read(ruta).map_err(|e| format!("no puedo leer {ruta}: {e}"))?;
    if b.len() < CAB_VOLCADO || &b[..8] != b"VCST0002" {
        return Err(format!("{ruta} no es un volcado VCST0002 (¿de otra version?)"));
    }
    let u32en = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (nctx, nmix, reg, ncx) = (u32en(8), u32en(12), u32en(16), u32en(20));
    // ⚠️ ESTO NO ES CEREMONIA. Un volcado hecho con otro NCTX se leeria igual —los registros
    // tienen otro paso— y daria numeros perfectamente creibles y completamente falsos. Es el
    // fallo mas caro que puede tener un atajo: no revienta, miente.
    if nctx != NCTX || nmix != NMIX || reg != REG_VOLCADO || ncx != 5 {
        return Err(format!("volcado de otra arquitectura: NCTX={nctx} (aqui {NCTX}), \
NMIX={nmix} (aqui {NMIX}), registro={reg} (aqui {REG_VOLCADO}). Vuelve a volcar."));
    }
    let tam = u64::from_le_bytes(b[24..32].try_into().unwrap());
    let (familia, modelo, perfil) = (b[32], b[33], b[34]);
    let cuerpo = b.len() - CAB_VOLCADO;
    if cuerpo % reg != 0 {
        return Err(format!("el volcado esta cortado: {cuerpo} B no son multiplo de {reg}"));
    }
    let bits = cuerpo / reg;
    if bits as u64 != tam * 8 {
        return Err(format!("el volcado tiene {bits} bits y el dato eran {} — truncado",
                           tam * 8));
    }
    Ok(Volcado { nctx, nmix, reg, tam, familia, modelo, perfil, bits, datos: b })
}

/// EL REPLAY. Ejecuta la cola del predictor —capa 1, capa 2 y calibracion— sobre predicciones ya
/// volcadas, con la configuracion que digan las variables de entorno de este proceso.
///
/// Devuelve `(bytes del codificador, bits de entropia)`. Los bytes son LOS MISMOS que saldrian de
/// comprimir de verdad con esa configuracion, no una estimacion: el codificador es el mismo
/// `Encoder`, y la mezcla es el mismo `mezcla()`/`actualiza_mezcla()` que usa `comprimir`. Que lo
/// sean se comprueba de todas formas —`bench/replay-control.sh`—, porque una propiedad que se
/// argumenta y no se mide es una propiedad que todavia no se tiene.
///
/// La configuracion se pasa POR PARAMETRO, no por variable de entorno. No es un capricho: el
/// entorno es global al proceso, asi que un replay que lo leyera no se podria paralelizar dentro
/// de un mismo binario —que es justo lo que lo hace barato— sin que dos hilos se pisaran la
/// configuracion y devolvieran numeros cruzados.
pub fn replay(v: &Volcado, cfg: &CfgMezcla) -> (usize, f64) {
    replay_muchas(v, std::slice::from_ref(cfg)).pop().unwrap()
}

/// Varias configuraciones sobre el MISMO volcado, reutilizando un solo `Predictor`.
///
/// Es la version que hay que usar para barrer: crear el predictor cuesta mas que ejecutar la cola,
/// asi que una tanda de N configuraciones con un solo predictor no es una micro-optimizacion, es
/// la diferencia entre que el replay compense y que no.
pub fn replay_muchas(v: &Volcado, cfgs: &[CfgMezcla]) -> Vec<(usize, f64)> {
    let t = Tables::new();
    // El tamaño importa (fija `tmask` y la poda), y familia/modelo/perfil tambien: van en el
    // volcado justamente para no tener que adivinarlos aqui.
    let mut pred = Predictor::new(&t, v.tam as usize, v.familia, v.modelo, v.perfil);
    let w0: Vec<Vec<i32>> = pred.w.clone();   // el arranque exacto de los pesos, para reiniciar
    let d = &v.datos[CAB_VOLCADO..];
    let mut salida = Vec::with_capacity(cfgs.len());
    for cfg in cfgs {
        pred.reinicia_cola(cfg, &w0, &t);
        let mut enc = Encoder::new();
        let mut ent = 0.0f64;
        for i in 0..v.bits {
            let r = &d[i * v.reg..(i + 1) * v.reg];
            let bit = r[0] as i32;
            pred.bpos = r[1] as u32;
            pred.sel2 = r[2] as usize;
            for k in 0..NMIX {
                pred.sel[k] =
                    u16::from_le_bytes(r[4 + k * 2..6 + k * 2].try_into().unwrap()) as usize;
            }
            for k in 0..5 {
                pred.cx_v[k] = u32::from_le_bytes(r[12 + k * 4..16 + k * 4].try_into().unwrap());
            }
            // ⚠️ SIN `try_into().unwrap()` AQUI. Este bucle corre NCTX veces por bit —83 millones
            // de veces sobre 64 KiB— y el `try_into` de un slice hace comprobacion de longitud y
            // deja un camino de panico que impide vectorizar. Con `chunks_exact` el limite se
            // comprueba una vez para todo el bloque.
            let sb = &r[32..32 + 2 * NCTX];
            for (j, c) in sb.chunks_exact(2).enumerate() {
                pred.st[j] = i16::from_le_bytes([c[0], c[1]]) as i32;
            }
            let p = pred.mezcla();
            let pr = if bit == 1 { p as f64 / 4096.0 } else { 1.0 - p as f64 / 4096.0 };
            ent += -pr.max(1e-12).log2();
            enc.encode(bit, p);
            pred.actualiza_mezcla(bit);
        }
        salida.push((enc.flush().len(), ent));
    }
    salida
}

pub fn comprimir(data: &[u8]) -> Vec<u8> {
    let familia = elige_familia(data);
    let modelo = modelo_a_escribir();
    let perfil = perfil_a_escribir();
    let t = Tables::new();
    // ⚠️ EL TAMAÑO ES EL DEL FRAME QUE SE MIDE, TAMBIEN CON CEBADOR, y no la suma de los dos.
    // `tam` fija la geometria de las tablas (`tmask`, `imask`, la poda por modelo). En el
    // paralelismo que esto va a informar todos los frames son del MISMO tamaño, asi que la
    // instantanea que se copiaria tiene exactamente esta geometria. Sumarle el cebador aqui
    // mediria unas tablas que el producto no tendria.
    let mut pred = Predictor::new(&t, data.len(), familia, modelo, perfil);
    pred.set_byte_contexts();
    if let Some(ruta) = ceba_ruta() {
        let ceba = std::fs::read(&ruta)
            .unwrap_or_else(|e| panic!("VC_CEBA={ruta}: no puedo leerlo ({e}). Una medida de \
                                        banco que se ceba con nada seria la linea de partida \
                                        disfrazada de resultado."));
        // El cebador se MODELA, no se EMITE: mismo bucle que abajo sin el `enc.encode`.
        for &byte in &ceba {
            for k in (0..8).rev() {
                let bit = ((byte >> k) & 1) as i32;
                let _ = pred.p();
                pred.update(bit);
            }
        }
        if !ceba_ventana() { pred.corta_ventana(); }
        eprintln!("[ceba] {} B de {ruta} · ventana {}", ceba.len(),
                  if ceba_ventana() { "ABIERTA (tablas + historia)" } else { "CERRADA (solo tablas)" });
    }
    let mut enc = Encoder::new();
    for &byte in data {
        for k in (0..8).rev() {
            let bit = ((byte >> k) & 1) as i32;
            let p = pred.p();
            enc.encode(bit, p);
            pred.update(bit);
        }
    }
    if pred.censo { pred.censo(data.len()); }
    if pred.corr { pred.correlacion(data.len()); }
    if let Ok(f) = std::env::var("VC_DUMP_W") {
        // Los pesos que el mezclador ha aprendido sobre ESTE dato. Entrenar aqui y medir en
        // otro sitio es la unica forma de que el numero signifique algo.
        let mut b: Vec<u8> = Vec::new();
        for k in 0..NMIX {
            for j in 0..pred.nsel[k] * NCTXP { b.extend_from_slice(&pred.w[k][j].to_le_bytes()); }
        }
        let _ = std::fs::write(&f, &b);
    }
    let mut out = (data.len() as u64).to_le_bytes().to_vec();
    out.push(modelo);
    // 🔑 EL PERFIL VIAJA EN LOS CUATRO BITS DE ARRIBA DEL BYTE DE LA FAMILIA, y no hace falta
    // byte nuevo: la familia son 0..3, o sea dos bits, y los otros seis estaban a cero desde
    // siempre. Un archivo viejo trae 0..3 ahi, que se lee como perfil 0 —el defecto— que es
    // exactamente lo que era. Compatible hacia atras por construccion, no por suerte.
    // ⚠️ La familia se enmascara a 4 bits: `VC_FAM` acepta cualquier u8 y un valor de diagnostico
    // por encima de 15 se comeria los bits del perfil. Solo 0..3 significan algo (`familia_apagados`
    // mira dos bits), asi que enmascarar no puede perder informacion util — pero sin la mascara,
    // un `VC_FAM=16` escribiria perfil 1 sin que nadie lo pidiera.
    out.push((familia & 0x0F) | (perfil << 4));
    out.extend(enc.flush());
    out
}

/// Inverso exacto de `comprimir`.
/// Reparte por version de modelo. Un archivo escrito con un modelo que este binario no
/// conoce da un error CLARO —"actualiza"— en vez de devolver bytes equivocados en silencio,
/// que es infinitamente peor en un archivador.
pub fn descomprimir(blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < 10 { return Err("flujo CM truncado".into()); }
    let modelo = blob[8];
    if !MODELOS_LEIBLES.contains(&modelo) {
        // El 254 no es «una version mas nueva»: es la marca de ABLACION. Decirle a alguien que
        // actualice para abrirlo seria mandarlo a esperar una version que no va a existir nunca.
        if modelo == MODELO_ABLACIONADO {
            return Err("este archivo se comprimio con una PERILLA DE BANCO (VC_APAGA / \
                        VC_APAGAMASK, o VC_CEBA), que cambia el estado con el que arranca el \
                        modelo y por tanto todas las predicciones. Ni la mascara ni el cebador \
                        viajan en el archivo, asi que no hay forma de descomprimirlo — ni ahora \
                        ni con una version futura. Es un artefacto de medida, no un respaldo. \
                        Vuelve a comprimir sin esas variables.".into());
        }
        return Err(format!(
            "este archivo usa el modelo v{modelo} y esta version conoce hasta v{}: \
             actualiza vcompresor para abrirlo", MODELO_MAX));
    }
    // ⚠️ ESTA LISTA Y `MODELOS_LEIBLES` ERAN DOS LISTAS, Y SE DESINCRONIZARON (03-08). Al meter
    // la v6 se anadio alli y no aqui: `compress` escribia v6 tan tranquilo y `list` y `extract`
    // respondian «listado como leible pero sin implementacion». El guardia hizo exactamente lo
    // que tenia que hacer —avisar en vez de devolver basura— pero el fallo era que hubiera dos
    // sitios, y lo que quedo escrito aqui fue: «si esto vuelve a pasar, la solucion no es
    // acordarse: es derivar una de la otra».
    //
    // ✅ VOLVIO A PASAR CON LA v9 (17-08, F6·Q6) Y SE APLICO ESA SOLUCION, no un parche. Las dos
    // listas salen ahora de `MODELO_MAX`: `MODELOS_LEIBLES` se construye como 1..=MODELO_MAX y
    // este reparto pregunta por el mismo rango en vez de enumerar. Subir `MODELO_MAX` es lo unico
    // que hay que hacer; no queda ningun sitio donde olvidarse. El guardia se queda —cuesta nada y
    // es lo que caza el dia que alguien vuelva a enumerar a mano.
    //
    // ⚠️ EL RANGO ES EL DE LECTURA (`MODELO_MAX`), NO EL DE ESCRITURA (`MODELO`). Preguntar por
    // `MODELO` aqui volveria a partir las dos listas por el otro lado: `VC_MODELO=9` escribiria un
    // v9 legitimo que este mismo reparto rechazaria.
    match modelo {
        m if m >= 1 && m <= MODELO_MAX => Ok(descomprimir_con(blob, m)),
        _ => Err(format!("modelo v{modelo} listado como leible pero sin implementacion")),
    }
}

/// El bucle es el mismo para todas las versiones; lo que cambia es el juego de modelos, y eso
/// entra por `modelo`. Duplicar la funcion por version seria copiar cien lineas para cambiar
/// un argumento, y la copia que no se toca es la que se queda con el fallo.
fn descomprimir_con(blob: &[u8], modelo: u8) -> Vec<u8> {
    let n = u64::from_le_bytes(blob[0..8].try_into().unwrap()) as usize;
    // la familia y el perfil se LEEN, no se deducen ni se preguntan al entorno de quien extrae
    let familia = blob[9] & 0x0F;
    let perfil = blob[9] >> 4;
    let t = Tables::new();
    let mut pred = Predictor::new(&t, n, familia, modelo, perfil);
    pred.set_byte_contexts();
    let mut dec = Decoder::new(&blob[10..]);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut byte = 0u8;
        for _ in 0..8 {
            let p = pred.p();
            let bit = dec.decode(p);
            pred.update(bit);
            byte = (byte << 1) | bit as u8;
        }
        out.push(byte);
    }
    out
}

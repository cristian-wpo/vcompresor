import Foundation

/// El puente con el binario. La app NO reimplementa nada: lanza `vcompresor` y lee lo que dice.
///
/// Es a proposito. El motor esta probado con fuzz de miles de ficheros y round-trip byte a byte;
/// duplicar aunque sea la logica de elegir ajustes en Swift seria crear una segunda verdad que
/// se desincroniza en la primera version.
final class Motor: ObservableObject {

    enum Estado: Equatable {
        case reposo
        /// `restante` = segundos que se calcula que faltan, o nil cuando todavía no se puede decir.
        case trabajando(pct: Double, detalle: String, restante: TimeInterval? = nil)
        case hecho(entrada: Int64, salida: Int64, segundos: Double, ruta: URL)
        case error(String)
    }

    @Published var estado: Estado = .reposo
    private var proceso: Process?

    /// Donde esta el binario. Se busca dentro del .app primero -asi la app es autocontenida- y
    /// solo si no esta se recurre a las rutas del sistema.
    static func binario() -> URL? {
        if let dentro = Bundle.main.url(forResource: "vcompresor", withExtension: nil),
           FileManager.default.isExecutableFile(atPath: dentro.path) { return dentro }
        for p in ["/usr/local/bin/vcompresor", "/opt/homebrew/bin/vcompresor",
                  NSHomeDirectory() + "/bin/vcompresor"] {
            if FileManager.default.isExecutableFile(atPath: p) { return URL(fileURLWithPath: p) }
        }
        return nil
    }

    func cancelar() {
        proceso?.terminate()
        proceso = nil
        estado = .reposo
    }

    func comprimir(origenes: [URL], destino: URL, maximo: Bool, contrasena: String?) {
        guard let bin = Motor.binario() else {
            estado = .error("No encuentro el motor «vcompresor». Debería estar dentro de la app.")
            return
        }
        guard !origenes.isEmpty else { return }
        let tam = Motor.tamano(de: origenes)
        estado = .trabajando(pct: 0, detalle: "preparando…")

        // TODAS las entradas. Antes iba una sola y la interfaz, para poder guardar una selección,
        // mandaba la carpeta que la contenía: cuatro archivos de 170 MB se convertían en 11 GB.
        var args = ["compress"] + origenes.map(\.path) + ["-o", destino.path]
        args += maximo ? ["--max"] : ["--level", "19"]
        // Sin esto el motor NO pinta el avance: lo tiene detrás de un `isatty` para no llenar de
        // barras repintadas los registros de un cron, y una tubería no es un terminal. La app
        // leía unas líneas que nunca salían y la barra se quedaba a cero toda la ejecución.
        args += ["--progress"]

        var ficheroPass: URL?
        if let c = contrasena, !c.isEmpty {
            // La contrasena va por FICHERO temporal con permisos 600, nunca como argumento: un
            // argumento sale en `ps` para cualquier usuario de la maquina.
            let f = FileManager.default.temporaryDirectory
                .appendingPathComponent("vc-\(UUID().uuidString)")
            try? c.write(to: f, atomically: true, encoding: .utf8)
            try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: f.path)
            ficheroPass = f
            args += ["--encrypt", "--password-file", f.path]
        }

        let p = Process()
        p.executableURL = bin
        p.arguments = args
        let err = Pipe(), out = Pipe()
        p.standardError = err
        p.standardOutput = out
        proceso = p

        let arranque = Date()
        // ❌ NEGATIVO MEDIDO (03-08): ESTIMAR EL RITMO. Lo que habia aqui y por que se fue.
        //
        // El razonamiento era bueno y la conclusion falsa. Decia: «la velocidad no es constante
        // -el carril de JXL va a ~6 MB/s y zstd a ~170-, asi que no se puede usar la media de
        // toda la corrida; hay que perseguir el ritmo RECIENTE con una media exponencial». Suena
        // a manual y es lo que hace todo el mundo.
        //
        // Medido contra otros siete estimadores sobre las mismas dos trazas, **fue el peor de los
        // ocho**: 85% de error mediano en el peor arbol, contra el 50% del que ha quedado. El
        // fallo del argumento es que si el ritmo se mueve un orden de magnitud, el ritmo RECIENTE
        // tampoco vale: es el del trozo que acaba de pasar, no el de lo que viene. Perseguir una
        // senal que salta no la acerca, la copia con retraso.
        //
        // Tambien se fue con el la deduccion del total (`totalMiB = max(totalMiB, miB*100/pct)`)
        // y el candado de dos estimaciones seguidas dentro del 35%. La deduccion funcionaba bien
        // -medida: +0,5% de error al final- pero ya no hace falta; y el candado tapaba los saltos
        // de un estimador que ya no salta.

        // ── EL TIEMPO QUE FALTA: NO SE ESTIMA EL RITMO ─────────────────────────────────────
        //
        // 🔑 MEDIDO EL 03-08 SOBRE DOS ARBOLES, con `bench/banco-eta.py` -que pasa UNA traza
        // capturada por ocho estimadores, para que todos vean exactamente los mismos datos-:
        //
        //     estimador                  mixto A   mixto B   peor
        //     por el porcentaje              48%       50%    50%   <- este
        //     ventana de 60 s                50%       43%    50%
        //     ritmo medio                    46%       50%    50%
        //     ventana de 30 s                64%       52%    64%
        //     media exponencial (el viejo)   85%       48%    85%
        //     pesimista (el peor ritmo)      75%       67%    75%
        //
        // El que habia -media exponencial del ritmo reciente- es EL PEOR de los ocho. Y los tres
        // primeros empatan, asi que gana el mas simple: `transcurrido * (100 - pct) / pct`.
        //
        // ⚠️ LO IMPORTANTE ES QUE NO HAY RITMO. No se mide, no se suaviza, no se acota. El ritmo
        // de este compresor va de 4,6 a 253 MiB/s DENTRO DE LA MISMA CORRIDA -enrutar un JPEG es
        // barato y comprimir es caro-, asi que cualquier medida del ritmo reciente es la del
        // trozo que acaba de pasar y no la de lo que viene. Usando el tiempo ya gastado, el
        // ritmo queda promediado solo y sin tener que elegir ventana.
        //
        // 🚨 Y ESTO SOLO FUNCIONA PORQUE EL PORCENTAJE YA ES HONESTO. Antes el denominador del
        // motor se descubria durante la corrida -la barra llegaba a caer del 93% al 67%- y una
        // regla de tres sobre un porcentaje que retrocede no vale nada. Se arreglo prediciendo el
        // plano POR TIPO DE FICHERO (`plano_por_byte_256` en vc-core): la prevision acierta al
        // +1,4% sobre 629 MiB. Los dos cambios son uno solo; ninguno sirve sin el otro.
        //
        // Ya no hace falta ni deducir el total ni el candado de dos estimaciones seguidas: este
        // estimador no da saltos, porque `transcurrido` solo crece y `pct` ya no retrocede.
        err.fileHandleForReading.readabilityHandler = { [weak self] h in
            guard let texto = String(data: h.availableData, encoding: .utf8), !texto.isEmpty else { return }
            // El motor pinta "  N/M ficheros (P%), X MiB leidos". Se saca todo de ahi
            // en vez de inventarse una estimacion propia.
            for linea in texto.split(whereSeparator: \.isNewline) {
                let s = String(linea)
                if let r = s.range(of: #"(\d+)/(\d+) ficheros \(([\d.]+)%\), (\d+) MiB"#,
                                   options: .regularExpression) {
                    // El porcentaje llega con decimal desde el 03-08, asi que se saca aparte:
                    // partir por "lo que no sea numero" se lo comeria en dos trozos.
                    let num = s[r]
                    guard let mp = num.range(of: #"\(([\d.]+)%\)"#, options: .regularExpression)
                    else { continue }
                    let pct = Double(num[mp].dropFirst().dropLast(2)) ?? 0
                    let enteros = num.split(whereSeparator: { !$0.isNumber })
                        .compactMap { Int($0) }
                    guard enteros.count >= 2 else { continue }

                    var restante: TimeInterval? = nil
                    let transcurrido = Date().timeIntervalSince(arranque)
                    // Los dos umbrales que se quedan: por debajo del 1% o de los 2 s cualquier
                    // numero seria inventado, y un "faltan 4 horas" que a los tres segundos pasa
                    // a "1 min" es peor que no decir nada.
                    if pct >= 1, transcurrido >= 2 {
                        restante = transcurrido * (100 - pct) / pct
                    }
                    // Se rehace la frase en vez de repetir la del motor: la del motor va sin
                    // acentos -sale a un terminal- y aqui se lee en una ventana.
                    let detalle = enteros[1] == 1 ? "1 archivo"
                        : "\(enteros[0]) de \(enteros[1]) archivos"
                    DispatchQueue.main.async {
                        self?.estado = .trabajando(pct: pct / 100, detalle: detalle,
                                                   restante: restante)
                    }
                } else if s.contains("deriving key") {
                    DispatchQueue.main.async {
                        self?.estado = .trabajando(pct: 0, detalle: "preparando el cifrado…")
                    }
                }
            }
        }

        p.terminationHandler = { [weak self] proc in
            err.fileHandleForReading.readabilityHandler = nil
            if let f = ficheroPass { try? FileManager.default.removeItem(at: f) }
            let salida = (try? Data(contentsOf: destino).count) ?? 0
            let textoErr = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            DispatchQueue.main.async {
                guard proc.terminationStatus == 0 else {
                    let limpio = textoErr.split(whereSeparator: \.isNewline)
                        .last(where: { $0.contains("Error") || $0.contains("error") })
                        .map(String.init) ?? "No se pudo guardar."
                    self?.estado = .error(limpio.replacingOccurrences(of: "Error: ", with: ""))
                    return
                }
                self?.estado = .hecho(entrada: tam, salida: Int64(salida),
                                      segundos: Date().timeIntervalSince(arranque), ruta: destino)
            }
        }
        do { try p.run() } catch { estado = .error("No pude arrancar el motor: \(error.localizedDescription)") }
    }

    /// Cuanto ocupa TODO lo que se va a guardar.
    static func tamano(de urls: [URL]) -> Int64 {
        urls.reduce(0) { $0 + tamano(de: $1) }
    }

    /// Cuanto ocupa lo que se va a guardar. Recorrer puede tardar en arboles grandes, asi que va
    /// fuera del hilo de la interfaz cuando haga falta; para lo normal es instantaneo.
    static func tamano(de url: URL) -> Int64 {
        let fm = FileManager.default
        var total: Int64 = 0
        if let e = fm.enumerator(at: url, includingPropertiesForKeys: [.fileSizeKey, .isRegularFileKey]) {
            for caso in e {
                guard let u = caso as? URL,
                      let v = try? u.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]),
                      v.isRegularFile == true else { continue }
                total += Int64(v.fileSize ?? 0)
            }
        }
        if total == 0, let v = try? url.resourceValues(forKeys: [.fileSizeKey]) {
            total = Int64(v.fileSize ?? 0)
        }
        return total
    }
}

/// Tamaños como los escribe el Finder, no como los escribe un programador.
func legible(_ b: Int64) -> String {
    let f = ByteCountFormatter()
    f.countStyle = .file
    return f.string(fromByteCount: b)
}

// MARK: - Abrir un archivo y recuperar de dentro

/// Lo que puede salir mal al leer un archivo.
///
/// `pideContrasena` va aparte del resto a proposito: la interfaz tiene que poder PEDIRLA en vez
/// de enseñar un error, y distinguirla mirando el texto del mensaje seria frágil.
enum FalloAlLeer: Error {
    case pideContrasena
    case otro(String)
}

/// Una entrada del archivo, tal y como la lista el motor.
struct Entrada: Identifiable, Hashable {
    let id = UUID()
    let ruta: String
    let bytes: Int64
    let tipo: Character   // '-' fichero, 'd' directorio, 'l' enlace, 'h' enlace duro

    var nombre: String { ruta.split(separator: "/").last.map(String.init) ?? ruta }
    var carpeta: String {
        let p = ruta.split(separator: "/").dropLast()
        return p.isEmpty ? "" : p.joined(separator: "/")
    }
    var icono: String {
        switch tipo {
        case "d": return "folder"
        case "l", "h": return "link"
        default: return "doc"
        }
    }
}

extension Motor {

    /// Lee el contenido del archivo. Devuelve nil si hace falta contrasena y no se ha dado —
    /// asi la interfaz puede pedirla en vez de enseñar un error tecnico.
    static func listar(archivo: URL, contrasena: String?) -> Result<[Entrada], FalloAlLeer> {
        guard let bin = binario() else { return .failure(.otro("No encuentro el motor.")) }
        var args = ["list", archivo.path]
        var tmp: URL?
        if let c = contrasena, !c.isEmpty {
            let f = FileManager.default.temporaryDirectory.appendingPathComponent("vc-\(UUID().uuidString)")
            try? c.write(to: f, atomically: true, encoding: .utf8)
            try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: f.path)
            tmp = f
            args += ["--password-file", f.path]
        }
        let p = Process()
        p.executableURL = bin
        p.arguments = args
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        do { try p.run() } catch { return .failure(.otro("No pude arrancar el motor.")) }
        let datos = out.fileHandleForReading.readDataToEndOfFile()
        let errores = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        p.waitUntilExit()
        if let f = tmp { try? FileManager.default.removeItem(at: f) }

        if p.terminationStatus != 0 {
            if errores.contains("contraseña") || errores.contains("password") {
                return .failure(.pideContrasena)
            }
            let l = errores.split(whereSeparator: \.isNewline).first(where: { $0.contains("Error") })
                .map { String($0).replacingOccurrences(of: "Error: ", with: "") }
            return .failure(.otro(l ?? "No se puede leer este archivo."))
        }

        // Cada linea es "t0644         2395 ruta/del/fichero". El tamaño puede llevar espacios
        // delante, y la ruta puede llevarlos dentro: se parte solo por los dos primeros campos.
        var v: [Entrada] = []
        for linea in (String(data: datos, encoding: .utf8) ?? "").split(whereSeparator: \.isNewline) {
            let s = String(linea)
            guard let tipo = s.first else { continue }
            let resto = s.dropFirst(5).drop(while: { $0 == " " })   // saltar el modo
            guard let corte = resto.firstIndex(of: " ") else { continue }
            let bytes = Int64(resto[..<corte]) ?? 0
            let ruta = String(resto[corte...].drop(while: { $0 == " " }))
            if ruta.isEmpty { continue }
            v.append(Entrada(ruta: ruta, bytes: bytes, tipo: tipo))
        }
        return .success(v)
    }

    /// Saca VARIAS cosas. Lista vacia = todo.
    ///
    /// El motor acepta UN patron por ejecucion, asi que se le llama una vez por cada cosa, en
    /// serie. La alternativa —juntarlas en un comodin— sacaria ademas ficheros que nadie pidio,
    /// que en una recuperacion es peor que tardar un poco mas.
    ///
    /// Esto existe porque la version anterior recuperaba SOLO el primero de la seleccion
    /// mientras el boton decia "Recuperar 3". Un boton que miente sobre lo que hace es un fallo,
    /// no una funcion a medias.
    func recuperarVarios(archivo: URL, destino: URL, rutas: [String], contrasena: String?) {
        guard !rutas.isEmpty else {
            recuperar(archivo: archivo, destino: destino, patron: nil, contrasena: contrasena)
            return
        }
        guard let bin = Motor.binario() else {
            estado = .error("No encuentro el motor.")
            return
        }
        var tmp: URL?
        var extra: [String] = []
        if let c = contrasena, !c.isEmpty {
            let f = FileManager.default.temporaryDirectory.appendingPathComponent("vc-\(UUID().uuidString)")
            try? c.write(to: f, atomically: true, encoding: .utf8)
            try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: f.path)
            tmp = f
            extra = ["--password-file", f.path]
        }
        estado = .trabajando(pct: 0, detalle: "recuperando 1 de \(rutas.count)…")
        let arranque = Date()

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var fallo: String?
            for (i, ruta) in rutas.enumerated() {
                let p = Process()
                p.executableURL = bin
                p.arguments = ["extract", archivo.path, "-o", destino.path, "--file", ruta] + extra
                let err = Pipe()
                p.standardError = err
                p.standardOutput = Pipe()
                do { try p.run() } catch { fallo = "No pude arrancar el motor."; break }
                let texto = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
                p.waitUntilExit()
                if p.terminationStatus != 0 {
                    // Se para en el PRIMER fallo y se dice cual. Seguir y recuperar los demas
                    // dejaria al usuario creyendo que lo tiene todo.
                    fallo = texto.split(whereSeparator: \.isNewline).first(where: { $0.contains("Error") })
                        .map { String($0).replacingOccurrences(of: "Error: ", with: "") }
                        ?? "No se pudo recuperar «\(ruta)»."
                    break
                }
                let hechos = i + 1
                DispatchQueue.main.async {
                    self?.estado = .trabajando(pct: Double(hechos) / Double(rutas.count),
                                               detalle: "recuperando \(min(hechos + 1, rutas.count)) de \(rutas.count)…")
                }
            }
            if let f = tmp { try? FileManager.default.removeItem(at: f) }
            DispatchQueue.main.async {
                if let f = fallo { self?.estado = .error(f) }
                else {
                    let n = Motor.tamano(de: destino)
                    self?.estado = .hecho(entrada: n, salida: n,
                                          segundos: Date().timeIntervalSince(arranque), ruta: destino)
                }
            }
        }
    }

    /// Saca UNA cosa a un sitio temporal para MIRARLA, y dice dónde ha quedado.
    ///
    /// No toca `estado` a propósito: quien abre un fichero para verlo espera seguir viendo la
    /// lista, no que la pantalla entera se convierta en una barra de progreso y luego en un
    /// «Recuperado» del que hay que volver.
    ///
    /// LO QUE SALE VA DE SOLO LECTURA. Es una copia sacada del archivo, no el fichero: si el
    /// programa que lo abre deja guardar encima, el cambio se pierde en cuanto se limpie el
    /// temporal, y perder trabajo en silencio es peor que un «no se puede guardar» claro.
    static func sacarParaVer(archivo: URL, ruta: String, bytes: Int64, contrasena: String?,
                             hecho: @escaping (Result<URL, FalloAlLeer>) -> Void) {
        guard let bin = binario() else { return hecho(.failure(.otro("No encuentro el motor."))) }

        // Que quepa. Sacar un vídeo de 8 GB al disco de arranque para verlo puede llenarlo, y
        // un disco lleno se nota mucho más tarde y en sitios que no tienen nada que ver.
        let tmpBase = FileManager.default.temporaryDirectory
            .appendingPathComponent("vcompresor-ver", isDirectory: true)
        if let libre = try? tmpBase.deletingLastPathComponent()
            .resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
            .volumeAvailableCapacityForImportantUsage, Int64(libre) < bytes + (100 << 20) {
            return hecho(.failure(.otro("Para abrirlo hay que sacarlo del archivo y no cabe: ocupa \(legible(bytes)) y quedan \(legible(Int64(libre)))."))) 
        }

        DispatchQueue.global(qos: .userInitiated).async {
            let destino = tmpBase.appendingPathComponent(UUID().uuidString, isDirectory: true)
            try? FileManager.default.createDirectory(at: destino, withIntermediateDirectories: true)

            var args = ["extract", archivo.path, "-o", destino.path, "--file", ruta]
            var tmpPass: URL?
            if let c = contrasena, !c.isEmpty {
                let f = FileManager.default.temporaryDirectory.appendingPathComponent("vc-\(UUID().uuidString)")
                try? c.write(to: f, atomically: true, encoding: .utf8)
                try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: f.path)
                tmpPass = f
                args += ["--password-file", f.path]
            }
            let p = Process()
            p.executableURL = bin
            p.arguments = args
            let err = Pipe()
            p.standardError = err
            p.standardOutput = Pipe()
            do { try p.run() } catch { return DispatchQueue.main.async { hecho(.failure(.otro("No pude arrancar el motor."))) } }
            let texto = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            p.waitUntilExit()
            if let f = tmpPass { try? FileManager.default.removeItem(at: f) }

            // Dentro del archivo la ruta es «sub/f.txt», así que el fichero sale ahí dentro.
            let salida = destino.appendingPathComponent(ruta)
            DispatchQueue.main.async {
                guard FileManager.default.fileExists(atPath: salida.path) else {
                    let l = texto.split(whereSeparator: \.isNewline).first(where: { $0.contains("Error") })
                        .map { String($0).replacingOccurrences(of: "Error: ", with: "") }
                    return hecho(.failure(.otro(l ?? "No se pudo abrir «\(ruta)».")))
                }
                try? FileManager.default.setAttributes([.posixPermissions: 0o444], ofItemAtPath: salida.path)
                hecho(.success(salida))
            }
        }
    }

    /// Tira los temporales de mirar ficheros. Se llama al arrancar: dejarlos ahí es ir llenando
    /// el disco con copias de cosas que ya se miraron.
    static func limpiaTemporales() {
        let d = FileManager.default.temporaryDirectory
            .appendingPathComponent("vcompresor-ver", isDirectory: true)
        try? FileManager.default.removeItem(at: d)
    }

    /// Saca de dentro solo lo pedido. `patron` nil = todo.
    func recuperar(archivo: URL, destino: URL, patron: String?, contrasena: String?) {
        guard let bin = Motor.binario() else {
            estado = .error("No encuentro el motor.")
            return
        }
        estado = .trabajando(pct: 0, detalle: patron == nil ? "recuperando todo…" : "recuperando…")
        var args = ["extract", archivo.path, "-o", destino.path]
        if let pat = patron { args += ["--file", pat] }
        var tmp: URL?
        if let c = contrasena, !c.isEmpty {
            let f = FileManager.default.temporaryDirectory.appendingPathComponent("vc-\(UUID().uuidString)")
            try? c.write(to: f, atomically: true, encoding: .utf8)
            try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: f.path)
            tmp = f
            args += ["--password-file", f.path]
        }
        let p = Process()
        p.executableURL = bin
        p.arguments = args
        let err = Pipe()
        p.standardError = err
        p.standardOutput = Pipe()
        let arranque = Date()
        p.terminationHandler = { proc in
            let texto = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            if let f = tmp { try? FileManager.default.removeItem(at: f) }
            DispatchQueue.main.async { [weak self] in
                guard proc.terminationStatus == 0 else {
                    let l = texto.split(whereSeparator: \.isNewline).first(where: { $0.contains("Error") })
                        .map { String($0).replacingOccurrences(of: "Error: ", with: "") }
                    self?.estado = .error(l ?? "No se pudo recuperar.")
                    return
                }
                let n = Motor.tamano(de: destino)
                self?.estado = .hecho(entrada: n, salida: n,
                                      segundos: Date().timeIntervalSince(arranque), ruta: destino)
            }
        }
        do { try p.run() } catch { estado = .error("No pude arrancar el motor.") }
    }
}

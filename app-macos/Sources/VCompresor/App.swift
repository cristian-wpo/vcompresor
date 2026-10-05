import SwiftUI
import UniformTypeIdentifiers

/// Qué .vc hay abierto, si hay alguno. Lo llena tanto arrastrar un archivo a la ventana como
/// abrirlo desde el Finder, para que las dos formas acaben en el mismo sitio.
final class Abierto: ObservableObject {
    static let compartido = Abierto()
    @Published var archivo: URL?
    /// Lo que el Finder ha mandado a comprimir por el menú contextual. Es una LISTA: si has
    /// seleccionado cuatro cosas, son esas cuatro y no la carpeta que las contiene.
    @Published var aComprimir: [URL] = []
}

/// Recoge los "abre esto" del sistema. Sin esto, hacer doble clic en un .vc abriría la app
/// mostrando la pantalla de guardar, que es exactamente lo contrario de lo que se pedía.
final class Delegado: NSObject, NSApplicationDelegate {
    private let servicios = Servicios()

    func applicationDidFinishLaunching(_ n: Notification) {
        // Sin esto el sistema enseña la entrada en el menú y al pulsarla no pasa nada: hay que
        // decirle QUIEN atiende los Services declarados en el Info.plist.
        NSApp.servicesProvider = servicios
        NSUpdateDynamicServices()
        // Las copias que se sacaron para mirar ficheros de dentro de un archivo. Se tiran al
        // arrancar: dejarlas ahí es ir llenando el disco con cosas que ya se miraron.
        Motor.limpiaTemporales()
    }

    func application(_ app: NSApplication, open urls: [URL]) {
        if let u = urls.first(where: { $0.pathExtension.lowercased() == "vc" }) {
            DispatchQueue.main.async { Abierto.compartido.archivo = u }
        }
    }
}

@main
struct VCompresorApp: App {
    @NSApplicationDelegateAdaptor(Delegado.self) var delegado
    @StateObject private var abierto = Abierto.compartido

    var body: some Scene {
        Window("vcompresor", id: "principal") {
            Group {
                if let a = abierto.archivo {
                    Explorador(archivo: a)
                        .toolbar {
                            ToolbarItem(placement: .navigation) {
                                Button { abierto.archivo = nil } label: {
                                    Label("Guardar algo", systemImage: "plus")
                                }
                            }
                        }
                } else {
                    Pantalla(inicial: abierto.aComprimir)
                        .id(abierto.aComprimir.map(\.path).joined(separator: "|"))
                        .frame(minWidth: 460, minHeight: 380)
                }
            }
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Abrir archivo…") { abrirDialogo() }
                    .keyboardShortcut("o")
            }
        }
        .defaultSize(width: 560, height: 460)
    }

    private func abrirDialogo() {
        let p = NSOpenPanel()
        p.allowedContentTypes = [UTType(filenameExtension: "vc") ?? .data]
        p.canChooseDirectories = false
        p.prompt = "Abrir"
        if p.runModal() == .OK { Abierto.compartido.archivo = p.url }
    }
}

struct Pantalla: View {
    /// Lo que venga del menú contextual del Finder, ya elegido.
    var inicial: [URL] = []
    @StateObject private var motor = Motor()
    @State private var encima = false
    /// LO QUE SE VA A GUARDAR, tal y como lo seleccionaste. Era un solo URL y por eso al soltar
    /// varias cosas la app cogía su carpeta común: cuatro vídeos de 170 MB se convertían en los
    /// 11 GB de Descargas. Ahora es la lista y se enseña la lista.
    @State private var origenes: [URL] = []
    @State private var arrancado = false
    @State private var maximo = false
    @State private var cifrar = false
    @State private var contrasena = ""

    var body: some View {
        VStack(spacing: 0) {
            switch motor.estado {
            case .reposo:            zona
            case .trabajando(let p, let d, let r): trabajando(p, d, r)
            case .hecho(let e, let s, let t, let r): resultado(e, s, t, r)
            case .error(let m):      fallo(m)
            }
        }
        .padding(24)
        .animation(.easeInOut(duration: 0.18), value: motor.estado)
        .onAppear {
            // Solo la primera vez: si no, volver de un resultado reabriría lo del Finder.
            if !arrancado, !inicial.isEmpty { origenes = inicial; arrancado = true }
        }
    }

    // MARK: reposo — la pantalla que se ve el 90% del tiempo

    private var zona: some View {
        VStack(spacing: 18) {
            ZStack {
                RoundedRectangle(cornerRadius: 14)
                    .strokeBorder(style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
                    .foregroundStyle(encima ? Color.accentColor : Color.secondary.opacity(0.35))
                    .background(
                        RoundedRectangle(cornerRadius: 14)
                            .fill(encima ? Color.accentColor.opacity(0.08) : Color.clear))
                seleccion
            }
            .frame(height: 170)
            .onDrop(of: [.fileURL], isTargeted: $encima) { items in
                // SE GUARDA LO QUE SUELTAS, no la carpeta que lo contiene. Antes, con varias
                // cosas se cogía su carpeta común porque el motor solo aceptaba una entrada:
                // soltar cuatro vídeos de Descargas ponía a comprimir los 11 GB de Descargas.
                // El motor ya acepta varias entradas, así que aquí no hay nada que adivinar.
                var urls: [URL] = []
                let grupo = DispatchGroup()
                for it in items {
                    grupo.enter()
                    _ = it.loadObject(ofClass: URL.self) { u, _ in
                        if let u { urls.append(u) }
                        grupo.leave()
                    }
                }
                grupo.notify(queue: .main) {
                    guard !urls.isEmpty else { return }
                    // Soltar UN .vc significa "enséñame qué hay dentro", no "comprime un .vc
                    // dentro de otro .vc". Suponer lo segundo sería obedecer la letra y no la
                    // intención. Con varios se ignora la regla: ahí sí estás guardando archivos.
                    if urls.count == 1, urls[0].pathExtension.lowercased() == "vc" {
                        Abierto.compartido.archivo = urls[0]
                        return
                    }
                    // El orden en que llegan los URL de un arrastre no está garantizado, y una
                    // lista que baila entre dos arrastres iguales parece un fallo.
                    origenes = urls.sorted { $0.path < $1.path }
                }
                return true
            }
            .onTapGesture { elegir() }

            opciones
            Spacer(minLength: 0)
            Button(action: guardar) {
                Text("Guardar").frame(maxWidth: .infinity)
            }
            .keyboardShortcut(.defaultAction)
            .controlSize(.large)
            .buttonStyle(.borderedProminent)
            .disabled(origenes.isEmpty || (cifrar && contrasena.isEmpty))
        }
    }

    /// QUÉ se va a guardar. Con varias cosas se enseñan LAS COSAS, no un resumen: al soltar
    /// cuatro archivos la app decía «Downloads» y ponía a comprimir la carpeta entera, y desde
    /// fuera no había forma de notarlo hasta ver el tamaño del resultado.
    @ViewBuilder private var seleccion: some View {
        if origenes.isEmpty {
            VStack(spacing: 10) {
                Image(systemName: "arrow.down.doc")
                    .font(.system(size: 34, weight: .light))
                    .foregroundStyle(encima ? Color.accentColor : .secondary)
                Text("Arrastra aquí lo que quieras guardar")
                    .font(.system(size: 15, weight: .medium))
                Text("una carpeta o unos archivos").font(.caption).foregroundStyle(.secondary)
            }
        } else if origenes.count == 1 {
            let o = origenes[0]
            VStack(spacing: 10) {
                Image(systemName: esCarpeta(o) ? "folder.fill" : "doc.fill")
                    .font(.system(size: 34, weight: .light))
                    .foregroundStyle(encima ? Color.accentColor : .secondary)
                Text(o.lastPathComponent).font(.headline)
                    .lineLimit(1).truncationMode(.middle)
                Text(legible(Motor.tamano(de: origenes)))
                    .font(.subheadline).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 20)
        } else {
            VStack(spacing: 6) {
                Text("\(origenes.count) archivos · \(legible(Motor.tamano(de: origenes)))")
                    .font(.headline)
                ScrollView {
                    VStack(alignment: .leading, spacing: 3) {
                        ForEach(origenes, id: \.self) { u in
                            HStack(spacing: 6) {
                                Image(systemName: esCarpeta(u) ? "folder" : "doc")
                                    .font(.caption2).foregroundStyle(.secondary)
                                Text(u.lastPathComponent).font(.caption)
                                    .lineLimit(1).truncationMode(.middle)
                                Spacer(minLength: 0)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 22)
                }
            }
            .padding(.vertical, 16)
        }
    }

    private func esCarpeta(_ u: URL) -> Bool {
        (try? u.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
    }

    private var opciones: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 16) {
                Toggle("Comprimir al máximo", isOn: $maximo)
                Spacer()
                Toggle("Cifrar", isOn: $cifrar)
            }
            .toggleStyle(.checkbox)
            if maximo {
                // Se dice el precio ANTES de pagarlo. Es el ajuste que multiplica el tiempo por
                // quince, y descubrirlo a mitad de una espera larga es la peor forma de saberlo.
                Text("Ocupa bastante menos y tarda unas quince veces más.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if cifrar {
                SecureField("Contraseña", text: $contrasena)
                    .textFieldStyle(.roundedBorder)
                Text("Sin esta contraseña no hay forma de recuperar lo guardado. No se puede restablecer.")
                    .font(.caption).foregroundStyle(.orange)
            }
        }
    }

    // MARK: los otros tres estados

    private func trabajando(_ pct: Double, _ detalle: String, _ restante: TimeInterval?) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Text(origenes.count == 1 ? origenes[0].lastPathComponent
                                     : "\(origenes.count) archivos").font(.headline)
            ProgressView(value: pct > 0 ? pct : nil, total: 1)
                .progressViewStyle(.linear)
            // El detalle y el tiempo que falta, en la misma linea y con el tiempo a la derecha:
            // es lo que uno mira mientras espera, y separarlo en dos renglones lo esconde.
            HStack(spacing: 6) {
                Text(detalle).lineLimit(1).truncationMode(.middle)
                if let r = restante {
                    Text("·")
                    // La TILDE va siempre: de lo que falta no se sabe la composicion -un arbol
                    // puede pasar de codigo a video-, asi que esto es una estimacion y se dice.
                    Text("~\(Self.cuantoFalta(r))").monospacedDigit()
                }
            }
            .font(.caption).foregroundStyle(.secondary)
            Spacer()
            Button("Cancelar") { motor.cancelar() }.controlSize(.large)
        }
    }

    /// El tiempo que falta, en la unidad en la que uno lo piensa.
    ///
    /// Por debajo del minuto se redondea a cinco segundos: un contador que baja de uno en uno
    /// invita a mirarlo, y ademas fingiria una precision que la estimacion no tiene. Y por encima
    /// de la hora no se dan los minutos por el mismo motivo.
    static func cuantoFalta(_ s: TimeInterval) -> String {
        if s < 10 { return "unos segundos" }
        if s < 60 { return "\(Int((s / 5).rounded()) * 5) s" }
        if s < 3600 {
            let m = Int((s / 60).rounded())
            return m <= 1 ? "1 min" : "\(m) min"
        }
        let h = s / 3600
        return h < 2 ? "más de 1 hora" : "unas \(Int(h.rounded())) horas"
    }

    private func resultado(_ entrada: Int64, _ salida: Int64, _ seg: Double, _ ruta: URL) -> some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 44)).foregroundStyle(.green)
            Text("\(legible(entrada))  →  \(legible(salida))").font(.title3).monospacedDigit()
            if entrada > 0 {
                Text("ahorras \(legible(entrada - salida)) · \(Int(100 - Double(salida) / Double(entrada) * 100))% menos")
                    .foregroundStyle(.secondary)
            }
            Text(seg < 60 ? "en \(Int(seg)) s" : "en \(Int(seg / 60)) min")
                .font(.caption).foregroundStyle(.secondary)
            Spacer()
            HStack {
                Button("Ver en el Finder") { NSWorkspace.shared.activateFileViewerSelecting([ruta]) }
                Button("Guardar otra cosa") { origenes = []; motor.estado = .reposo }
                    .buttonStyle(.borderedProminent)
            }
            .controlSize(.large)
        }
    }

    private func fallo(_ mensaje: String) -> some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 40)).foregroundStyle(.orange)
            Text(mensaje).multilineTextAlignment(.center).font(.callout)
            Spacer()
            Button("Volver") { motor.estado = .reposo }.controlSize(.large).buttonStyle(.borderedProminent)
        }
    }

    // MARK: acciones

    private func elegir() {
        let p = NSOpenPanel()
        p.canChooseDirectories = true
        p.canChooseFiles = true
        // Se puede elegir más de una cosa. Estaba en false por la misma limitación de antes:
        // el motor solo aceptaba una entrada.
        p.allowsMultipleSelection = true
        p.prompt = "Elegir"
        if p.runModal() == .OK, !p.urls.isEmpty { origenes = p.urls.sorted { $0.path < $1.path } }
    }

    private func guardar() {
        guard !origenes.isEmpty else { return }
        let s = NSSavePanel()
        // Con una cosa, su nombre. Con varias, «Archivo»: ponerle el nombre de la carpeta que
        // las contiene es justo lo que hacía creer que se guardaba la carpeta entera.
        s.nameFieldStringValue = origenes.count == 1
            ? origenes[0].lastPathComponent + ".vc" : "Archivo.vc"
        s.canCreateDirectories = true
        s.prompt = "Guardar"
        guard s.runModal() == .OK, let destino = s.url else { return }
        motor.comprimir(origenes: origenes, destino: destino, maximo: maximo,
                        contrasena: cifrar ? contrasena : nil)
    }
}

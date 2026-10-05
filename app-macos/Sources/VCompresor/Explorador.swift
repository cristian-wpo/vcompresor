import SwiftUI

/// Lo que se ve al abrir un .vc: qué hay dentro y cómo sacar algo.
///
/// Es la pantalla que justifica todo lo demás. Uno hace copias durante meses sin mirarlas y solo
/// abre esta el día que ha borrado algo sin querer — normalmente con prisa y de mal humor. Por
/// eso lo primero que hay es un buscador con el foco puesto, y por eso recuperar UN fichero está
/// al mismo nivel que recuperarlo todo, no escondido en un menú.
struct Explorador: View {
    let archivo: URL
    @StateObject private var motor = Motor()
    @State private var entradas: [Entrada] = []
    @State private var busca = ""
    @State private var seleccion: Set<Entrada.ID> = []
    @State private var contrasena = ""
    @State private var pideContrasena = false
    @State private var fallo: String?
    @State private var cargando = true
    /// La ruta que se está sacando para abrir, si hay alguna. Sirve para poner la ruedecita en
    /// SU fila y no cambiar de pantalla: abrir un fichero para mirarlo no debería llevarse por
    /// delante la lista que estabas mirando.
    @State private var abriendo: String?
    @State private var abrirFallo: String?
    @FocusState private var focoBusqueda: Bool

    private var visibles: [Entrada] {
        guard !busca.isEmpty else { return entradas }
        return entradas.filter { $0.ruta.localizedCaseInsensitiveContains(busca) }
    }
    private var elegidas: [Entrada] { entradas.filter { seleccion.contains($0.id) } }

    var body: some View {
        VStack(spacing: 0) {
            switch motor.estado {
            case .trabajando(_, let d, _): ocupado(d)
            case .hecho(_, _, _, let r): recuperado(r)
            case .error(let m):         problema(m)
            default:
                if pideContrasena { pedirClave }
                else if let f = fallo { problema(f) }
                else if cargando { ocupado("leyendo el archivo…") }
                else { contenido }
            }
        }
        .frame(minWidth: 560, minHeight: 420)
        .onAppear(perform: cargar)
    }

    // MARK: contenido

    private var contenido: some View {
        VStack(spacing: 0) {
            HStack {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField("Buscar dentro de \(archivo.lastPathComponent)", text: $busca)
                    .textFieldStyle(.plain)
                    .focused($focoBusqueda)
                if !busca.isEmpty {
                    Button { busca = "" } label: { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.plain).foregroundStyle(.secondary)
                }
            }
            .padding(10)
            .background(.quaternary.opacity(0.4))

            List(visibles, selection: $seleccion) { e in
                HStack(spacing: 8) {
                    Image(systemName: e.icono).foregroundStyle(.secondary).frame(width: 16)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(e.nombre)
                        if !e.carpeta.isEmpty {
                            Text(e.carpeta).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Spacer()
                    if abriendo == e.ruta {
                        ProgressView().controlSize(.small)
                    } else if e.tipo == "-" {
                        Text(legible(e.bytes)).font(.caption).monospacedDigit().foregroundStyle(.secondary)
                    }
                }
                .contentShape(Rectangle())
                // DOBLE CLIC = ABRIRLO, como en cualquier sitio de un Mac. Va como gesto
                // SIMULTÁNEO para no pisar la selección que maneja la propia lista.
                .simultaneousGesture(TapGesture(count: 2).onEnded { abrir(e) })
                .contextMenu {
                    // El doble clic no se ve; el menú del botón derecho, sí. Que la forma
                    // rápida sea invisible no es excusa para que sea la única.
                    if e.tipo != "d" { Button("Abrir") { abrir(e) } }
                    Button("Recuperar…") { recuperar(rutas: [e.ruta]) }
                }
                .tag(e.id)
            }
            .listStyle(.inset(alternatesRowBackgrounds: true))

            Divider()
            HStack {
                Text(busca.isEmpty
                     ? "\(entradas.count) elementos"
                     : "\(visibles.count) de \(entradas.count)")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                if let a = abrirFallo {
                    Text(a).font(.caption).foregroundStyle(.orange)
                        .lineLimit(1).truncationMode(.middle)
                    Spacer()
                }
                Button("Recuperar todo") { recuperar(rutas: []) }
                Button(elegidas.count == 1 ? "Recuperar este" : "Recuperar \(elegidas.count)") {
                    recuperar(rutas: elegidas.map(\.ruta))
                }
                .buttonStyle(.borderedProminent)
                .disabled(elegidas.isEmpty)
            }
            .padding(10)
        }
    }

    private var pedirClave: some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "lock.fill").font(.system(size: 38)).foregroundStyle(.secondary)
            Text("Este archivo está cifrado").font(.headline)
            SecureField("Contraseña", text: $contrasena)
                .textFieldStyle(.roundedBorder).frame(width: 260)
                .onSubmit { pideContrasena = false; cargando = true; cargar() }
            Spacer()
            Button("Abrir") { pideContrasena = false; cargando = true; cargar() }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .disabled(contrasena.isEmpty)
        }
        .padding(28)
    }

    private func ocupado(_ q: String) -> some View {
        VStack(spacing: 12) {
            Spacer(); ProgressView(); Text(q).font(.caption).foregroundStyle(.secondary); Spacer()
        }
    }

    private func recuperado(_ ruta: URL) -> some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "checkmark.circle.fill").font(.system(size: 42)).foregroundStyle(.green)
            Text("Recuperado").font(.title3)
            Spacer()
            HStack {
                Button("Ver en el Finder") { NSWorkspace.shared.activateFileViewerSelecting([ruta]) }
                Button("Volver") { motor.estado = .reposo }.buttonStyle(.borderedProminent)
            }.controlSize(.large)
        }.padding(28)
    }

    private func problema(_ m: String) -> some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 36)).foregroundStyle(.orange)
            Text(m).multilineTextAlignment(.center).font(.callout)
            Spacer()
            Button("Volver") { fallo = nil; motor.estado = .reposo; cargando = true; cargar() }
                .buttonStyle(.borderedProminent).controlSize(.large)
        }.padding(28)
    }

    // MARK: acciones

    private func cargar() {
        cargando = true
        DispatchQueue.global(qos: .userInitiated).async {
            let r = Motor.listar(archivo: archivo, contrasena: contrasena.isEmpty ? nil : contrasena)
            DispatchQueue.main.async {
                cargando = false
                switch r {
                case .success(let v):
                    entradas = v
                    fallo = nil
                    focoBusqueda = true
                case .failure(.pideContrasena):
                    pideContrasena = true
                case .failure(.otro(let m)):
                    fallo = m
                }
            }
        }
    }

    /// Abrir una entrada: sacarla a un temporal y dársela al programa que le toque.
    ///
    /// Una CARPETA no se abre: se entra en ella filtrando la lista por su ruta, que es lo que uno
    /// quiere al hacer doble clic en una carpeta y no «sácame los 400 ficheros de dentro».
    private func abrir(_ e: Entrada) {
        abrirFallo = nil
        if e.tipo == "d" {
            busca = e.ruta + "/"
            return
        }
        guard abriendo == nil else { return }   // uno cada vez: dos a la vez sólo confunden
        abriendo = e.ruta
        Motor.sacarParaVer(archivo: archivo, ruta: e.ruta, bytes: e.bytes,
                           contrasena: contrasena.isEmpty ? nil : contrasena) { r in
            abriendo = nil
            switch r {
            case .success(let u):
                // Si el sistema no sabe abrirlo, se enseña en el Finder en vez de no hacer nada.
                if !NSWorkspace.shared.open(u) {
                    NSWorkspace.shared.activateFileViewerSelecting([u])
                }
            case .failure(.otro(let m)):
                abrirFallo = m
            case .failure(.pideContrasena):
                abrirFallo = "Hace falta la contraseña."
            }
        }
    }

    /// `rutas` vacio = recuperarlo todo.
    private func recuperar(rutas: [String]) {
        let p = NSOpenPanel()
        p.canChooseDirectories = true
        p.canChooseFiles = false
        p.prompt = "Recuperar aquí"
        p.message = rutas.isEmpty ? "¿Dónde dejo todo el contenido?" : "¿Dónde lo dejo?"
        guard p.runModal() == .OK, let destino = p.url else { return }
        motor.recuperarVarios(archivo: archivo, destino: destino, rutas: rutas,
                              contrasena: contrasena.isEmpty ? nil : contrasena)
    }
}

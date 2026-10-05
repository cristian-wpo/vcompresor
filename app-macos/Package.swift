// swift-tools-version:5.9
import PackageDescription

// La app es un ejecutable Swift normal; el envoltorio .app se arma aparte con un guion. La
// alternativa era un .xcodeproj, que es un fichero enorme, generado y horrible de versionar
// para una app de dos pantallas.
let package = Package(
    name: "VCompresor",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(name: "VCompresor", path: "Sources/VCompresor"),
        // La extension de Finder es un binario APARTE que el sistema carga dentro del Finder.
        // El punto de entrada NO es main(): es _NSExtensionMain, que es lo que espera macOS de
        // una extension. Sin ese flag el binario compila y el sistema no lo puede cargar.
        .executableTarget(name: "FinderExt", path: "Sources/FinderExt",
                          linkerSettings: [.unsafeFlags(["-Xlinker", "-e", "-Xlinker", "_NSExtensionMain"])]),
    ]
)

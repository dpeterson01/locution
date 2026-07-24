// swift-tools-version: 6.0
import PackageDescription

// Phase-1 spike: a minimal, crash-isolated sidecar that exposes Apple's
// on-device Foundation Model behind an OpenAI-compatible HTTP surface.
// Bundled with Locution later as a Tauri `externalBin`; for the spike it is
// built standalone with `swift build -c release`.
let package = Package(
    name: "afm-sidecar",
    platforms: [.macOS("26.0")],
    targets: [
        .executableTarget(
            name: "afm-sidecar",
            path: "Sources/afm-sidecar"
        )
    ]
)

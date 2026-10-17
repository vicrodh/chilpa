import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Patterns"

// The dynamic background's pieces, each with a fixed triad so the Gallery
// shows the same picture every time.
TitledFlow {
    title: "Ambient"
    CoverColors { consumer: "galleryColors" }
    AmbientField { width: Metrics.listenArt; height: Metrics.listenArt; primary: Theme.color.accent; secondary: Theme.color.casting; accent: Theme.color.statusPositive; running: false }
    AmbientCanvas { width: Metrics.listenArt; height: Metrics.listenArt; primary: Theme.color.accent; secondary: Theme.color.casting; accent: Theme.color.statusPositive }
    AtmosphereField { width: Metrics.listenArt; height: Metrics.listenArt; fallbackSource: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"; animated: false; dim: Metrics.ambientDim }
    AtmosphereLayers { width: Metrics.listenArt; height: Metrics.listenArt; fallbackSource: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"; dim: Metrics.ambientDim }
    WallpaperField { width: Metrics.listenArt; height: Metrics.listenArt; source: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"; blur: Metrics.ambientVeilLight; dim: Metrics.ambientDim }
    AppBackground { paletteConsumer: "gallery"; width: Metrics.listenWide; height: Metrics.listenArt; plane: Qt.rect(Metrics.listenArt, 0, Metrics.listenArt, Metrics.listenArt) }
}

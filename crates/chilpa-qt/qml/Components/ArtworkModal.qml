import QtQuick
import "../Theme"
import "../Primitives"

// The full album cover, opened from its header.
Modal {
    id: dialog

    property url artwork

    width: Math.min(Metrics.artworkModal, parent ? parent.width - Metrics.artworkModalGutter : Metrics.artworkModal)
    height: width

    function show(source) {
        artwork = source
        open()
    }

    ArtTile {
        width: parent.width
        height: parent.height
        source: dialog.artwork
        radius: Metrics.radiusCard
    }
}

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"
import "../Primitives"

// A cover at a size the eye can read, over the small one it grew from:
// centered above `target`, kept inside the window, and shown while `shown`
// holds. It takes no pointer, so it never steals the hover that opened it.
Popup {
    id: preview
    property Item target: null
    property url source
    property bool shown: false
    parent: target
    visible: shown && target !== null
    modal: false
    focus: false
    closePolicy: Popup.NoAutoClose
    padding: Metrics.artPreviewFrame
    margins: Metrics.gap8
    width: Metrics.artPreview + Metrics.artPreviewFrame * 2 // derived: the image and its frame
    height: width
    x: target ? (target.width - width) / 2 : 0 // derived: centered over the target
    y: -height - Metrics.gap8

    background: Item {
        RectangularShadow {
            anchors.fill: frame
            radius: frame.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        Rectangle {
            id: frame
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised
            Edge {}
        }
    }
    contentItem: ArtTile {
        objectName: "artPreviewImage"
        source: preview.source
        radius: Metrics.radiusArtSmall
        edged: false
    }
}

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"
import "../Primitives"

// A cover at a size the eye can read, over the small one it grew from:
// centered above `target`, or under it when the window has no room above
// (a bar along the top), kept inside the window, and shown while `shown`
// holds. It is never drawn over `target`: there it would take the hover
// that keeps it open, close, give the hover back and open again.
Popup {
    id: preview
    PopupInput { surface: preview }
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
    readonly property bool under: shown && target !== null && target.mapToItem(null, 0, 0).y < height + Metrics.gap8 * 2 // derived: the gap and the window's margin
    y: under ? target.height + Metrics.gap8 : -height - Metrics.gap8

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

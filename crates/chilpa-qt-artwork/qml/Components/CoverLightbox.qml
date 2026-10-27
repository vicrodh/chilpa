import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A cover or a portrait shown whole, as QBZ's lightbox: the window under a
// veil, the picture in its own proportion inside nine tenths of it, decoded
// no larger than that. Escape or a click on the veil closes it; a click on
// the picture does not, and the wheel does not reach the page below.
Popup {
    id: lightbox
    PopupInput { surface: lightbox }
    property url source
    onClosed: source = ""
    function show(picture) {
        lightbox.source = picture
        open()
    }
    parent: Overlay.overlay
    x: 0
    y: 0
    width: parent ? parent.width : 0
    height: parent ? parent.height : 0
    padding: 0
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape
    background: Rectangle { color: Theme.color.lightboxScrim }
    contentItem: Item {
        id: stage
        readonly property real roomWide: lightbox.width * Metrics.lightboxShare
        readonly property real roomHigh: lightbox.height * Metrics.lightboxShare
        // The picture's own proportion, square until it says.
        readonly property real ratio: picture.implicitWidth > 0 && picture.implicitHeight > 0 ? picture.implicitWidth / picture.implicitHeight : 1
        // no cursor: the veil only closes on a click, as QBZ's does
        MouseArea {
            objectName: "lightboxVeil"
            anchors.fill: parent
            onClicked: lightbox.close()
            onWheel: (wheel) => wheel.accepted = true
        }
        Image {
            id: picture
            objectName: "lightboxPicture"
            anchors.centerIn: parent
            width: Math.min(stage.roomWide, stage.roomHigh * stage.ratio)
            height: Math.min(stage.roomHigh, stage.roomWide / stage.ratio)
            source: lightbox.source
            cache: false
            asynchronous: true
            fillMode: Image.PreserveAspectFit
            sourceSize: Qt.size(Math.round(stage.roomWide), Math.round(stage.roomHigh))
            smooth: true
            visible: status === Image.Ready
            // no cursor: a click on the picture is the picture's, not the veil's
            MouseArea { anchors.fill: parent }
        }
        Text {
            anchors.centerIn: parent
            visible: lightbox.source.toString() !== "" && picture.status === Image.Error
            text: I18n.tr("Could not load the image", I18n.rev)
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
    }
}

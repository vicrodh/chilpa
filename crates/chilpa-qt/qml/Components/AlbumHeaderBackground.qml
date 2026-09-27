import QtQuick
import "../Theme"

// QBZ's album atmosphere: a tiny pre-blurred field is cropped four different
// ways for Full, drawn once for Colors, then darkened and faded into the page.
Item {
    id: root

    property url art
    property string mode: "full"

    readonly property string field: art.toString() === "" ? ""
        : "image://atmosphere/" + encodeURIComponent(art.toString())
    readonly property bool active: mode !== "off" && field !== ""
    readonly property real tau: 6.283185
    readonly property var layers: [
        { growW: 520, growH: 420, baseX: -260, baseY: -210,
          offX: 0.35 * Math.sin(1.9 * tau) * 132, offY: Math.sin(1.1 * tau) * 58, opacity: 0.92 },
        { growW: 620, growH: 500, baseX: -310, baseY: -250,
          offX: -(Math.sin(0.4 * tau) + 0.42 * Math.sin(2.6 * tau)) * 156,
          offY: -Math.sin(1.7 * tau) * 74, opacity: 0.38 },
        { growW: 700, growH: 540, baseX: -350, baseY: -270,
          offX: Math.sin(1.6 * tau) * 118,
          offY: (Math.sin(0.2 * tau) + 0.28 * Math.sin(2.8 * tau)) * 92, opacity: 0.24 },
        { growW: 560, growH: 620, baseX: -280, baseY: -310,
          offX: -Math.sin(2.4 * tau) * 102, offY: Math.sin(1.4 * tau) * 128, opacity: 0.18 }
    ]

    visible: active
    enabled: false
    clip: true

    Rectangle { anchors.fill: parent; color: Theme.color.albumHeaderBase }

    Image {
        anchors.fill: parent
        visible: root.mode === "colors"
        source: root.field
        asynchronous: true
        fillMode: Image.PreserveAspectCrop
    }
    Repeater {
        model: root.mode === "full" ? root.layers : []
        Image {
            required property var modelData
            source: root.field
            asynchronous: true
            fillMode: Image.PreserveAspectCrop
            width: root.width + modelData.growW
            height: root.height + modelData.growH
            x: modelData.baseX + modelData.offX
            y: modelData.baseY + modelData.offY
            opacity: modelData.opacity
        }
    }

    Rectangle { anchors.fill: parent; color: Theme.color.albumHeaderDim }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: Theme.color.albumHeaderTop }
            GradientStop { position: 0.35; color: "transparent" }
            GradientStop { position: 1.0; color: Theme.color.albumHeaderBottom }
        }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.55; color: "transparent" }
            GradientStop { position: 1.0; color: Theme.color.surfaceWindow }
        }
    }
}

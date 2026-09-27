import QtQuick
import "../Theme"

// Five stars without five controls: two glyph runs and one hit target keep
// the rating cheap enough for every visible track row.
Item {
    id: root

    property int value: 0
    property bool interactive: true
    signal rated(int value)

    readonly property int preview: area.containsMouse
        ? Math.max(1, Math.min(Metrics.ratingMaximum,
                              Math.ceil(area.mouseX / width * Metrics.ratingMaximum)))
        : value
    readonly property string emptyStars: Array(Metrics.ratingMaximum + 1).join(Icons.star)
    readonly property string fullStars: Array(Metrics.ratingMaximum + 1).join(Icons.starFilled)

    implicitWidth: Metrics.ratingStar * Metrics.ratingMaximum
    implicitHeight: Metrics.ratingStar

    Text {
        anchors.fill: parent
        text: root.emptyStars
        font.family: Icons.outline
        font.pixelSize: Metrics.ratingStar
        color: Theme.color.textTertiary
    }
    Item {
        width: parent.width * root.preview / Metrics.ratingMaximum
        height: parent.height
        clip: true
        Text {
            width: root.width
            height: root.height
            text: root.fullStars
            font.family: Icons.filled
            font.pixelSize: Metrics.ratingStar
            color: Theme.color.accent
        }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        enabled: root.interactive
        hoverEnabled: true
        onClicked: {
            root.value = root.preview
            root.rated(root.value)
        }
    }
}

import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Five stars without five controls: two glyph runs and one hit target keep
// the rating cheap enough for every visible track row. I draw only the filled
// prefix so each row can batch its glyphs without a separate clip.
Item {
    id: root

    property int value: 0
    property bool interactive: true
    property int pointerPreview: 0
    property color baseColor: Theme.color.textTertiary
    property int size: Metrics.ratingStar
    signal rated(int value)

    readonly property int preview: area.containsMouse
        ? Math.max(1, Math.min(Metrics.ratingMaximum,
                              Math.ceil(area.mouseX / width * Metrics.ratingMaximum)))
        : pointerPreview > 0 ? pointerPreview : value
    readonly property string emptyStars: Array(Metrics.ratingMaximum + 1).join(Icons.star)
    readonly property string fullStars: Array(Metrics.ratingMaximum + 1).join(Icons.starFilled)

    implicitWidth: size * Metrics.ratingMaximum
    implicitHeight: size

    Text {
        anchors.fill: parent
        text: root.emptyStars
        font.family: Icons.outline
        font.pixelSize: root.size
        color: root.baseColor
    }
    Text {
        anchors.fill: parent
        text: root.fullStars.slice(0, Math.max(0, root.preview))
        font.family: Icons.filled
        font.pixelSize: root.size
        color: Theme.color.accent
    }
    MouseArea {
        id: area
        cursorShape: root.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
        anchors.fill: parent
        enabled: root.interactive
        hoverEnabled: true
        onClicked: root.rated(root.value === root.preview ? 0 : root.preview)
        onDoubleClicked: (mouse) => { mouse.accepted = true }
    }
}

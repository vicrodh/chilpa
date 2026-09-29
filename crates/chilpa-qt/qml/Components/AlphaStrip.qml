import QtQuick
import com.blitzfc.chilpa
import "../Theme"

// The letters beside a grouped list: "#" and A to Z, always the twenty-seven.
// One that starts no group is dimmed and does nothing; the others jump to
// where they start. `letters` is a list of [letter, row].
Item {
    id: strip

    property var letters: []
    readonly property var alphabet: "#ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("")
    readonly property real cell: Math.min(Metrics.alphaCell, height / alphabet.length)

    signal jumped(int row)

    function rowOf(letter) {
        const found = letters.find((entry) => entry[0] === letter)
        return found ? found[1] : -1
    }

    implicitWidth: Metrics.alphaStrip

    Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width

        Repeater {
            model: strip.alphabet

            Text {
                id: letter
                required property string modelData
                readonly property int row: strip.rowOf(modelData)

                width: strip.width
                height: strip.cell
                text: modelData
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pointSize: Type.micro
                color: row < 0 ? Theme.color.textSecondary : over.hovered ? Theme.color.accent : Theme.color.textPrimary
                opacity: row < 0 ? Metrics.disabledOpacity : 1

                HoverHandler { id: over; enabled: letter.row >= 0; cursorShape: Qt.PointingHandCursor }
                TapHandler { enabled: letter.row >= 0; onTapped: strip.jumped(letter.row) }
            }
        }
    }
}

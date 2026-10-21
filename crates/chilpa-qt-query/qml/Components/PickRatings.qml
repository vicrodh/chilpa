import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Stars (F3): any rating, at least, exactly, between and unrated, two by
// two; the one the query holds is marked.
Grid {
    id: ratings
    // The rating the query holds: {id, min, max} or null.
    property var rating: null
    signal edit(var change)
    readonly property var choices: [[-1, -1], [4, 5], [3, 5], [5, 5], [3, 3], [2, 4], [0, 0]]
    columns: 2
    columnSpacing: Refine.ratingColumnsApart
    rowSpacing: Refine.segTrackPad
    function label(min, max) {
        if (min < 0) return I18n.tr("Any rating", I18n.rev)
        if (min === 0) return I18n.tr("unrated", I18n.rev)
        if (min === max) return I18n.tr("exactly", I18n.rev) + " " + min + "★"
        if (max === 5) return I18n.tr("at least", I18n.rev) + " " + min + "★"
        return min + "★ – " + max + "★"
    }
    Repeater {
        model: ratings.choices
        Rectangle {
            required property var modelData
            readonly property bool on: modelData[0] < 0 ? !ratings.rating : !!ratings.rating && ratings.rating.min === modelData[0] && ratings.rating.max === modelData[1]
            width: (ratings.width - ratings.columnSpacing) / 2 // derived: two columns
            height: Refine.pickRow
            radius: Refine.pickRowRound
            color: area.containsMouse ? Theme.color.stateHover : "transparent"
            Row {
                x: Refine.pickPad
                anchors.verticalCenter: parent.verticalCenter
                spacing: Refine.pickPad
                Icon { anchors.verticalCenter: parent.verticalCenter; glyph: parent.parent.on ? Icons.circleDotFilled : Icons.circle; filled: parent.parent.on; size: Refine.pickStateIcon; color: parent.parent.on ? Theme.color.accent : Theme.color.textTertiary }
                Text { anchors.verticalCenter: parent.verticalCenter; text: ratings.label(modelData[0], modelData[1]); font.pointSize: Type.body; color: Theme.color.textPrimary }
            }
            Text {
                anchors.right: parent.right
                anchors.rightMargin: Refine.pickPad
                anchors.verticalCenter: parent.verticalCenter
                visible: modelData[0] > 0
                text: "★★★★★".slice(0, Math.max(0, modelData[0]))
                font.pointSize: Type.meta
                font.letterSpacing: Metrics.hairline
                color: Theme.color.textTertiary
            }
            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: modelData[0] < 0 ? (ratings.rating ? ratings.edit({ op: "remove", id: ratings.rating.id }) : null) : ratings.edit({ op: "rating", min: modelData[0], max: modelData[1] })
            }
        }
    }
}

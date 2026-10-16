import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The stars a track keeps on the Tracks screen: a toggle a row, five stars
// down to one and then the tracks with none. It holds no choice of its
// own; it says the next one and the screen keeps it.
Column {
    id: filter
    // The stars chosen, 0 for none; empty keeps every track.
    property var ratings: []
    signal chosen(var ratings)
    spacing: Metrics.gap8

    Text {
        text: I18n.tr("Rating", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    Repeater {
        model: [5, 4, 3, 2, 1, 0]
        Item {
            required property int modelData
            readonly property bool kept: filter.ratings.indexOf(modelData) >= 0
            width: filter.width
            height: Metrics.sourceFilterRow
            // The stars as a track row draws them: filled up to the
            // rating, outlined after it.
            Row {
                visible: modelData > 0
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    text: Array(modelData + 1).join(Icons.starFilled)
                    font.family: Icons.filled
                    font.pixelSize: Metrics.ratingStar
                    color: Theme.color.accent
                }
                Text {
                    text: Array(Metrics.ratingMaximum - modelData + 1).join(Icons.star)
                    font.family: Icons.outline
                    font.pixelSize: Metrics.ratingStar
                    color: Theme.color.textTertiary
                }
            }
            Text {
                visible: modelData === 0
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Unrated", I18n.rev)
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            Toggle {
                objectName: "ratingToggle-" + modelData
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: kept
                onToggled: filter.chosen(kept ? filter.ratings.filter(v => v !== modelData) : filter.ratings.concat([modelData]))
            }
        }
    }
}

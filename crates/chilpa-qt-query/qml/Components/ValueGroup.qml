import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// A condition's values as one joined block (F4): 24 tall, radius 7, a
// hairline between values and an x on each one that can go. Kept out, the
// block has no fill and its words are quieter. Never separate pills, never
// a loose "or" between values. A click on a value opens its field's picker.
Rectangle {
    id: block
    // [{label, value, sub, removable}]
    property var tokens: []
    property bool negated: false
    // How wide the block may grow before its values wrap.
    property real room: implicitWidth
    signal picked()
    signal removed(string value)

    visible: tokens.length > 0
    implicitWidth: Math.min(flow.implicitWidth, room)
    implicitHeight: flow.implicitHeight
    width: implicitWidth
    radius: Refine.valueRound
    color: negated ? "transparent" : Theme.color.surfaceRaised
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline

    Flow {
        id: flow
        width: block.room
        Repeater {
            model: block.tokens
            Item {
                id: token
                required property var modelData
                required property int index
                objectName: "valueToken"
                implicitWidth: words.implicitWidth + Refine.valuePadLeft + Refine.valueApart + (modelData.removable ? remove.implicitWidth + Refine.valuePadRight : Refine.valueNoRemove)
                implicitHeight: Refine.valueTall
                Rectangle {
                    visible: token.index > 0
                    width: Metrics.hairline
                    height: parent.height
                    color: Theme.color.strokeHairline
                }
                Row {
                    id: words
                    x: Refine.valuePadLeft
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Refine.valueSubApart
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: token.modelData.label
                        font.pointSize: Type.row
                        font.weight: Type.medium
                        color: block.negated ? Theme.color.textSecondary : Theme.color.textPrimary
                    }
                    Text {
                        visible: token.modelData.sub !== ""
                        anchors.verticalCenter: parent.verticalCenter
                        text: token.modelData.sub
                        font.pointSize: Type.small
                        color: Theme.color.textTertiary
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: block.picked()
                }
                RemoveDot {
                    id: remove
                    objectName: "valueRemove"
                    visible: token.modelData.removable
                    anchors.right: parent.right
                    anchors.rightMargin: Refine.valuePadRight
                    anchors.verticalCenter: parent.verticalCenter
                    round: Refine.valueRemoveRound
                    tint: Theme.color.textTertiary
                    tip: I18n.tr("Remove", I18n.rev)
                    onClicked: block.removed(token.modelData.value)
                }
            }
        }
    }
}

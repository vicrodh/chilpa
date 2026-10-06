import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// A choice with its name: the name on the left, and on the right whatever
// it is chosen with -- a segmented control, a field, a row of swatches. The
// sibling of OptionRow for what is not a switch.
Item {
    id: row

    property string title
    default property alias control: slot.data

    implicitHeight: Math.max(Metrics.optionRow, slot.implicitHeight + Metrics.gap10)

    Text {
        anchors.left: parent.left
        anchors.right: slot.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.title
        elide: Text.ElideRight
        font.pointSize: Type.body
        font.weight: Type.medium
        color: Theme.color.textPrimary
    }

    Row {
        id: slot
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
    }
}

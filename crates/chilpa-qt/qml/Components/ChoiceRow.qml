import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A choice with its name: the name on the left, and on the right whatever
// it is chosen with -- a segmented control, a field, a row of swatches. The
// sibling of OptionRow for what is not a switch. When the name and the
// control do not fit side by side, the control goes under the name.
Item {
    id: row

    property string title
    default property alias control: slot.data
    readonly property bool stacked: name.implicitWidth + Metrics.gap12 + slot.implicitWidth > width

    implicitHeight: stacked ? Metrics.gap4 + name.implicitHeight + Metrics.gap6 + slot.implicitHeight + Metrics.gap6 // derived: padding, the name, the gap, the control and padding
        : Math.max(Metrics.optionRow, slot.implicitHeight + Metrics.gap10)

    Text {
        id: name
        y: row.stacked ? Metrics.gap4 : (row.height - height) / 2 // derived: centred beside the control
        width: row.stacked ? row.width : row.width - slot.width - Metrics.gap12
        text: row.title
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        font.weight: Type.medium
        color: Theme.color.textPrimary
    }

    Row {
        id: slot
        anchors.right: parent.right
        y: row.stacked ? name.y + name.height + Metrics.gap6 : (row.height - height) / 2 // derived: under the name, or centred beside it
    }
}

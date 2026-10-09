import QtQuick
import "../Theme"

// A title with a quiet credit after it on the same baseline, such as a
// track's composer: the credit keeps up to a third of the width, the title
// the rest, and each elides on its own.
Row {
    id: line
    property string title
    property string credit
    property real titleSize: Type.row
    property int titleWeight: Type.regular
    property color titleColor: Theme.color.textPrimary
    spacing: Metrics.gap6

    Text {
        id: titleText
        width: Math.min(implicitWidth, line.width - (creditText.visible ? Math.min(creditText.implicitWidth, line.width / 3) + line.spacing : 0)) // derived: the credit keeps up to a third
        text: line.title
        elide: Text.ElideRight
        font.pointSize: line.titleSize
        font.weight: line.titleWeight
        color: line.titleColor
    }
    Text {
        id: creditText
        objectName: "titleCredit"
        visible: line.credit !== ""
        width: line.width - titleText.width - line.spacing
        anchors.baseline: titleText.baseline
        text: line.credit
        elide: Text.ElideRight
        font.pointSize: Type.meta
        color: Theme.color.textTertiary
    }
}

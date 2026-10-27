import QtQuick
import "../Theme"

// A title with a quiet credit after it on the same baseline, such as a
// track's composer: the credit keeps up to a third of the width, the title
// the rest, and each elides on its own.
Item {
    id: line
    property string title
    property string credit
    property bool explicitContent: false
    property real titleSize: Type.row
    property int titleWeight: Type.regular
    property color titleColor: Theme.color.textPrimary
    readonly property real spacing: Metrics.gap6
    readonly property real advisoryWidth: advisory.active ? advisory.width + line.spacing : 0
    implicitWidth: titleText.implicitWidth + advisoryWidth + (creditText.visible ? line.spacing + creditText.implicitWidth : 0)
    implicitHeight: Math.max(titleText.implicitHeight, creditText.implicitHeight)

    Text {
        id: titleText
        objectName: "creditedText"
        width: Math.max(0, Math.min(implicitWidth, line.width - line.advisoryWidth - (creditText.visible ? Math.min(creditText.implicitWidth, line.width / 3) + line.spacing : 0))) // derived: the credit keeps up to a third
        text: line.title
        textFormat: Text.PlainText
        elide: Text.ElideRight
        font.pointSize: line.titleSize
        font.weight: line.titleWeight
        color: line.titleColor
    }
    Loader {
        id: advisory
        active: line.explicitContent
        x: titleText.width + line.spacing
        anchors.verticalCenter: titleText.verticalCenter
        sourceComponent: ExplicitBadge {}
    }
    Text {
        id: creditText
        x: titleText.width + line.advisoryWidth + line.spacing
        objectName: "titleCredit"
        visible: line.credit !== ""
        width: Math.max(0, line.width - x)
        anchors.baseline: titleText.baseline
        text: line.credit
        textFormat: Text.PlainText
        elide: Text.ElideRight
        font.pointSize: Type.meta
        color: Theme.color.textTertiary
    }
}

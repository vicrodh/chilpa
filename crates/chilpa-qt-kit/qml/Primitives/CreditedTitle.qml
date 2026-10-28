import QtQuick
import "../Theme"

// A title with independently linked featured and optional composer credits:
// the credit keeps up to a third of the width, the title
// the rest, and each elides on its own.
Item {
    id: line
    property string title
    property string credit
    property string composer
    property bool richCredits: false
    function linkAt(from, x, y) {
        const point = creditText.mapFromItem(from, x, y)
        return creditText.visible ? creditText.linkAt(point.x, point.y) : ""
    }
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
        visible: line.credit !== "" || line.composer !== ""
        width: Math.max(0, line.width - x)
        anchors.baseline: titleText.baseline
        text: [line.credit, line.composer].filter(Boolean).join(" · ")
        // Styled text keeps the link color and elides; rich text does neither, so a linked
        // name ran over the next column and took Qt's own link color.
        textFormat: line.richCredits ? Text.StyledText : Text.PlainText
        linkColor: color
        elide: Text.ElideRight
        font.pointSize: Type.meta
        color: Theme.color.textTertiary
    }
}

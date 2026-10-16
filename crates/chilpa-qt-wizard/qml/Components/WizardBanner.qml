import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// QBZ's WarningBanner: a title and a body over a wash of the variant's
// tone, info, success or warning.
Item {
    id: banner
    property string title
    property string body
    property string variant: "warning"
    readonly property color tone: variant === "info" ? Theme.color.accent
                                : variant === "success" ? Theme.color.statusPositive : Theme.color.statusWarning
    width: parent ? parent.width : 0
    implicitHeight: row.implicitHeight + Metrics.gap24

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: banner.tone
        opacity: WizardFrame.toneFill
    }
    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: "transparent"
        border.width: 1
        border.color: banner.tone
        opacity: WizardFrame.toneEdge
    }
    Row {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap10
        Icon {
            glyph: Lucide.info
            iconFamily: Lucide.font
            size: Metrics.iconSm
            color: banner.tone
        }
        Column {
            width: parent.width - Metrics.iconSm - Metrics.gap10 // derived: beside the glyph
            spacing: Metrics.gap2
            Text {
                visible: banner.title !== ""
                width: parent.width
                text: banner.title
                color: banner.tone
                font.pointSize: Type.small
                font.weight: Type.semibold
                wrapMode: Text.WordWrap
            }
            Text {
                visible: banner.body !== ""
                width: parent.width
                text: banner.body
                color: Theme.color.textSecondary
                font.pointSize: Type.small
                wrapMode: Text.WordWrap
            }
        }
    }
}

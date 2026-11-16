import QtQuick
import QtQuick.Controls.Basic
import "../Theme"
import "../Primitives"

// Transient messages scroll within a reserved slot. No text is elided.
Item {
    id: area
    property string label: ""
    property string text: ""
    property color labelColor: Theme.color.textSecondary
    property int lines: Metrics.statusMessageLines
    property bool reserveProgress: true
    property real progress: -1
    property string labelObjectName: "statusLabel"
    property string textObjectName: "statusMessage"
    implicitHeight: metrics.lineSpacing * lines + (reserveProgress ? Metrics.gap8 + Metrics.progress : 0)
    FontMetrics { id: metrics; font: message.font }
    Flickable {
        id: viewport
        objectName: "statusViewport"
        width: parent.width
        height: parent.height - (area.reserveProgress ? Metrics.gap8 + Metrics.progress : 0)
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
        Column {
            id: body
            width: Math.max(0,viewport.width - Metrics.gap8)
            spacing: Metrics.gap6
            Row {
                width: parent.width
                visible: area.label !== ""
                spacing: Metrics.gap8
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Metrics.gap12
                    height: width
                    radius: width / 2 // derived: circular status indicator
                    color: area.labelColor
                }
                Text {
                    objectName: area.labelObjectName
                    width: parent.width - Metrics.gap12 - Metrics.gap8
                    text: area.label
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    font.pointSize: Type.body
                    font.weight: Type.semibold
                    color: area.labelColor
                }
            }
            Text {
                id: message
                objectName: area.textObjectName
                width: parent.width
                text: area.text
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
        }
    }
    onTextChanged: viewport.contentY = 0
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: Metrics.progress
        visible: area.reserveProgress && Number.isFinite(area.progress) && area.progress >= 0
        radius: Metrics.radiusControl
        color: Theme.color.strokeHairline
        Rectangle {
            width: parent.width * (Number.isFinite(area.progress) ? Math.max(0,Math.min(1,area.progress)) : 0)
            height: parent.height
            radius: parent.radius
            color: Theme.color.accent
        }
    }
}

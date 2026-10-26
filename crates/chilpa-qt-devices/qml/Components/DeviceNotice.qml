import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: notice
    property var error: null
    property bool busy: false
    property bool details: false
    implicitHeight: Math.min(DeviceLayout.noticeHeight, body.implicitHeight + Metrics.gap12 * 2) // derived: bounded notice and vertical inset
    color: Theme.color.surfacePanel
    radius: Metrics.radiusControl
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    onErrorChanged: details = false

    Flickable {
        anchors.fill: parent
        anchors.margins: Metrics.gap12
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        Column {
            id: body
            width: parent.width
            spacing: Metrics.gap8
            Text {
                width: parent.width
                text: notice.error?.message || ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.color.statusNegative
                font.pointSize: Type.body
            }
            Flow {
                width: parent.width
                spacing: Metrics.gap8
                PillButton { visible: (notice.error?.detail || "") !== ""; kind: "ghost"; text: notice.details ? I18n.tr("Hide details", I18n.rev) : I18n.tr("Details", I18n.rev); onClicked: notice.details = !notice.details }
            }
            Flickable {
                visible: notice.details
                width: parent.width
                height: Math.min(DeviceLayout.detailHeight, detail.contentHeight)
                contentWidth: width
                contentHeight: detail.contentHeight
                clip: true
                TextEdit {
                    id: detail
                    width: parent.width
                    text: notice.error?.detail || ""
                    textFormat: TextEdit.PlainText
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    color: Theme.color.textSecondary
                    font.pointSize: Type.small
                }
            }
        }
    }
}

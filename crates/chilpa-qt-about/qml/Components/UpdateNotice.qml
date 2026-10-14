import QtQuick
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Item {
    id: notice
    readonly property var state: JSON.parse(Updates.snapshot || "{}")
    property string dismissed: ""
    readonly property string identity: (state.version || "") + ":" + (state.phase || "")
    width: Frame.updateWidth
    height: contents.height + Metrics.gap12 * 2 // derived: panel padding
    Timer { interval: Frame.updateDelay; running: true; onTriggered: Updates.check(true) }
    Timer { interval: Frame.updateInterval; repeat: true; running: true; onTriggered: Updates.check(true) }
    Timer { interval: Motion.poll; repeat: true; running: notice.state.supported === true; onTriggered: Updates.refresh() }
    Rectangle {
        anchors.fill: parent
        visible: notice.dismissed !== notice.identity && ["available", "downloading", "installing", "ready"].indexOf(notice.state.phase) >= 0
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised
        border.color: Theme.color.strokeHairline
        Row {
            id: contents
            x: Metrics.gap12; y: Metrics.gap12
            spacing: Metrics.gap8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                font.pointSize: Type.body
                color: Theme.color.textPrimary
                text: notice.state.phase === "ready" ? I18n.tr("Update ready", I18n.rev)
                    : notice.state.busy ? I18n.tr("Updating Chilpa…", I18n.rev) : I18n.tr("Chilpa update available", I18n.rev)
            }
            PillButton { text: I18n.tr("View", I18n.rev); onClicked: details.open() }
            IconButton { glyph: Icons.x; tip: I18n.tr("Dismiss", I18n.rev); onClicked: notice.dismissed = notice.identity }
        }
    }
    Modal {
        id: details
        width: Frame.updateWidth + Metrics.modalInsets
        UpdatesPanel { width: details.width - Metrics.modalInsets }
    }
    Component.onCompleted: Updates.refresh()
}

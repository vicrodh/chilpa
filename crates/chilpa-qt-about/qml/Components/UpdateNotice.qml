import QtQuick
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The bubble that comes out of the app menu when a version of Chilpa is on
// offer: which one, which one runs now, and a way to it. Its point sits at
// `caretX`, under the menu's button.
Item {
    id: notice
    property var state: JSON.parse(Updates.snapshot || "{}")
    property real caretX: width - Frame.noticeCaret * 2 // derived: near the right end when nothing points it
    signal viewAsked()
    signal dismissed()
    readonly property string available: (state.phase === "ready" ? I18n.tr("Chilpa {} is ready", I18n.rev)
        : state.busy === true ? I18n.tr("Updating Chilpa…", I18n.rev) : I18n.tr("Chilpa {} is available", I18n.rev)).replace("{}", state.version || "")
    implicitWidth: Frame.updateWidth
    implicitHeight: contents.height + Metrics.gap12 * 2 // derived: panel padding

    ElevatedShadow { target: panel; radius: panel.radius }
    // The point: a square turned on its corner, half of it under the panel.
    Rectangle {
        width: Frame.noticeCaret
        height: Frame.noticeCaret
        x: notice.caretX - width / 2 // derived: centered on the point
        y: -height / 2 // derived: its lower half under the panel
        rotation: 45 // derived: a square turned into a point
        color: Theme.color.surfaceRaised
        border.color: Theme.color.strokeHairline
    }
    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised
        border.color: Theme.color.strokeHairline
    }
    // Covers the point's border where it meets the panel.
    Rectangle {
        width: Frame.noticeCaret * 2 // derived: wider than the point's diagonal
        height: Frame.noticeCaret / 2 // derived: the point's lower half
        x: notice.caretX - width / 2 // derived: centered on the point
        y: 1
        color: Theme.color.surfaceRaised
    }
    Column {
        id: contents
        x: Metrics.gap12; y: Metrics.gap12
        width: notice.width - Metrics.gap12 * 2 // derived: panel padding
        spacing: Metrics.gap8
        Item {
            width: parent.width
            height: Math.max(texts.height, close.height)
            Column {
                id: texts
                width: parent.width - close.width - Metrics.gap8
                spacing: Metrics.gap4
                Text {
                    objectName: "noticeTitle"
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: notice.available
                    font.pointSize: Type.body
                    font.weight: Type.semibold
                    color: Theme.color.textPrimary
                }
                Text {
                    objectName: "noticeCurrent"
                    width: parent.width
                    visible: !!notice.state.current_version
                    text: I18n.tr("Current version: {}", I18n.rev).replace("{}", notice.state.current_version || "")
                    font.pointSize: Type.small
                    color: Theme.color.textSecondary
                }
            }
            IconButton {
                id: close
                objectName: "noticeDismiss"
                anchors.right: parent.right
                glyph: Icons.x
                tip: I18n.tr("Dismiss", I18n.rev)
                onClicked: notice.dismissed()
            }
        }
        PillButton { objectName: "noticeView"; text: I18n.tr("View", I18n.rev); kind: "primary"; onClicked: notice.viewAsked() }
    }
}

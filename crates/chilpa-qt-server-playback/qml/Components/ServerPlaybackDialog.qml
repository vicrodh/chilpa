import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.server.playback
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// What one server is told of what plays from it: that it plays, and the
// play counted once enough of it was heard. Each is turned off on its own;
// a server whose reports already count the play has no count apart, and
// says so. Every change is kept at once: closing only closes.
Modal {
    id: dialog
    property int serverId: -1
    // `ServerPlayback.settings`: the server's name and kind, what it can be
    // told, and what it is told.
    property var state: ({})
    signal changed()

    preferredWidth: Metrics.confirmWidth

    function show(id) {
        serverId = id
        reload()
        open()
    }
    function reload() { state = JSON.parse(ServerPlayback.settings(serverId) || "{}") }
    function keep(name, value) {
        if (ServerPlayback.change(serverId, name, String(value))) changed()
        reload()
    }
    // The slider runs from 0; a play counts from a tenth of the track on.
    function markOf(level) { return Math.max(10, Math.min(100, Math.round(level))) }

    Column {
        width: parent.width
        spacing: Metrics.gap16

        Column {
            width: parent.width
            spacing: Metrics.gap4
            Text {
                width: parent.width
                text: I18n.tr("Playback settings", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Text {
                objectName: "serverPlaybackName"
                width: parent.width
                text: dialog.state.name || ""
                textFormat: Text.PlainText
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
        }

        Item {
            width: parent.width
            height: Math.max(reportText.implicitHeight, reportToggle.height)
            SettingText {
                id: reportText
                anchors.left: parent.left
                anchors.right: reportToggle.left
                anchors.rightMargin: Metrics.gap16
                title: I18n.tr("Report playback to the server", I18n.rev)
                description: dialog.state.counts_apart === false
                    ? I18n.tr("Tells the server what plays and where. This server counts a play as soon as it hears that it started, and passes it on to the services it scrobbles to.", I18n.rev)
                    : I18n.tr("Tells the server what plays and where, without counting it.", I18n.rev)
            }
            Toggle {
                id: reportToggle
                objectName: "serverReportPlayback"
                anchors.right: parent.right
                anchors.top: parent.top
                checked: dialog.state.report !== false
                onToggled: on => dialog.keep("reportPlayback", on)
            }
        }

        Item {
            visible: dialog.state.counts_apart === true
            width: parent.width
            height: visible ? Math.max(countText.implicitHeight, countToggle.height) : 0
            SettingText {
                id: countText
                anchors.left: parent.left
                anchors.right: countToggle.left
                anchors.rightMargin: Metrics.gap16
                title: I18n.tr("Count plays on the server", I18n.rev)
                description: I18n.tr("Adds to its play count once enough of a track was heard. A server that scrobbles for you passes it on.", I18n.rev)
            }
            Toggle {
                id: countToggle
                objectName: "serverCountPlays"
                anchors.right: parent.right
                anchors.top: parent.top
                checked: dialog.state.count !== false
                onToggled: on => dialog.keep("countPlays", on)
            }
        }

        Column {
            visible: dialog.state.counts_apart === true && dialog.state.count !== false
            width: parent.width
            spacing: Metrics.gap8
            Item {
                width: parent.width
                height: markText.implicitHeight
                SettingText {
                    id: markText
                    anchors.left: parent.left
                    anchors.right: markValue.left
                    anchors.rightMargin: Metrics.gap16
                    title: I18n.tr("Counts as a play at", I18n.rev)
                    description: I18n.tr("How much of a track has to be heard before the server counts it. Your own history keeps its rule.", I18n.rev)
                }
                Text {
                    id: markValue
                    objectName: "serverPlayMarkValue"
                    anchors.right: parent.right
                    anchors.top: parent.top
                    text: String(mark.value) + "%"
                    font.pointSize: Type.body
                    font.features: { "tnum": 1 }
                    color: Theme.color.textPrimary
                }
            }
            LevelSlider {
                id: mark
                objectName: "serverPlayMark"
                width: parent.width
                value: dialog.state.mark || 50
                tip: I18n.tr("Counts as a play at", I18n.rev)
                onMoved: level => value = dialog.markOf(level)
                onReleased: level => dialog.keep("playMark", dialog.markOf(level))
            }
        }

        Row {
            anchors.right: parent.right
            PillButton {
                objectName: "serverPlaybackClose"
                text: I18n.tr("Close", I18n.rev)
                kind: "primary"
                onClicked: dialog.close()
            }
        }
    }
}

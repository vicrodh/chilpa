import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.controls
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
import "../Theme"

// The timer that pauses playback after a while. Off, it offers how long;
// running, it says what is left and how to call it off. Setting it again
// replaces the one running.
Modal {
    id: timer
    objectName: "sleepTimer"

    readonly property bool running: Controls.sleepLeft !== ""
    // The times offered, in minutes; after them, one typed by hand.
    readonly property var minutes: [30, 60, 120, 180, 300]
    readonly property var names: [I18n.tr("30 min", I18n.rev), I18n.tr("1 hr", I18n.rev), I18n.tr("2 hr", I18n.rev),
        I18n.tr("3 hr", I18n.rev), I18n.tr("5 hr", I18n.rev), I18n.tr("Custom…", I18n.rev)]
    property int chosen: 0
    readonly property bool custom: chosen === minutes.length

    function set() {
        const wanted = custom ? parseInt(typed.text) : minutes[chosen]
        if (wanted > 0)
            Controls.sleepIn(wanted)
        close()
    }

    onAboutToShow: Controls.refresh()

    Column {
        width: Metrics.sleepTimer
        spacing: Metrics.gap8

        Text {
            text: timer.running ? I18n.tr("Stops in", I18n.rev) : I18n.tr("Stop playback after:", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
        Text {
            objectName: "sleepLeft"
            visible: timer.running
            text: Controls.sleepLeft
            font.pointSize: Type.section
            font.weight: Type.semibold
            color: Theme.color.accent
        }
        PillButton {
            objectName: "sleepCancel"
            visible: timer.running
            text: I18n.tr("Cancel Timer", I18n.rev)
            onClicked: {
                Controls.sleepIn(0)
                timer.close()
            }
        }
        Repeater {
            model: timer.running ? [] : timer.names

            Rectangle {
                id: choice
                required property int index
                required property string modelData
                width: parent.width
                height: Metrics.rowAction
                radius: Metrics.radiusItem
                color: timer.chosen === index ? Theme.color.stateSelected : over.hovered ? Theme.color.stateHover : "transparent"

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    text: choice.modelData
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
                Icon {
                    visible: timer.chosen === choice.index
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.check
                    size: Metrics.iconSm
                    color: Theme.color.accent
                }
                HoverHandler {
                    id: over
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler { onTapped: timer.chosen = choice.index }
            }
        }
        Row {
            visible: !timer.running && timer.custom
            spacing: Metrics.gap8

            TextField {
                id: typed
                objectName: "sleepMinutes"
                width: Metrics.sleepTimer / 2 // derived: half of the popover
                text: "60"
                onAccepted: timer.set()
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("min", I18n.rev)
                font.pointSize: Type.meta
                color: Theme.color.textTertiary
            }
        }
        PillButton {
            objectName: "sleepSet"
            visible: !timer.running
            kind: "primary"
            text: I18n.tr("Set Timer", I18n.rev)
            onClicked: timer.set()
        }
    }
}

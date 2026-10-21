import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// One switch per source, the same in Library, Albums and Tracks: a source's
// switch on puts its saved releases and tracks in all three and shows them;
// off, it is in none. The library's own files and a server's are shown or
// kept out. A source turned off stays listed, so it can be turned on again.
Column {
    id: switches

    property bool active: false
    readonly property alias rows: source.rows
    function flip(row, on) { source.flip(row, on) }
    SourceRows { id: source; active: switches.active }

    spacing: Metrics.gap8

    Text {
        text: I18n.tr("Sources", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    Repeater {
        model: switches.rows

        Item {
            required property var modelData
            width: switches.width
            height: Metrics.sourceFilterRow

            SourceIcon {
                id: sourceIcon
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.iconNav
                height: Metrics.iconNav
                sourceInfo: ({ icon: modelData.icon, local: modelData.key === "local:default" })
            }
            Column {
                anchors.left: sourceIcon.right
                anchors.leftMargin: Metrics.gap10
                anchors.right: toggle.left
                anchors.rightMargin: Metrics.gap10
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    text: modelData.label
                    elide: Text.ElideRight
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
                Text {
                    visible: modelData.count >= 0
                    text: I18n.trCount("{} track", "{} tracks", modelData.count, I18n.rev)
                    font.pointSize: Type.micro
                    color: Theme.color.textTertiary
                }
            }
            Toggle {
                id: toggle
                objectName: "sourceSwitch-" + modelData.key
                compact: true
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: modelData.on
                onToggled: (on) => switches.flip(modelData, on)
            }
        }
    }
}

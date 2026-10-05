import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

Item {
    id: strip
    property var document: ({tabs: [], selected: ""})
    implicitHeight: Metrics.tabHeight
    Flickable {
        id: viewport
        anchors.left: parent.left
        width: Math.min(tabs.width, Math.max(0, strip.width - add.width - Metrics.gap4))
        height: parent.height
        contentWidth: tabs.width
        clip: true
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        Row {
            id: tabs
            spacing: Metrics.gap4
            Repeater {
                model: strip.document.tabs || []
                TabButton {
                    required property var modelData
                    title: modelData.name + " · " + modelData.count
                    glyph: modelData.playing ? Icons.waveSquare : Icons.listDetails
                    active: modelData.id === strip.document.selected
                    width: implicitWidth
                    height: implicitHeight
                    onActivated: ListenList.command("select", modelData.id, "")
                    onClosed: ListenList.command("close", modelData.id, "")
                }
            }
        }
    }
    IconButton {
        id: add
        objectName: "newQueue"
        anchors.left: viewport.right
        anchors.leftMargin: Metrics.gap4
        glyph: Icons.plus
        tip: I18n.tr("New queue", I18n.rev)
        onClicked: ListenList.command("new", "", "")
    }
}

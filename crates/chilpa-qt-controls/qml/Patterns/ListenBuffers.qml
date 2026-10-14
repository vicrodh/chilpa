import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
Item {
    id: strip
    property var document: ({tabs: [], selected: ""})
    function reveal() {
        for (let i = 0; i < buttons.count; i++) {
            const tab = buttons.itemAt(i)
            if (!tab || !tab.active) continue
            viewport.contentX = Math.max(0, Math.min(Math.max(0, viewport.contentWidth - viewport.width),
                tab.x < viewport.contentX ? tab.x : tab.x + tab.width > viewport.contentX + viewport.width ? tab.x + tab.width - viewport.width : viewport.contentX))
        }
    }
    onDocumentChanged: Qt.callLater(reveal)
    onWidthChanged: Qt.callLater(reveal)
    implicitHeight: Metrics.tabHeight + Metrics.gap8
    Flickable {
        id: viewport
        anchors.left: parent.left
        width: Math.min(tabs.width, Math.max(0, strip.width - add.width - Metrics.gap4))
        height: Metrics.tabHeight
        contentWidth: tabs.width
        clip: true
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ViewportScrollBar { gutter: Metrics.gap4 }
        Row {
            id: tabs
            spacing: Metrics.gap4
            Repeater {
                id: buttons
                model: strip.document.tabs || []
                TabButton {
                    required property var modelData
                    title: modelData.name + " · " + modelData.count
                    name: modelData.name
                    renamable: true
                    glyph: modelData.playing ? Icons.waveSquare : Icons.listDetails
                    active: modelData.id === strip.document.selected
                    width: implicitWidth
                    height: implicitHeight
                    onActivated: ListenList.command("select", modelData.id, "")
                    onClosed: ListenList.command("close", modelData.id, "")
                    onRenamed: (name) => ListenList.command("rename", modelData.id, name)
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

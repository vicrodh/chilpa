import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One of the columns a source's root is narrowed by, drawn as the Library's
// are: its name, a field that narrows its own entries, "All" first, then
// the entries with how much each has.
Rectangle {
    id: pane

    // { id, title, picked, entries: [{ id, title, subtitle }] }
    property var column: ({})

    signal picked(string id)

    readonly property var shown: {
        const needle = input.text.toLowerCase()
        const entries = (column.entries || []).filter(e => needle === "" || e.title.toLowerCase().includes(needle))
        return [{ key: "", label: I18n.tr("All", I18n.rev), meta: "", selected: (column.picked || "") === "", all: true }]
            .concat(entries.map(e => ({ key: e.id, label: e.title, meta: e.subtitle, selected: e.id === column.picked, all: false })))
    }

    color: Theme.color.surfacePane

    Rectangle {
        id: head
        width: parent.width
        height: Metrics.paneHeader
        color: Theme.color.surfacePane

        Icon {
            id: lens
            anchors.left: parent.left
            anchors.leftMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.search
            size: Metrics.iconXs
            color: input.activeFocus ? Theme.color.accent : Theme.color.textTertiary
        }
        TextInput {
            id: input
            anchors.left: lens.right
            anchors.leftMargin: Metrics.gap6
            anchors.right: parent.right
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            clip: true
            font.pointSize: Type.label
            color: Theme.color.textPrimary
            selectionColor: Theme.color.rowSelected

            Text {
                anchors.fill: parent
                visible: input.text === ""
                text: pane.column.title || ""
                font: input.font
                color: Theme.color.textSecondary
                elide: Text.ElideRight
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: input.activeFocus ? Metrics.focusRing : 1
            color: input.activeFocus ? Theme.color.accent : Theme.color.strokeHairline
        }
    }

    ListView {
        objectName: "sourceColumnList"
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        anchors.topMargin: Metrics.gap4
        width: parent.width
        clip: true
        model: pane.shown
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}

        delegate: PaneRow {
            required property var modelData
            width: ListView.view.width
            key: modelData.key
            label: modelData.label
            meta: modelData.meta
            selected: modelData.selected
            excluded: false
            canExclude: false
            all: modelData.all
            onChosen: pane.picked(modelData.key)
        }
    }
}

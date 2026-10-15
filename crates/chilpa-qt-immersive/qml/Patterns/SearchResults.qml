import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Tools
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The results under the header's search (03-immersive §1.6): a glass
// panel of 380 at y 58, with sections of artists, albums and playlists,
// rows of 56, the keyboard's row lit with a bar of the cover's accent,
// and nothing to open: Enter or a click acts on the queue the way the
// setting says, clears the field and stays here. A click anywhere else
// dismisses it. It shows from two characters, 220 ms after the last key,
// and never while the setting says disabled.
Item {
    id: root
    property string query: ""
    property bool shaders: true
    signal cleared()
    readonly property bool allowed: Immersive.searchAction !== "disabled"
    readonly property bool enough: query.trim().length >= Stage.searchMinChars
    property bool open: false
    property int current: -1
    readonly property string action: Immersive.searchAction === "replace" ? "play" : Immersive.searchAction

    visible: open && enough && allowed
    onEnoughChanged: if (!enough) { open = false; current = -1 }
    onQueryChanged: {
        current = -1
        if (enough && allowed)
            settle.restart()
        else
            search.text = ""
    }
    Timer {
        id: settle
        interval: Stage.searchDebounce
        onTriggered: {
            search.text = root.query.trim()
            root.open = true
        }
    }
    Timer { interval: Motion.poll; repeat: true; running: root.visible; onTriggered: search.refresh() }

    Tools.SearchNavigation { search: search }
    SearchModel {
        id: search
        scope: "immersive"
        Component.onCompleted: source = SettingsModel.kept("ui.search.sources") || "all"
        onModelReset: root.current = -1
    }
    readonly property int rows: search.count

    function dismiss() {
        open = false
        current = -1
    }
    // Over the headers, both ways, without wrapping: down from nothing
    // lands on the first row and up from the first returns to nothing.
    function step(by) {
        let row = current
        do
            row += by
        while (row >= 0 && row < list.count && list.itemAtIndex(row) && list.itemAtIndex(row).header)
        if (row >= list.count)
            return
        current = row < 0 ? -1 : row
        if (current >= 0)
            list.positionViewAtIndex(current, ListView.Contain)
    }
    function choose(row) {
        if (row < 0 || !root.allowed)
            return
        search.act(row, root.action)
        dismiss()
        root.cleared()
    }
    function chooseCurrent() {
        if (current < 0)
            step(1)
        if (current >= 0)
            choose(current)
    }

    readonly property real fieldWide: Math.min(root.width - Stage.edge * 2, Stage.searchWide)
    readonly property real fieldLeft: (root.width - fieldWide) / 2 // derived: centered
    readonly property real bandBottom: Stage.searchPanelTop - Stage.searchFieldApart
    MouseArea { visible: root.visible; x: 0; y: root.bandBottom; width: root.width; height: root.height - y; cursorShape: Qt.ArrowCursor; onClicked: root.dismiss() }
    MouseArea { visible: root.visible; x: 0; y: 0; width: root.fieldLeft; height: root.bandBottom; cursorShape: Qt.ArrowCursor; onClicked: root.dismiss() }
    MouseArea { visible: root.visible; x: root.fieldLeft + root.fieldWide; y: 0; width: root.width - x; height: root.bandBottom; cursorShape: Qt.ArrowCursor; onClicked: root.dismiss() }

    readonly property real panelTall: Math.min(list.contentHeight + Stage.menuRowPad, root.height - Stage.searchPanelTop - Stage.searchBottomLess)
    Rectangle {
        visible: root.visible
        x: (root.width - width) / 2 // derived: centered
        y: Stage.searchPanelTop + Stage.searchPanelShadowDrop
        width: Stage.searchPanelWide
        height: root.panelTall
        radius: Stage.searchPanelRound
        color: Theme.color.immersiveShadow
        layer.enabled: root.shaders
        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReach; blur: 0.5 }
    }
    Rectangle {
        id: panel
        visible: root.visible
        x: (root.width - width) / 2 // derived: centered
        y: Stage.searchPanelTop
        width: Stage.searchPanelWide
        height: root.panelTall
        radius: Stage.searchPanelRound
        color: Theme.color.immersiveSearchPanel
        border.width: Metrics.hairline
        border.color: Theme.color.immersiveSearchEdge
        clip: true

        ListView {
            id: list
            objectName: "immersiveResults"
            anchors.fill: parent
            anchors.topMargin: Stage.searchSectionPad
            anchors.bottomMargin: Stage.searchSectionPad
            anchors.leftMargin: Stage.searchFieldApart
            anchors.rightMargin: Stage.searchFieldApart
            model: search
            clip: true
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            boundsBehavior: Flickable.StopAtBounds
            delegate: Item {
                id: row
                required property int index
                required property int kind
                required property string title
                required property string detail
                required property url art
                readonly property bool header: kind === 0
                readonly property bool active: root.current === index
                width: list.width
                height: header ? Stage.searchHeadTall : Stage.searchRowTall
                Text {
                    visible: row.header
                    anchors.left: parent.left
                    anchors.leftMargin: Stage.menuRowLead
                    anchors.verticalCenter: parent.verticalCenter
                    text: row.title
                    color: Theme.color.immersiveTextGhost
                    font.pointSize: Stage.searchHeadText
                    font.weight: Type.semibold
                }
                Rectangle {
                    visible: !row.header
                    anchors.fill: parent
                    radius: Stage.searchRowRound
                    color: row.active ? Theme.color.immersiveRowActive : (area.containsMouse ? Theme.color.immersiveRowHover : "transparent")
                    Rectangle {
                        visible: row.active
                        y: Stage.searchSectionPad
                        width: Stage.searchActiveBar
                        height: parent.height - Stage.searchSectionPad * 2 // derived: inset top and bottom
                        radius: Stage.tinyRound
                        color: Ambient.accent
                    }
                    ArtTile {
                        x: Stage.menuRowLead
                        anchors.verticalCenter: parent.verticalCenter
                        width: Stage.searchThumb
                        height: Stage.searchThumb
                        radius: row.kind === 2 ? width / 2 : Stage.chipRound // derived: a circle for an artist
                        edged: false
                        source: row.art
                    }
                    Column {
                        x: Stage.menuRowLead + Stage.searchThumb + Stage.menuRowPad
                        width: parent.width - x - Stage.menuRowLead // derived: what is left of the row
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: Stage.menuRowApart * 2 // derived: a hair between the lines
                        Text { width: parent.width; text: row.title; color: Theme.color.immersiveText; font.pointSize: Stage.searchTitleText; font.weight: Type.medium; elide: Text.ElideRight }
                        Text { width: parent.width; text: row.detail; color: Theme.color.immersiveTextGhost; font.pointSize: Stage.searchSubText; elide: Text.ElideRight }
                    }
                    MouseArea {
                        id: area
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.choose(row.index)
                    }
                }
            }
            Text {
                visible: list.count === 0 && !search.running
                x: Stage.searchPad
                y: (Stage.searchHeadTall - height) / 2 // derived: centered in a header's height
                text: I18n.tr("No results for", I18n.rev) + " “" + root.query.trim() + "”"
                color: Theme.color.immersiveTextGhost
                font.pointSize: Stage.searchSubText
            }
        }
    }
}

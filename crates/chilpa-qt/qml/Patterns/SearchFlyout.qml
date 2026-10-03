import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../Theme"
import "../Primitives"
import "../Components"

// The search results, floating beside the search field (design/03
// SearchFlyout, design/04 10). Typing filters live and the chips narrow it
// to one kind. Esc or a click outside closes it. With the sidebar collapsed
// there is no field to type in, so it brings its own.
Popup {
    id: flyout

    required property SearchModel search
    property bool ownInput: false
    // The tallest it can be.
    property int room: 0
    // The row Up and Down move to, which Enter opens.
    property int current: -1
    // The sources a search asks, kept between runs: Local alone to begin with.
    readonly property var chosen: search.source.split(",").filter(s => s !== "")
    Component.onCompleted: search.source = SettingsModel.kept("ui.search.sources") || "all"
    function toggle(id) {
        let now = id === "all" ? (chosen.indexOf("all") >= 0 ? ["local"] : ["all"])
            : chosen.filter(s => s !== "all")
        if (id !== "all")
            now = now.indexOf(id) >= 0 ? now.filter(s => s !== id) : now.concat([id])
        search.source = (now.length > 0 ? now : ["local"]).join(",")
        SettingsModel.keep("ui.search.sources", search.source)
    }
    Timer { interval: Motion.poll; repeat: true; running: flyout.visible; onTriggered: flyout.search.refresh() }

    // Moves to the next result up or down, over the section labels.
    function step(by) {
        let row = current
        do
            row += by
        while (row >= 0 && row < list.count && list.itemAtIndex(row) && list.itemAtIndex(row).header)
        if (row >= 0 && row < list.count)
            current = row
        list.positionViewAtIndex(current, ListView.Contain)
    }
    function choose(row) {
        // The way to more of a source's results stays open, with them.
        const item = list.itemAtIndex(row)
        const link = item && item.link
        if (!link)
            close()
        // The rows are built again; the list stays where it was read.
        const at = list.contentY
        search.act(row, "open")
        if (link) {
            search.refresh()
            list.contentY = at
        }
    }
    // Enter: the row the keyboard is on, or the first result.
    function chooseCurrent() {
        if (current < 0)
            step(1)
        if (current >= 0)
            choose(current)
    }

    width: Metrics.searchFlyout
    padding: Metrics.gap8
    focus: ownInput
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent

    onOpened: if (ownInput) own.focusInput()
    onClosed: ownInput = false

    Connections {
        target: flyout.search
        function onModelReset() { flyout.current = -1 }
    }

    background: Item {
        Hold {}
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusPanel
            color: Theme.color.surfaceRaised

            Edge {}
        }
    }

    contentItem: Column {
        spacing: Metrics.gap8

        SearchField {
            id: own
            visible: flyout.ownInput
            width: parent.width
            placeholder: I18n.tr("Search", I18n.rev)
            text: flyout.search.text
            onTextChanged: flyout.search.text = text
            onStepped: (by) => flyout.step(by)
            onAccepted: flyout.chooseCurrent()
            onEscaped: flyout.close()
        }

        Flickable {
            width: parent.width
            height: Metrics.chipHeight
            contentWidth: sources.width
            clip: true
            Row {
                id: sources
                spacing: Metrics.gap6
                Repeater {
                    model: [{id: "all", name: I18n.tr("All sources", I18n.rev)}, {id: "local", name: I18n.tr("Local", I18n.rev)}].concat(JSON.parse(Sources.snapshot || "{}").panels || [])
                    Chip {
                        required property var modelData
                        text: modelData.name
                        active: flyout.chosen.indexOf(modelData.id) >= 0 || (modelData.id !== "all" && flyout.chosen.indexOf("all") >= 0)
                        onClicked: flyout.toggle(modelData.id)
                    }
                }
            }
        }
        Row {
            spacing: Metrics.gap6

            Repeater {
                model: [
                    { scope: "all", text: I18n.tr("All", I18n.rev) },
                    { scope: "artists", text: I18n.tr("Artists", I18n.rev) },
                    { scope: "albums", text: I18n.tr("Albums", I18n.rev) },
                    { scope: "songs", text: I18n.tr("Songs", I18n.rev) },
                    { scope: "playlists", text: I18n.tr("Playlists", I18n.rev) }
                ]

                Chip {
                    required property var modelData
                    text: modelData.text
                    active: flyout.search.scope === modelData.scope
                    onClicked: flyout.search.scope = modelData.scope
                }
            }
        }

        ListView {
            id: list
            width: parent.width
            height: Math.min(contentHeight, Math.max(0, flyout.room - list.y - flyout.padding * 2)) // derived: the room left under the chips
            interactive: contentHeight > height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: flyout.search

            delegate: SearchHit {
                required sourceIcon
                required sourceName
                width: list.width
                current: index === flyout.current
                onActivated: flyout.choose(index)
                onPlayed: {
                    flyout.close()
                    flyout.search.act(index, "play")
                }
                onMenu: (left, top) => {
                    const at = mapToItem(list, left, top)
                    hitMenu.show(index, at.x, at.y)
                }
            }

            SearchMenu {
                id: hitMenu
                search: flyout.search
                onLeft: flyout.close()
            }
        }

        Text {
            visible: flyout.search.count === 0 && flyout.search.text.trim() !== ""
            leftPadding: Metrics.gap8
            text: I18n.trArgs("Nothing found for “{}”", [flyout.search.text], I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
    }
}

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Tools
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The search results, floating beside the search field (design/03
// SearchFlyout, design/04 10). Typing filters live and the chips narrow it
// to one kind. Esc or a click outside closes it. With the sidebar collapsed
// there is no field to type in, so it brings its own.
Popup {
    id: flyout

    Tools.SearchNavigation { id: searchNavigation; search: flyout.search }
    required property SearchModel search
    property bool ownInput: false
    // The tallest it can be.
    property int room: 0
    // The row Up and Down move to, which Enter opens.
    property int current: -1
    // Everything found, on the results page.
    signal allAsked()
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
        searchNavigation.flush()
        if (link) {
            search.refresh()
            list.contentY = at
        }
    }
    // Enter: the row the keyboard is on, or everything on the results page, as QBZ.
    function chooseCurrent() {
        if (current >= 0)
            choose(current)
        else
            allAsked()
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

        Tools.SearchFilters { width: parent.width; search: flyout.search }
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
            height: Math.min(contentHeight, Math.max(0, flyout.room - list.y - flyout.padding * 2 - footer.height - Metrics.gap8)) // derived: the room left under the chips and over the footer
            interactive: contentHeight > height
            clip: true; ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            boundsBehavior: Flickable.StopAtBounds
            model: flyout.search

            delegate: SearchHit {
                required sourceIcon
                required sourceName
                markOwn: true
                width: list.width
                current: index === flyout.current
                onActivated: flyout.choose(index)
                onPlayed: {
                    flyout.close()
                    flyout.search.act(index, "play")
                    searchNavigation.flush()
                }
                onMenu: (left, top) => {
                    const at = mapToItem(list, left, top)
                    hitMenu.show(index, at.x, at.y)
                }
            }

            Tools.SearchMenu {
                id: hitMenu
                navigation: searchNavigation
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
        Item {
            id: footer
            width: parent.width; height: Metrics.button32
            Tools.SearchPreviewSwitch { anchors.verticalCenter: parent.verticalCenter; search: flyout.search }
            PillButton {
                objectName: "searchAllResults"
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("All results", I18n.rev)
                onClicked: flyout.allAsked()
            }
        }
    }
}

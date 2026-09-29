import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import com.blitzfc.chilpa
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
        close()
        search.activate(row)
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

        Row {
            spacing: Metrics.gap6

            Repeater {
                model: [
                    { scope: "all", text: I18n.tr("All", I18n.rev) },
                    { scope: "artists", text: I18n.tr("Artists", I18n.rev) },
                    { scope: "albums", text: I18n.tr("Albums", I18n.rev) },
                    { scope: "songs", text: I18n.tr("Songs", I18n.rev) }
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
                width: list.width
                current: index === flyout.current
                onActivated: flyout.choose(index)
                onPlayed: {
                    flyout.close()
                    flyout.search.play(index)
                }
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

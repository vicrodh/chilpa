import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"

// The one track list (frontend ADR-0006): configured by properties, never
// forked. Rows are recycled; hover and clicks are resolved here, once for
// the whole list, so a row carries no mouse handling of its own.
Item {
    id: list

    property TrackListModel model
    property bool rich: SettingsModel.density === "rich"
    property bool showArtist: true
    property bool showAlbum: true
    property bool showArt: true
    property bool showPlays: false
    property bool header: true
    // Grow with the rows instead of scrolling: a list inside a page that
    // scrolls as a whole.
    property bool fitContent: false
    // Room below the last row, for what floats over the list.
    property int bottomInset: 0

    readonly property alias contentHeight: view.contentHeight

    implicitHeight: (header ? Metrics.rowHeader : 0) + view.contentHeight

    TrackColumns {
        id: columns
        width: list.width
        rich: list.rich
        showArtist: list.showArtist
        showAlbum: list.showAlbum
        showArt: list.showArt
        showPlays: list.showPlays
    }

    TrackHeader {
        id: head
        visible: list.header
        height: list.header ? Metrics.rowHeader : 0
        cols: columns
    }

    ListView {
        id: view

        property int hovered: -1

        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        interactive: !list.fitContent
        clip: !list.fitContent
        model: list.model
        reuseItems: true
        // A list that grows with its rows builds them all; one that scrolls
        // keeps a screen of them ready on each side.
        cacheBuffer: list.fitContent ? Metrics.fitContentBuffer : Math.max(0, height)
        bottomMargin: list.bottomInset
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: list.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded }

        delegate: TrackRow {
            cols: columns
            hovered: index === view.hovered
        }

        onContentYChanged: pointer.track()
    }

    MouseArea {
        id: pointer

        anchors.fill: view
        anchors.rightMargin: list.fitContent ? 0 : Metrics.gap10
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton

        function rowAt(y) {
            return view.indexAt(Metrics.gap2, y + view.contentY)
        }
        function track() {
            view.hovered = containsMouse ? rowAt(mouseY) : -1
        }

        onPositionChanged: track()
        onContainsMouseChanged: track()
        onClicked: (mouse) => {
            const row = rowAt(mouse.y)
            if (row < 0)
                return
            const toggle = (mouse.modifiers & Qt.ControlModifier) || (mouse.modifiers & Qt.MetaModifier)
            list.model.select(row, (toggle ? 1 : 0) | (mouse.modifiers & Qt.ShiftModifier ? 2 : 0))
        }
        onDoubleClicked: (mouse) => {
            const row = rowAt(mouse.y)
            if (row >= 0)
                list.model.play(row)
        }
    }
}

import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"

// Tracks: every track of the library in one list, found by what is typed,
// in an order, and grouped or not. Grouped, its headers say the record, the
// artist or the letter, and a strip of letters jumps among them.
Item {
    id: page

    property string order: SettingsModel.kept("ui.tracks.order") || "default"
    property string group: SettingsModel.kept("ui.tracks.group") || "off"
    property string typed: Visit.recall("tracks.typed", "")
    // Where the list was when the screen was left, until it is back there.
    property real returnTo: Visit.recall("tracks.at", 0)

    Component.onDestruction: {
        Visit.keep("tracks.typed", typed)
        Visit.keep("tracks.at", list.rows.contentY)
    }
    property bool selectionMode: false
    readonly property bool grouped: group !== "off"
    // Where each letter starts, read again each time the list arrives.
    property var letters: []

    TrackListModel {
        id: tracks
        kind: "tracks"
        key: [page.order === "default" ? "" : page.order, page.group, page.typed].join("\u001f")
        filterRev: Library.rev + Library.contentRev
        playingPath: Player.path
    }

    // Escape lets the selection go; Ctrl+A takes every track.
    Shortcut {
        enabled: page.selectionMode
        sequence: "Escape"
        onActivated: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }
    Shortcut {
        sequence: StandardKey.SelectAll
        onActivated: {
            page.selectionMode = true
            tracks.selectAll()
        }
    }

    // What is typed is asked for once the typing stops.
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: page.typed = find.text.trim()
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Tracks", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        // How many there are, in the room the title and the tools leave.
        Text {
            anchors.left: title.right
            anchors.leftMargin: Metrics.gap12
            anchors.right: tools.left
            anchors.rightMargin: Metrics.gap12
            anchors.baseline: title.baseline
            text: tracks.summary
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Row {
            id: tools
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap12

            SearchField {
                id: find
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.screenSearch
                placeholder: I18n.tr("Search", I18n.rev)
                text: page.typed
                onTextChanged: if (text.trim() !== page.typed) settle.restart()
                onEscaped: text = ""
            }
            SortButton {
                anchors.verticalCenter: parent.verticalCenter
                value: page.order
                options: [
                    { value: "default", text: I18n.tr("Default", I18n.rev) },
                    { value: "artist", text: I18n.tr("Artist A-Z", I18n.rev) },
                    { value: "artist-desc", text: I18n.tr("Artist Z-A", I18n.rev) },
                    { value: "title", text: I18n.tr("Title A-Z", I18n.rev) },
                    { value: "title-desc", text: I18n.tr("Title Z-A", I18n.rev) },
                    { value: "year-desc", text: I18n.tr("Year (newest)", I18n.rev) },
                    { value: "year", text: I18n.tr("Year (oldest)", I18n.rev) },
                    { value: "added", text: I18n.tr("Date added", I18n.rev) },
                    { value: "duration", text: I18n.tr("Duration (shortest)", I18n.rev) },
                    { value: "duration-desc", text: I18n.tr("Duration (longest)", I18n.rev) }
                ]
                onPicked: (value) => {
                    page.order = value
                    SettingsModel.keep("ui.tracks.order", value)
                }
            }
            SortButton {
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.stack2
                value: page.group
                options: [
                    { value: "off", text: I18n.tr("No grouping", I18n.rev) },
                    { value: "album", text: I18n.tr("By album", I18n.rev) },
                    { value: "artist", text: I18n.tr("By artist", I18n.rev) },
                    { value: "name", text: I18n.tr("By name", I18n.rev) }
                ]
                onPicked: (value) => {
                    page.group = value
                    SettingsModel.keep("ui.tracks.group", value)
                }
            }
            IconButton {
                id: filterButton
                anchors.verticalCenter: parent.verticalCenter
                tip: I18n.tr("Quality, format and source filters", I18n.rev)
                glyph: Icons.filter
                round: true
                checked: Library.extraFiltered || filters.opened
                onClicked: filters.opened ? filters.close() : filters.open()

                SourcesPopover {
                    id: filters
                    x: filterButton.width - width
                    y: filterButton.height + Metrics.gap6
                }
            }
            IconButton {
                anchors.verticalCenter: parent.verticalCenter
                tip: I18n.tr("Select tracks", I18n.rev)
                glyph: Icons.checkbox
                round: true
                checked: page.selectionMode
                onClicked: {
                    page.selectionMode = !page.selectionMode
                    tracks.clearSelection()
                }
            }
        }
    }

    TrackList {
        id: list
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width - (strip.visible ? strip.width + Metrics.gap8 : 0)
        bottomInset: Metrics.contentBottom
        header: false
        rich: true
        showAlbum: !page.grouped || page.group !== "album"
        selectionMode: page.selectionMode
        model: tracks
        Component.onCompleted: rows.section.delegate = band
    }
    Connections {
        target: tracks
        function onModelReset() {
            page.letters = JSON.parse(tracks.letters(0) || "[]")
        }
        function onCountChanged() {
            if (page.returnTo > 0 && tracks.count > 0) {
                list.rows.contentY = Math.min(page.returnTo, Math.max(0, list.rows.contentHeight - list.rows.height))
                page.returnTo = 0
            }
        }
    }
    Binding {
        target: list.rows.section
        property: "property"
        value: page.grouped ? "group" : ""
    }
    Component {
        id: band
        Text {
            required property string section
            width: list.width
            height: Metrics.rowGroup
            leftPadding: Metrics.gap2
            text: section
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.textSecondary
        }
    }

    AlphaStrip {
        id: strip
        visible: page.grouped && tracks.count > 0
        anchors.top: list.top
        anchors.bottom: list.bottom
        anchors.bottomMargin: Metrics.contentBottom
        anchors.right: head.right
        letters: page.letters
        onJumped: (row) => list.rows.positionViewAtIndex(row, ListView.Beginning)
    }

    Text {
        visible: tracks.count === 0
        anchors.centerIn: list
        text: page.typed !== "" ? I18n.tr("No tracks match your search.", I18n.rev) : I18n.tr("No tracks in your local library yet.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }

    PlaylistSelectionBar {
        visible: page.selectionMode && tracks.selectedCount > 0
        z: 2
        model: tracks
        playlistName: I18n.tr("Tracks", I18n.rev)
        canRemove: false
        x: (page.width - width) / 2
        y: page.height - height - Metrics.selectionBottom
        onFinished: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }
}

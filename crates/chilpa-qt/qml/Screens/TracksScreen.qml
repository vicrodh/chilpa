import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Tracks: every track of the library in one list, found by what is typed,
// in an order, and grouped or not. Grouped, its headers say the record, the
// artist or the letter, and a strip of letters jumps among them.
Item {
    id: page

    property string order: SettingsModel.kept("ui.tracks.order") || "default"
    property string group: SettingsModel.kept("ui.tracks.group") || "off"
    property string typed: Visit.recall("tracks.typed", "")
    // A track's own stars to keep, 0 for none; empty keeps them all.
    property var ratings: Visit.recall("tracks.ratings", [])
    readonly property var scrollView: list.rows

    Component.onDestruction: {
        Visit.keep("tracks.typed", typed)
        Visit.keep("tracks.ratings", ratings)
    }
    property bool selectionMode: false
    readonly property bool grouped: group !== "off"
    // Where each letter starts, read again each time the list arrives.
    property var letters: []

    TrackListModel {
        id: tracks
        kind: "tracks"
        key: [page.order === "default" ? "" : page.order, page.group, page.typed, page.ratings.join(",")].join("\u001f")
        filterRev: Library.rev + Library.contentRev + Library.remoteRev
        playingPath: Player.path
    }

    // Escape lets the selection go; Ctrl+A takes every track.
    Shortcut {
        enabled: !InputLayers.blocksShortcuts(page.Window.window) && (page.selectionMode)
        sequence: "Escape"
        onActivated: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }
    Shortcut {
        enabled: !InputLayers.blocksShortcuts(page.Window.window)
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
                    { value: "default", text: I18n.tr("Default", I18n.rev), fixed: true },
                    { value: "artist", text: I18n.tr("Artist", I18n.rev) },
                    { value: "title", text: I18n.tr("Title", I18n.rev) },
                    { value: "album", text: I18n.tr("Album", I18n.rev) },
                    { value: "year", text: I18n.tr("Year", I18n.rev) },
                    { value: "added", text: I18n.tr("Date added", I18n.rev), descending: true },
                    { value: "duration", text: I18n.tr("Duration", I18n.rev) }
                ]
                onPicked: (value) => {
                    page.order = value
                    SettingsModel.keep("ui.tracks.order", value)
                }
            }
            SortButton {
                objectName: "tracksGrouping"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.stack2
                directed: false
                value: page.group
                options: [
                    { value: "off", text: I18n.tr("No grouping", I18n.rev) },
                    { value: "album", text: I18n.tr("By album", I18n.rev) },
                    { value: "artist", text: I18n.tr("By artist", I18n.rev) },
                    { value: "name", text: I18n.tr("By name", I18n.rev) },
                    { value: "year", text: I18n.tr("By year", I18n.rev) },
                    { value: "decade", text: I18n.tr("By decade", I18n.rev) },
                    { value: "genre", text: I18n.tr("By genre", I18n.rev) },
                    { value: "label", text: I18n.tr("By label", I18n.rev) },
                    { value: "composer", text: I18n.tr("By composer", I18n.rev) },
                    { value: "plays", text: I18n.tr("By plays", I18n.rev) }
                ]
                onPicked: (value) => {
                    page.group = value
                    SettingsModel.keep("ui.tracks.group", value)
                }
            }
            IconButton {
                id: filterButton
                anchors.verticalCenter: parent.verticalCenter
                tip: I18n.tr("Quality, format, source and rating filters", I18n.rev)
                glyph: Icons.filter
                round: true
                checked: Library.extraFiltered || filters.opened || page.ratings.length > 0
                onClicked: filters.opened ? filters.close() : filters.open()

                SourcesPopover {
                    id: filters
                    showAlbumGrouping: false
                    ratings: page.ratings
                    onRatingsChosen: (chosen) => page.ratings = chosen
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

    // The group the first row shown is in, above the rows: bands between
    // the rows made the view misjudge far rows, and the first scroll after
    // a far jump leapt hundreds of rows. Each group's first row carries a
    // divider instead.
    readonly property string currentGroup: grouped ? list.rows.currentSection : ""
    Text {
        id: groupName
        objectName: "tracksGroupName"
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        x: head.x
        width: list.width
        height: page.grouped ? Metrics.rowGroup : 0
        visible: page.grouped
        leftPadding: Metrics.gap2
        text: page.currentGroup
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        font.pointSize: Type.meta
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }

    TrackList {
        id: list
        anchors.top: groupName.bottom
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width - (strip.visible ? strip.width + Metrics.gap8 : 0)
        // The letters take the margin's place beside the rows.
        scrollGutter: strip.visible ? Metrics.scrollbarWidth / 2 : Metrics.pageGutter
        bottomInset: Metrics.contentBottom
        header: false
        rich: SettingsModel.density === "rich"
        showAlbum: !page.grouped || page.group !== "album"
        selectionMode: page.selectionMode
        model: tracks
    }
    Connections {
        target: tracks
        function onModelReset() {
            page.letters = JSON.parse(tracks.letters(0) || "[]")
        }
    }
    Binding {
        target: list.rows.section
        property: "property"
        value: page.grouped ? "group" : ""
    }

    AlphaStrip {
        id: strip
        objectName: "tracksLetters"
        visible: page.grouped && tracks.count > 0 && page.letters.length > 0
        anchors.top: list.top
        anchors.bottom: list.bottom
        anchors.bottomMargin: Metrics.contentBottom
        anchors.right: head.right
        letters: page.letters
        onJumped: (row) => list.jumpTo(row)
    }

    Text {
        visible: tracks.count === 0
        anchors.centerIn: list
        text: page.typed !== "" ? I18n.tr("No tracks match your search.", I18n.rev) : I18n.tr("No tracks in the selected libraries yet.", I18n.rev)
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

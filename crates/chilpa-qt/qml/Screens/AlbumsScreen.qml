import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Components"
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"

// Albums (design/06): every record in the library, as cards or as rows,
// through the sources and qualities chosen, and grouped or not. It scrolls
// over a snapshot of ids and reads its records a page at a time, so ten
// thousand scroll like ten. With `source`, a source's listing on the same grid.
Item {
    id: page

    // "<plugin> <address>" of a source's listing, or empty for the library.
    property string source
    property string membership
    readonly property var about: source ? JSON.parse(Sources.page(source, Library.remoteRev) || "{}") : ({})
    property string sourceOrder: SettingsModel.kept("ui.source.order")
    property bool rows: SettingsModel.kept("ui.albums.view") === "list"
    property string group: SettingsModel.kept("ui.albums.group")

    AlbumGridModel {
        id: all
        kind: page.source ? "source" : "all"
        key: page.source ? page.source + (page.membership ? " " + page.membership : "") : ""
        order: page.source ? page.sourceOrder : Library.albumOrder
        group: page.group
        filter: find.text
        contentRev: Library.contentRev + Library.rev + (page.source ? Library.remoteRev + Library.remoteArtRev : 0)
        columns: grid.columns
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
            text: page.source ? page.about.title || page.about.name || "" : I18n.tr("Albums", I18n.rev)
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
            text: page.about.entity === "playlist" ? I18n.trCount("{} playlist", "{} playlists", all.albums, I18n.rev)
                : page.about.entity === "artist" ? I18n.trCount("{} artist", "{} artists", all.albums, I18n.rev) : I18n.trCount("{} album", "{} albums", all.albums, I18n.rev)
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
                // What the title and the other tools leave, and no less than it takes to type in.
                width: Math.max(Metrics.folderTreeMin, Math.min(Metrics.screenSearch, head.width - title.width - sort.width - grouping.width - filterButton.width - views.width - Metrics.folderTreeMin))
                placeholder: I18n.tr("Filter albums", I18n.rev)
                onEscaped: text = ""
            }
            SortButton {
                id: sort
                anchors.verticalCenter: parent.verticalCenter
                value: page.source ? page.sourceOrder : Library.albumOrder
                options: page.source ? [
                    { value: "", text: I18n.tr("As the source lists them", I18n.rev) },
                    { value: "artist", text: I18n.tr("Artist", I18n.rev) },
                    { value: "title", text: I18n.tr("Title", I18n.rev) },
                    { value: "year", text: I18n.tr("Year", I18n.rev) }
                ] : [
                    { value: "artist", text: I18n.tr("Artist", I18n.rev) },
                    { value: "title", text: I18n.tr("Title", I18n.rev) },
                    { value: "year", text: I18n.tr("Year", I18n.rev) },
                    { value: "added", text: I18n.tr("Date added", I18n.rev) },
                    { value: "played", text: I18n.tr("Most played", I18n.rev) },
                    { value: "last_played", text: I18n.tr("Recently played", I18n.rev) }
                ]
                onPicked: (value) => {
                    if (!page.source) { Library.albumOrder = value; return }
                    page.sourceOrder = value
                    SettingsModel.keep("ui.source.order", value)
                }
            }
            SortButton {
                id: grouping
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.stack2
                value: page.group
                options: [
                    { value: "", text: I18n.tr("No grouping", I18n.rev) },
                    { value: "letter", text: I18n.tr("By letter", I18n.rev) },
                    { value: "artist", text: I18n.tr("By artist", I18n.rev) }
                ]
                onPicked: (value) => {
                    page.group = value
                    SettingsModel.keep("ui.albums.group", value)
                }
            }
            IconButton {
                id: filterButton
                visible: !page.source
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
            SegmentedControl {
                id: views
                anchors.verticalCenter: parent.verticalCenter
                options: [
                    { glyph: Icons.list, tip: I18n.tr("List", I18n.rev) },
                    { glyph: Icons.layoutGrid, tip: I18n.tr("Grid", I18n.rev) }
                ]
                current: page.rows ? 0 : 1
                onActivated: (index) => {
                    page.rows = index === 0
                    SettingsModel.keep("ui.albums.view", page.rows ? "list" : "grid")
                }
            }
        }
    }

    // What the source's records are to the listener: all, or one of them.
    Row {
        id: kinds
        anchors.top: head.bottom
        x: head.x
        spacing: Metrics.gap8
        visible: (page.about.memberships || []).length > 1
        height: visible ? implicitHeight : 0

        Repeater {
            model: [""].concat(page.about.memberships || [])
            PillButton {
                required property string modelData
                kind: page.membership === modelData ? "primary" : "secondary"
                text: ({ "": I18n.tr("All", I18n.rev), favorites: I18n.tr("Favorites", I18n.rev), purchases: I18n.tr("Purchases", I18n.rev) })[modelData] || modelData
                onClicked: page.membership = modelData
            }
        }
    }

    AlbumGrid {
        id: grid
        anchors.top: kinds.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        bottomInset: Metrics.contentBottom
        list: page.rows
        round: page.about.round === true
        model: all
        onOpen: (albumId) => Navigation.go("album", albumId)
    }

    SourceState {
        anchors.centerIn: grid
        visible: page.source !== "" && all.albums === 0
        source: page.source
        about: page.about
    }
}

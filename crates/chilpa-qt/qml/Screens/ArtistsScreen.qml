import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Artists (design/06): who the records are credited to, or everyone who
// plays on a track, through the sources Albums filters by, as Artist
// Scene's cards with the icon of each source that holds their records.
Item {
    id: page

    property string role: SettingsModel.kept("ui.artists.role") === "all" ? "all" : "album"

    AlbumGridModel {
        id: everyone
        kind: "artists"
        key: page.role
        contentRev: Library.contentRev + Library.rev + Library.remoteRev
        columns: grid.columns
        filter: find.text
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
            text: I18n.tr("Artists", I18n.rev)
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
            text: I18n.trCount("{} artist", "{} artists", everyone.albums, I18n.rev)
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
                // What the title and the roles leave, and no less than it takes to type in.
                width: Math.max(Metrics.folderTreeMin, Math.min(Metrics.screenSearch, head.width - title.width - roles.width - filterButton.width - Metrics.folderTreeMin))
                placeholder: I18n.tr("Filter artists", I18n.rev)
                onEscaped: text = ""
            }
            SegmentedControl {
                id: roles
                anchors.verticalCenter: parent.verticalCenter
                options: [
                    { text: I18n.tr("Album artists", I18n.rev), tip: I18n.tr("Who the records are credited to", I18n.rev) },
                    { text: I18n.tr("All artists", I18n.rev), tip: I18n.tr("Everyone who plays on a track", I18n.rev) }
                ]
                current: page.role === "all" ? 1 : 0
                onActivated: (index) => {
                    page.role = index === 1 ? "all" : "album"
                    SettingsModel.keep("ui.artists.role", page.role)
                }
            }
            IconButton {
                id: filterButton
                objectName: "artistsFilters"
                anchors.verticalCenter: parent.verticalCenter
                tip: I18n.tr("Sources", I18n.rev)
                glyph: Icons.filter
                round: true
                checked: Library.excludedSources.length > 0 || filters.opened
                onClicked: filters.opened ? filters.close() : filters.open()
                SourcesPopover { id: filters; showAlbumGrouping: false; sourcesOnly: true; x: filterButton.width - width; y: filterButton.height + Metrics.gap6 }
            }
        }
    }

    ArtistCardGrid {
        id: grid
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        bottomMargin: Metrics.contentBottom
        artists: everyone
    }
}

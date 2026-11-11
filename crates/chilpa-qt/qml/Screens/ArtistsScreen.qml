import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

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

    ScreenHead {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        title: I18n.tr("Artists", I18n.rev)
        count: I18n.trCount("{} artist", "{} artists", everyone.albums, I18n.rev)
        flexible: find

        SearchField {
            id: find
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, Math.min(Metrics.screenSearch, head.flexibleRoom))
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

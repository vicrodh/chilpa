import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/Viewport.js" as Viewport
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"

Item {
    id: overview
    property string key: ""
    property string name: ""
    property var about: ({})
    property bool loading: false
    property Component artistsContent
    readonly property bool ready: !loading && layoutReady
    readonly property alias scrollView: view
    readonly property var summary: about.summary || ({})
    readonly property var subgenres: (about.items || []).slice(0, Metrics.discoveryPreview)
    property bool layoutReady: false
    function settleLayout() { subgenreGrid.forceLayout(); body.forceLayout(); layoutReady = true }
    Component.onCompleted: Qt.callLater(settleLayout)
    onAboutChanged: { layoutReady = false; Qt.callLater(settleLayout) }
    onWidthChanged: { layoutReady = false; Qt.callLater(settleLayout) }
    readonly property real albumShelfHeight: CardLayout.cardWidth + Metrics.cardTextHeight + Metrics.cardTextGap + Metrics.cardRing * 2 + Metrics.gap8 // derived: one card row and its focus rings
    signal activated(var entry)
    signal browseRequested(string section)
    signal playRequested()
    function inView(item) {
        const top = Viewport.topIn(item, view.contentItem)
        return layoutReady && visible && top + item.height > view.contentY && top < view.contentY + view.height
    }

    Flickable {
        id: view
        objectName: "genreOverviewScroll"
        anchors.fill: parent; contentWidth: width; contentHeight: body.height
        clip: true; boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        Column {
            id: body
            width: view.width
            spacing: Metrics.gap24
            Item {
                id: hero
                objectName: "genreHero"
                width: parent.width; height: Metrics.discoveryHeroHeight
                DiscoveryMosaic { anchors.fill: parent; images: overview.summary.images || []; label: overview.name; activeImages: overview.inView(hero) }
                Rectangle {
                    anchors.fill: parent; radius: Metrics.radiusCard
                    gradient: Gradient {
                        GradientStop { position: 0; color: "transparent" }
                        GradientStop { position: 1; color: Theme.color.overlayScrim }
                    }
                }
                Column {
                    anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: Metrics.gap24
                    spacing: Metrics.gap12
                    Text { width: parent.width; text: overview.name; textFormat: Text.PlainText; wrapMode: Text.WrapAtWordBoundaryOrAnywhere; maximumLineCount: 2; elide: Text.ElideRight; font.pointSize: Type.title; font.weight: Type.semibold; color: Theme.color.backdropText }
                    Text { width: parent.width; text: I18n.trCount("{} album in your library", "{} albums in your library", overview.summary.albums || 0, I18n.rev); font.pointSize: Type.body; color: Theme.color.backdropTextMuted }
                    Flow {
                        width: parent.width; spacing: Metrics.gap8
                        PillButton { objectName: "genrePlay"; text: I18n.tr("Play", I18n.rev); kind: "primary"; enabled: (overview.summary.albums || 0) + (overview.summary.looseTracks || 0) > 0; onClicked: overview.playRequested() }
                        PillButton { text: I18n.tr("Tracks", I18n.rev); kind: "backdrop"; onClicked: overview.browseRequested("tracks") }
                        PillButton { text: I18n.tr("Subgenres", I18n.rev); kind: "backdrop"; visible: (overview.about.total || 0) > 0; onClicked: overview.browseRequested("subgenres") }
                    }
                }
            }
            Text {
                width: parent.width; visible: text !== ""
                text: overview.about.description || ""; textFormat: Text.PlainText; wrapMode: Text.WordWrap
                font.pointSize: Type.body; color: Theme.color.textSecondary
            }
            Row {
                width: parent.width; spacing: Metrics.gap12
                Text { width: Math.max(0, parent.width - allAlbums.width - parent.spacing); anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Albums in your library", I18n.rev); elide: Text.ElideRight; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
                PillButton { id: allAlbums; objectName:"genreAllAlbums"; text: I18n.tr("See all", I18n.rev); kind: "ghost"; onClicked: overview.browseRequested("albums") }
            }
            Loader {
                id: albums
                objectName: "genreAlbumShelf"
                width: parent.width; height: overview.albumShelfHeight
                active: overview.inView(albums)
                sourceComponent: Item {
                    AlbumGridModel { id: records; kind:"discovery"; key:overview.key; maxRows:1; columns:grid.columns; contentRev:Library.contentRev + Library.remoteRev }
                    AlbumGrid { id:grid; anchors.fill:parent; model:records; onOpen: albumId => Navigation.go("album", albumId) }
                    Text { anchors.centerIn:parent; visible:records.ready && records.albums === 0; text:I18n.tr("No albums in this selection", I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.body }
                }
            }
            Row {
                width: parent.width; spacing: Metrics.gap12
                Text { width: Math.max(0, parent.width - allArtists.width - parent.spacing); anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Artists in your library", I18n.rev); elide: Text.ElideRight; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
                PillButton { id: allArtists; text: I18n.tr("See all", I18n.rev); kind: "ghost"; onClicked: overview.browseRequested("artists") }
            }
            Loader {
                id: artists
                objectName: "genreArtistShelf"
                width: parent.width; height: Metrics.sceneCardHeight + Metrics.sceneRowGap
                active: overview.inView(artists)
                sourceComponent: overview.artistsContent
            }
            Row {
                width: parent.width; spacing: Metrics.gap12; visible: overview.subgenres.length > 0
                Text { width: Math.max(0, parent.width - allSubgenres.width - parent.spacing); anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Subgenres", I18n.rev); elide: Text.ElideRight; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
                PillButton { id:allSubgenres; text:I18n.tr("See all", I18n.rev); kind:"ghost"; onClicked:overview.browseRequested("subgenres") }
            }
            Flow {
                id: subgenreGrid
                width: parent.width; spacing: Metrics.gap12
                visible: overview.subgenres.length > 0
                readonly property int columns: Math.max(1, Math.floor((width + spacing) / Metrics.discoveryEntryWidth))
                Repeater {
                    model: overview.subgenres
                    DiscoveryEntry {
                        id: tile
                        required property var modelData
                        width: (parent.width - parent.spacing * (parent.columns - 1)) / parent.columns
                        height: Metrics.discoveryGenreHeight
                        entry: modelData; detail:I18n.trCount("{} album", "{} albums", modelData.albums || 0, I18n.rev)
                        activeImages: overview.inView(tile)
                        onClicked: overview.activated(modelData)
                    }
                }
            }
            Text {
                width: parent.width; visible: (overview.about.resolutions || []).length > 0
                text: I18n.tr("Your genre tags", I18n.rev) + ": " + (overview.about.resolutions || []).map(record => record.original).join(" · ")
                textFormat:Text.PlainText; wrapMode:Text.WordWrap; maximumLineCount:2; elide:Text.ElideRight
                font.pointSize:Type.meta; color:Theme.color.textTertiary
            }
        }
    }
}

import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/Viewport.js" as Viewport
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"

Item {
    id: overview
    property string key: ""
    property string filter: ""
    property string order: "name"
    property string name: ""
    property var about: ({})
    property bool loading: false
    property Component artistsContent
    readonly property bool ready: !loading && layoutReady
    readonly property alias scrollView: view
    readonly property var summary: about.summary || ({})
    readonly property var subgenres: (about.items || []).slice(0, Metrics.discoveryPreview)
    readonly property url chosenImage: Artwork.genre(key, Artwork.rev)
    readonly property bool customImage: chosenImage.toString() !== ""
    property bool layoutReady: false
    function settleLayout() { subgenreGrid.forceLayout(); body.forceLayout(); layoutReady = true }
    Component.onCompleted: Qt.callLater(settleLayout)
    onAboutChanged: { layoutReady = false; Qt.callLater(settleLayout) }
    onWidthChanged: { layoutReady = false; Qt.callLater(settleLayout) }
    readonly property real albumShelfHeight: CardLayout.cardWidth + Metrics.cardTextHeight + Metrics.cardTextGap + Metrics.cardRing * 2 + Metrics.gap8 // derived: one card row and its focus rings
    signal activated(var entry)
    signal browseRequested(string section)
    signal playRequested()
    PageLifetime { id:shelfLifetime }
    function showShelf(loader,wanted) {
        if (wanted) loader.active=true
        else shelfLifetime.cancel(loader)
    }
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
                DiscoveryMosaic { anchors.fill: parent; images: overview.customImage ? [overview.chosenImage] : overview.summary.images || []; art:overview.about.art || ({}); label: overview.name; activeImages: overview.inView(hero) }
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
                        PillButton { objectName: "genreChooseImage"; text: overview.customImage ? I18n.tr("Change image", I18n.rev) : I18n.tr("Add image", I18n.rev); kind: "backdrop"; onClicked: Artwork.chooseGenre(overview.key) }
                        PillButton { objectName: "genreForgetImage"; text: I18n.tr("Remove image", I18n.rev); kind: "backdrop"; visible: overview.customImage; onClicked: Artwork.forgetGenre(overview.key) }
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
                readonly property bool wantedActive:overview.inView(albums)
                active:false
                onWantedActiveChanged:overview.showShelf(albums,wantedActive)
                sourceComponent: Item {
                    AlbumGridModel { id: records; objectName:"genreAlbumModel"; kind:"discovery"; key:overview.key; filter:overview.filter; order:overview.order.replace("name", "title"); maxRows:1; columns:grid.columns; contentRev:Library.contentRev + Library.remoteRev }
                    AlbumGrid { id:grid; anchors.fill:parent; model:records; visible:records.ready && records.error === ""; onOpen: albumId => Navigation.go("album", albumId) }
                    ShelfSkeleton { objectName:"genreAlbumsLoading"; anchors.fill:parent; visible:!records.ready }
                    Column {
                        objectName:"genreAlbumsMessage"
                        anchors.centerIn:parent; spacing:Metrics.gap12
                        visible:records.ready && (records.error !== "" || records.albums === 0)
                        Text { text:records.error || I18n.tr("No albums in this selection", I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.body }
                        PillButton { objectName:"genreAlbumsRetry"; anchors.horizontalCenter:parent.horizontalCenter; visible:records.error !== ""; text:I18n.tr("Retry", I18n.rev); onClicked:records.retry() }
                    }
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
                readonly property bool wantedActive:overview.inView(artists)
                active:false
                onWantedActiveChanged:overview.showShelf(artists,wantedActive)
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
                readonly property int columns: Math.max(1, Math.floor((width + spacing) / Metrics.sceneCardWidth))
                Repeater {
                    model: overview.subgenres
                    DiscoveryEntry {
                        id: tile
                        required property var modelData
                        width: (parent.width - parent.spacing * (parent.columns - 1)) / parent.columns
                        height: Metrics.sceneCardHeight
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

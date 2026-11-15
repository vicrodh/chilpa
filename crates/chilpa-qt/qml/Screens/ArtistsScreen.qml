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
    property string order: SettingsModel.kept("ui.artists.order") || "name"

    AlbumGridModel {
        id: everyone
        objectName: "artistsPageModel"
        kind: "artists"
        key: page.role
        contentRev: Library.contentRev + Library.rev + Library.remoteRev
        columns: grid.columns
        filter: head.text
        order: page.order
    }

    CollectionHead {
        id: head
        objectName: "artistsPageHeading"
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        title: I18n.tr("Artists", I18n.rev)
        count: everyone.ready && everyone.error === "" ? I18n.trCount("{} artist", "{} artists", everyone.albums, I18n.rev) : ""
        order: page.order
        placeholder: I18n.tr("Filter artists", I18n.rev)
        onOrderPicked: value => { page.order = value; SettingsModel.keep("ui.artists.order", value) }

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
        objectName: "artistsPageGrid"
        visible: everyone.ready && everyone.error === ""
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        bottomMargin: Metrics.contentBottom
        artists: everyone
    }
    ShelfSkeleton {
        objectName:"artistsPageLoading"; anchors.fill:grid
        visible:!everyone.ready && everyone.error === ""; round:true; oneRow:false
        fixedCardWidth:Metrics.sceneCardWidth
        count:grid.columns * Math.ceil(grid.height / (Metrics.sceneCardHeight + Metrics.sceneRowGap))
    }
    Column {
        objectName:"artistsPageMessage"; anchors.centerIn:grid; width:Math.min(grid.width,Metrics.settingsMaxWidth); spacing:Metrics.gap12
        visible:everyone.error !== "" || everyone.ready && everyone.albums === 0
        Text { width:parent.width; horizontalAlignment:Text.AlignHCenter; wrapMode:Text.WrapAtWordBoundaryOrAnywhere; textFormat:Text.PlainText; text:everyone.error || I18n.tr("No artists in this selection",I18n.rev); font.pointSize:Type.body; color:Theme.color.textSecondary }
        PillButton { objectName:"artistsPageRetry"; anchors.horizontalCenter:parent.horizontalCenter; visible:everyone.error !== ""; text:I18n.tr("Retry",I18n.rev); onClicked:everyone.retry() }
    }
}

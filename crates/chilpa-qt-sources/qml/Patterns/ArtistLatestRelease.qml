import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Column {
    id: latest
    property var section: ({})
    property Flickable viewport: null
    readonly property bool ready: record.ready && (!section.tracks_key || tracks.ready)
    readonly property bool shown: record.albums > 0
    spacing: Metrics.gap12
    visible: shown
    AlbumGridModel {
        id: record
        kind: latest.section.model || "source"
        key: latest.section.model_key || latest.section.key || ""
        limit: 1; columns: 1
        contentRev: Library.remoteRev + Library.remoteArtRev
    }
    TrackListModel {
        id: tracks
        kind: latest.section.tracks_key ? "source" : ""
        key: latest.section.tracks_key || ""
        limit: 6; filterRev: Library.remoteRev + Library.remoteArtRev
        playingPath: Player.path
    }
    Item {
        width: parent.width; height: Math.max(Metrics.shelfHeader, openRelease.implicitHeight)
        Text {
            anchors.left: parent.left; anchors.right: openRelease.left; anchors.rightMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter
            text: latest.section.title || ""; textFormat: Text.PlainText
            elide: Text.ElideRight; font.pointSize: Type.section
            font.weight: Type.heading; font.letterSpacing: Type.sectionTracking; color: Theme.color.textPrimary
        }
        PillButton {
            id: openRelease
            objectName: "latestReleaseOpen"
            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Go to release", I18n.rev)
            onClicked: Navigation.go("album", record.albumId(0, record.rev))
        }
    }
    Grid {
        width: parent.width
        columns: width >= CardLayout.cardWidth * 2 + Metrics.gap12 ? 2 : 1
        spacing: Metrics.gap12
        AlbumCard {
            width: CardLayout.cardWidth; height: implicitHeight
            albumId: record.albumId(0, record.rev)
            title: record.title(0, record.rev); subtitle: record.subtitle(0, record.rev)
            year: record.year(0, record.rev); art: record.art(0, record.rev)
            aiGenerated: record.aiGenerated(0, record.rev)
            sources: albumId < 0 ? JSON.parse(AlbumActions.sources(albumId, record.rev) || "[]") : []
            qualityInfo: JSON.parse(AlbumActions.quality(albumId, record.rev) || "{}")
            canExpand: false
            onClicked: Navigation.go("album", albumId)
        }
        TrackList {
            objectName: "latestReleaseTracks"
            visible: tracks.count > 0
            width: parent.columns === 1 ? parent.width : parent.width - CardLayout.cardWidth - parent.spacing
            height: implicitHeight; fitContent: true; viewport: latest.viewport
            model: tracks; header: false; showArtist: false; showAlbum: false
        }
    }
}

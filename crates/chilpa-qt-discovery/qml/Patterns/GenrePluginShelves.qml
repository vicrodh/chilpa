import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.discovery
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Column {
    id: shelves
    objectName: "genrePluginShelves"
    property string genreKey: ""
    property bool nearViewport: false
    property Flickable viewport: null
    property var rows: {
        try { return JSON.parse(feed.document || "{}").plugins || [] }
        catch (_) { return [] }
    }
    readonly property string releaseIds: JSON.stringify(rows.filter(row => row.newReleases?.length).map(row => row.id))
    readonly property string artistIds: JSON.stringify(rows.filter(row => row.artists?.length).map(row => row.id))
    spacing: Metrics.gap24

    PageLifetime { id: rowLifetime }
    DiscoveryModel { id: feed; objectName: "genrePluginModel" }
    function request() {
        if (nearViewport && genreKey !== "")
            feed.request("genre_plugins", genreKey, "", "{}", Library.contentRev + Library.remoteRev + Library.remoteArtRev)
    }
    onNearViewportChanged: if (nearViewport) request()
    onGenreKeyChanged: { feed.cancel(); request() }
    Connections {
        target: Library
        enabled: shelves.nearViewport
        function onContentRevChanged() { shelves.request() }
        function onRemoteRevChanged() { shelves.request() }
        function onRemoteArtRevChanged() { shelves.request() }
    }

    PluginSection {
        objectName: "genrePluginReleases"
        width: parent.width
        title: I18n.tr("New releases from your plugins", I18n.rev)
        plugins: JSON.parse(shelves.releaseIds)
    }
    PluginSection {
        objectName: "genrePluginArtists"
        width: parent.width
        title: I18n.tr("Artist discovery", I18n.rev)
        plugins: JSON.parse(shelves.artistIds)
        artistsOnly: true
    }

    component PluginSection: Column {
        id: pluginSection
        required property string title
        required property var plugins
        property bool artistsOnly: false
        visible: plugins.length > 0
        spacing: Metrics.gap12
        Text { width: parent.width; text: pluginSection.title; elide: Text.ElideRight; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
        Repeater {
            model: pluginSection.plugins
            onItemRemoved: (index, item) => rowLifetime.cancel(item)
            AlbumCarousel {
                required property var modelData
                readonly property var source: shelves.rows.find(row => row.id === modelData)
                readonly property var entries: (pluginSection.artistsOnly ? source?.artists : source?.newReleases) || []
                objectName: (pluginSection.artistsOnly ? "genrePluginArtistRow:" : "genrePluginReleaseRow:") + modelData
                width: pluginSection.width
                title: source?.name || ""
                viewport: shelves.viewport
                round: pluginSection.artistsOnly
                size: entries.length
                destinations: entries.reduce((routes, row) => { if (row.key) routes[row.id] = row.key; return routes }, {})
                model: AlbumGridModel {
                    kind: "ids"
                    key: JSON.stringify(entries.map(row => row.id))
                    contentRev: Library.remoteRev + Library.remoteArtRev
                }
            }
        }
    }
}

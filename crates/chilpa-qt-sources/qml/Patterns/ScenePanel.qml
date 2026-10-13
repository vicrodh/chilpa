import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The two-column browse mode: a grouped artist rail and the
// selected artist's release sections. The source page supplies the releases.
Item {
    id: panel
    objectName: "scenePanel"
    property var artists: []
    property bool includeSources: false
    property string selectedId: ""
    signal artistSelected(string id)
    signal artistOpened(var artist)
    property string selectedPlugin: ""
    onSelectedIdChanged: selectedPlugin = ""
    readonly property var selectedArtist: artists.find(a => a.mbid === selectedId) || ({})
    readonly property var source: (selectedArtist.sources || []).find(s => s.plugin === selectedPlugin) || (selectedArtist.sources || [])[0] || ({})
    readonly property string pageKey: source.plugin && source.ref ? source.plugin + " native:" + source.ref : ""
    readonly property var page: pageKey ? JSON.parse(Sources.page(pageKey, Library.remoteRev) || "{}") : ({})

    ScenePanelRail {
        id: rail
        width: Metrics.scenePanelRailWidth; height: parent.height
        artists: panel.artists; selectedId: panel.selectedId
        onArtistSelected: (id) => panel.artistSelected(id)
    }
    Rectangle {
        x: rail.width; width: Metrics.hairline; height: parent.height
        color: Theme.color.surfaceRaised
    }
    Row {
        id: sourcePicker
        visible: (panel.selectedArtist.sources || []).length > 1
        x: rail.width + Metrics.scenePanelPadLeft; y: Metrics.gap8; spacing: Metrics.gap8
        Repeater {
            model: panel.selectedArtist.sources || []
            PillButton {
                required property var modelData
                text: modelData.name || modelData.plugin
                kind: panel.source.plugin === modelData.plugin ? "primary" : "secondary"
                onClicked: panel.selectedPlugin = modelData.plugin
            }
        }
    }
    ScenePanelAlbums {
        objectName: "scenePanelAlbums"
        x: rail.width + Metrics.hairline; y: sourcePicker.visible ? Metrics.sceneNavHeight : 0
        width: Math.max(0, parent.width - x); height: parent.height - y
        artist: panel.selectedArtist; source: panel.source; document: panel.page; includeSources: panel.includeSources
        onArtistOpened: panel.artistOpened(panel.selectedArtist)
    }
}

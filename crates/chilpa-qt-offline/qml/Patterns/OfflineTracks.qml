import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The copies of the Offline music page, as QBZ's offline cache lays them
// out (OfflineManagerView): the order and "Failed only" over a rail of
// artists and the records, each with its tracks. Servers are a choice of
// their own beside them when there is more than one. Narrow, the rail
// becomes a choice too. What is checked gathers in a bar at the bottom.
Item {
    id: tracks
    property var doc: ({})
    property var filters: ({sort: "az", failed: false, server: 0, artist: ""})
    property var checked: ({})
    property bool busy: false
    property bool loading: false
    signal filtersEdited(var filters)
    signal toggled(int id)
    signal retried(var ids)
    signal removed(var ids)
    signal everyTrackChecked()
    signal checksCleared()
    readonly property var rows: doc.rows || []
    readonly property var servers: doc.servers || []
    readonly property var artists: doc.artists || []
    readonly property var checkedIds: Object.keys(checked).map(Number)
    readonly property bool railShown: width >= Metrics.offlineRail * 4 // derived: the rail beside three of its widths of records
    function edit(change) { tracks.filtersEdited(Object.assign({}, tracks.filters, change)) }

    Flow {
        id: toolbar
        width: parent.width
        spacing: Metrics.gap12
        SelectField {
            id: order
            objectName: "offlineSort"
            width: order.chrome + order.widestOption
            options: [I18n.tr("Name", I18n.rev), I18n.tr("Recent", I18n.rev), I18n.tr("Largest", I18n.rev), I18n.tr("Smallest", I18n.rev)]
            values: ["az", "recent", "largest", "smallest"]
            value: tracks.filters.sort || "az"
            onPicked: value => tracks.edit({sort: value})
        }
        Chip {
            objectName: "offlineFailedOnly"
            text: I18n.tr("Failed only", I18n.rev)
            active: tracks.filters.failed === true
            onClicked: tracks.edit({failed: !active, artist: ""})
        }
        SelectField {
            id: server
            objectName: "offlineServer"
            visible: tracks.servers.length > 1
            width: Math.min(server.chrome + server.widestOption, tracks.width)
            options: [I18n.tr("All servers", I18n.rev)].concat(tracks.servers.map(entry => entry.name))
            values: ["0"].concat(tracks.servers.map(entry => String(entry.id)))
            value: String(tracks.filters.server || 0)
            onPicked: value => tracks.edit({server: Number(value), artist: ""})
        }
        SelectField {
            id: artist
            objectName: "offlineArtist"
            visible: !tracks.railShown && tracks.artists.length > 0
            width: Math.min(artist.chrome + artist.widestOption, tracks.width)
            options: [I18n.tr("All artists", I18n.rev)].concat(tracks.artists.map(entry => entry.name || I18n.tr("Unknown artist", I18n.rev)))
            values: [""].concat(tracks.artists.map(entry => entry.name))
            value: tracks.filters.artist || ""
            onPicked: value => tracks.edit({artist: value})
        }
    }
    ListView {
        id: rail
        objectName: "offlineArtists"
        visible: tracks.railShown
        anchors.top: toolbar.bottom; anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        width: Metrics.offlineRail
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        bottomMargin: Metrics.contentBottom
        model: tracks.artists.length > 0 ? tracks.artists.length + 1 : 0 // derived: "All artists" first
        ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
        delegate: Rectangle {
            id: entry
            required property int index
            readonly property var artist: index === 0 ? null : tracks.artists[index - 1]
            readonly property string name: artist ? artist.name : ""
            readonly property bool active: (tracks.filters.artist || "") === name
            width: ListView.view.width - Metrics.gap8
            height: Metrics.offlineTrackRow
            radius: Metrics.radiusCard
            color: active ? Theme.color.stateSelected : pointer.hovered ? Theme.color.stateHover : "transparent"
            Column {
                x: Metrics.gap10; width: parent.width - Metrics.gap10 * 2 // derived: padding on both sides
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap2
                Text {
                    width: parent.width
                    text: entry.artist ? (entry.name || I18n.tr("Unknown artist", I18n.rev)) : I18n.tr("All artists", I18n.rev)
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    font.pointSize: Type.body; font.weight: entry.active ? Type.semibold : Type.regular
                    color: entry.active ? Theme.color.accent : Theme.color.textPrimary
                }
                Text {
                    width: parent.width
                    text: entry.artist ? I18n.trCount("{} album", "{} albums", entry.artist.albums, I18n.rev) + " · " + I18n.trCount("{} track", "{} tracks", entry.artist.tracks, I18n.rev)
                        : I18n.trCount("{} track", "{} tracks", tracks.doc.inScope || 0, I18n.rev)
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    font.pointSize: Type.small; color: Theme.color.textTertiary
                }
            }
            HoverHandler { id: pointer; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: tracks.edit({artist: entry.name}) }
        }
    }
    ListView {
        id: list
        objectName: "offlineRecords"
        anchors.top: toolbar.bottom; anchors.topMargin: Metrics.gap12
        anchors.bottom: bar.visible ? bar.top : parent.bottom
        anchors.left: tracks.railShown ? rail.right : parent.left; anchors.leftMargin: tracks.railShown ? Metrics.gap12 : 0
        anchors.right: parent.right
        clip: true
        reuseItems: true
        spacing: Metrics.gap2
        boundsBehavior: Flickable.StopAtBounds
        bottomMargin: bar.visible ? Metrics.gap12 : Metrics.contentBottom
        model: tracks.rows
        ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
        header: Item {
            width: list.width
            height: note.visible ? note.implicitHeight + Metrics.gap8 : 0
            Text {
                id: note
                objectName: "offlineShownNote"
                width: parent.width
                visible: (tracks.doc.shown || 0) < (tracks.doc.matching || 0)
                text: I18n.trArgs("Showing {} of {} tracks. Choose an artist or a server to see the rest.", [tracks.doc.shown || 0, tracks.doc.matching || 0], I18n.rev)
                textFormat: Text.PlainText; wrapMode: Text.WordWrap
                font.pointSize: Type.small; color: Theme.color.textSecondary
            }
        }
        delegate: OfflineRow {
            required property var modelData
            width: ListView.view.width - Metrics.gap8
            row: modelData
            busy: tracks.busy
            checked: tracks.checked[modelData.id] === true
            onToggled: tracks.toggled(modelData.id)
            onRetried: ids => tracks.retried(ids)
            onRemoved: ids => tracks.removed(ids)
        }
    }
    Column {
        objectName: "offlineEmpty"
        visible: !tracks.loading && tracks.rows.length === 0 && tracks.doc.files !== undefined
        anchors.top: toolbar.bottom; anchors.topMargin: Metrics.gap24
        x: list.x; width: list.width
        spacing: Metrics.gap6
        Text {
            width: parent.width
            text: (tracks.doc.files || 0) === 0 ? I18n.tr("No offline music yet", I18n.rev) : I18n.tr("No matching music.", I18n.rev)
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.bodyLarge; font.weight: Type.semibold; color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            visible: (tracks.doc.files || 0) === 0
            text: I18n.tr("Choose “Make available offline…” in the menu of a server's album, playlist or tracks.", I18n.rev)
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.small; color: Theme.color.textSecondary
        }
    }
    Rectangle {
        id: bar
        objectName: "offlineSelectionBar"
        visible: tracks.checkedIds.length > 0
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.contentBottom - Metrics.selectionBarHeight // derived: above a floating player
        width: parent.width
        height: Math.max(Metrics.selectionBarHeight, actions.height + Metrics.gap8 * 2) // derived: padding above and below
        radius: Metrics.radiusPanel
        color: Theme.color.surfaceRaised
        Flow {
            id: actions
            x: Metrics.gap12; width: parent.width - Metrics.gap12 * 2 // derived: padding on both sides
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8
            Text {
                height: Metrics.button32; verticalAlignment: Text.AlignVCenter
                text: I18n.trCount("{} track selected", "{} tracks selected", tracks.checkedIds.length, I18n.rev)
                textFormat: Text.PlainText; font.pointSize: Type.body; color: Theme.color.textPrimary
            }
            PillButton { objectName: "offlineCheckAll"; text: I18n.tr("Select all", I18n.rev); onClicked: tracks.everyTrackChecked() }
            PillButton { objectName: "offlineRetryChecked"; text: I18n.tr("Download again", I18n.rev); enabled: !tracks.busy; onClicked: tracks.retried(tracks.checkedIds) }
            PillButton { objectName: "offlineRemoveChecked"; text: I18n.tr("Remove…", I18n.rev); enabled: !tracks.busy; onClicked: tracks.removed(tracks.checkedIds) }
            PillButton { objectName: "offlineClearChecks"; text: I18n.tr("Clear", I18n.rev); onClicked: tracks.checksCleared() }
        }
    }
}

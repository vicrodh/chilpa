import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// The playlist manager's tools: what is typed to find a list or a folder,
// which kind it lists, whose, in what order, and as a grid or a list.
Row {
    id: tools
    property var sources: []
    property string source
    property bool grid: true
    property var sourceLabel: function(id) { return id }
    property alias query: search.text
    property alias kind: filterButton.value
    readonly property string order: sort.base
    readonly property bool ascending: sort.value === sort.base
    // What it lists changed: the search, the kind or the order.
    signal changed()
    signal sourceChosen(string value)
    signal viewChosen(bool grid)
    spacing: Metrics.gap8

    TextField {
        id: search
        width: Metrics.playlistManagerSearch
        placeholder: I18n.tr("Search playlists and folders", I18n.rev)
        onTextChanged: tools.changed()
    }
    PillButton {
        id: filterButton
        property string value: "all"
        glyph: Icons.filter
        text: value === "folders" ? I18n.tr("Folders", I18n.rev)
            : value === "playlists" ? I18n.tr("Playlists", I18n.rev)
            : value === "empty" ? I18n.tr("Empty folders", I18n.rev)
            : I18n.tr("All", I18n.rev)
        onClicked: filters.open()
        Menu {
            id: filters
            y: parent.height + Metrics.gap4
            entries: [
                {text: I18n.tr("All", I18n.rev), value: "all"},
                {text: I18n.tr("Folders", I18n.rev), value: "folders"},
                {text: I18n.tr("Playlists", I18n.rev), value: "playlists"},
                {text: I18n.tr("Empty folders", I18n.rev), value: "empty"}
            ]
            onChosen: (index) => filterButton.value = entries[index].value
            onClosed: tools.changed()
        }
    }
    PillButton {
        id: sourceButton
        objectName: "playlistSourceFilter"
        visible: tools.sources.length > 1
        glyph: Icons.server
        text: tools.sources.indexOf(tools.source) >= 0 ? tools.sourceLabel(tools.source) : I18n.tr("All sources", I18n.rev)
        onClicked: sourceMenu.open()
        Menu {
            id: sourceMenu
            objectName: "playlistSourceMenu"
            y: parent.height + Metrics.gap4
            entries: [{text: I18n.tr("All sources", I18n.rev), value: ""}].concat(tools.sources.map(id => ({text: tools.sourceLabel(id), value: id})))
            onChosen: (index) => tools.sourceChosen(entries[index].value)
        }
    }
    SortButton {
        id: sort
        value: "name"
        options: [
            {value: "name", text: I18n.tr("Name", I18n.rev)},
            {value: "recent", text: I18n.tr("Recent", I18n.rev), descending: true},
            {value: "tracks", text: I18n.tr("Number of tracks", I18n.rev), descending: true},
            {value: "plays", text: I18n.tr("Play count", I18n.rev), descending: true},
            {value: "custom", text: I18n.tr("Custom", I18n.rev), fixed: true}
        ]
        onPicked: (value) => sort.value = value
        onValueChanged: tools.changed()
    }
    Item { width: Math.max(0, parent.width - x - viewButtons.width); height: 1 }
    Row {
        id: viewButtons
        spacing: Metrics.gap2
        IconButton { round: true; glyph: Icons.layoutGrid; tone: tools.grid ? "accent" : "normal"; tip: I18n.tr("Grid", I18n.rev); onClicked: tools.viewChosen(true) }
        IconButton { round: true; glyph: Icons.list; tone: !tools.grid ? "accent" : "normal"; tip: I18n.tr("List", I18n.rev); onClicked: tools.viewChosen(false) }
    }
}

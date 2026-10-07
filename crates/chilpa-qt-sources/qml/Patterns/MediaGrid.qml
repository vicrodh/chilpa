import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

Column {
    id: grid
    property var entries: []
    property bool station: false
    property var selected: null
    readonly property int columns: Math.max(2, Math.floor((width + Metrics.gridGap) / (Metrics.pageHero + Metrics.gridGap)))
    readonly property int selectedIndex: selected ? entries.findIndex(e => e.id === selected.id) : -1
    signal opened(var entry)
    signal played(var entry)
    signal saved(var entry)
    signal detailed(var entry)
    signal closed()
    spacing: Metrics.gridGap
    Repeater {
        model: Math.ceil(grid.entries.length / grid.columns)
        Column {
            id: shelf
            required property int index
            width: grid.width; spacing: Metrics.gap16
            Row {
                spacing: Metrics.gridGap
                Repeater {
                    model: grid.entries.slice(shelf.index * grid.columns, (shelf.index + 1) * grid.columns)
                    MediaCard {
                        required property var modelData
                        width: (grid.width - (grid.columns - 1) * Metrics.gridGap) / grid.columns // derived: equal cards within the viewport
                        entry: modelData; station: grid.station
                        onOpened: grid.opened(entry)
                        onPlayed: grid.played(entry)
                        onSaved: grid.saved(entry)
                    }
                }
            }
            MediaQuickView {
                width: parent.width; visible: grid.selectedIndex >= 0 && Math.floor(grid.selectedIndex / grid.columns) === shelf.index
                entry: grid.entries[grid.selectedIndex] || ({})
                onClosed: grid.closed()
                onPlayed: grid.played(entry)
                onSaved: grid.saved(entry)
                onOpened: grid.detailed(entry)
            }
        }
    }
}

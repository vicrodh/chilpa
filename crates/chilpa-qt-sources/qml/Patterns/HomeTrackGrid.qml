import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"

// A Home rail of tracks in the compact grid: a click plays from that track
// on through the rail, a right click opens the track's menu. Most played
// says each track's plays, small, at the row's end.
SlimGrid {
    id: tracks
    property TrackListModel model
    property bool showPlays: false
    total: model ? model.count : 0
    cells: Repeater {
        model: tracks.model
        Item {
            id: cell
            required property int index
            required property var model
            visible: index < tracks.placed
            x: tracks.cellX(index); y: tracks.cellY(index)
            width: tracks.cellWidth; height: Metrics.slimRow
            SlimRow {
                anchors.fill: parent
                art: cell.model.art || ""
                title: cell.model.title || ""
                subtitle: cell.model.artist || ""
                playing: cell.model.playing === true
                available: cell.model.available !== false
                count: tracks.showPlays ? String(cell.model.plays || 0) : ""
                onActivated: tracks.model.play(cell.index)
                onMenuAsked: (x, y) => {
                    tracks.model.select(cell.index, 4)
                    const point = mapToItem(tracks, x, y)
                    menu.show(cell.index, point.x, point.y)
                }
            }
        }
    }
    TrackMenu { id: menu; model: tracks.model }
}

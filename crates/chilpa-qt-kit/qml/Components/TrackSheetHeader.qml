import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The table shares its boundaries with the ordinary rich and compact rows.
Item {
    id: head
    required property TrackColumns cols
    property var filters: ({})
    signal requested(string field, string label, Item anchor)
    height: Metrics.rowHeader + Metrics.gap12
    readonly property var cells: [
        {key: "index", label: "#", x: cols.indexX, width: cols.showIndex ? cols.index : 0},
        {key: "title", label: cols.names.title, x: cols.titleX, width: cols.titleW, filter: true},
        {key: "artist", label: cols.names.artist, x: cols.artistX, width: cols.artistW, filter: true},
        {key: "album", label: cols.names.album, x: cols.albumX, width: cols.albumW, filter: true},
        {key: "source", label: cols.names.source, x: cols.sourceX, width: cols.sourceWidth, filter: true},
        {key: "plays", label: cols.names.plays, x: cols.playsX, width: cols.plays},
        {key: "time", label: cols.names.time, x: cols.timeX, width: cols.time},
        {key: "rating", label: cols.names.rating, x: cols.ratingX, width: cols.rating, filter: true},
        {key: "quality", label: cols.names.quality, x: cols.qualityX, width: cols.quality, filter: true},
        {key: "format", label: cols.names.format, x: cols.formatX, width: cols.format, filter: true},
        {key: "favorite", label: cols.names.favorite, x: cols.moreX, width: cols.favorite, filter: true}
    ]
    Repeater {
        model: head.cells
        Item {
            id: cell
            required property var modelData
            readonly property var selection: head.filters[modelData.key]
            readonly property bool filtered: selection !== undefined && selection !== null && (typeof selection === "boolean" || selection.length > 0)
            objectName: "tracksColumn-" + modelData.key
            x: modelData.x
            width: Math.max(0, modelData.width - Metrics.gap4)
            height: head.height
            visible: modelData.width > 0
            activeFocusOnTab: modelData.filter === true
            Accessible.role: Accessible.Button
            Accessible.name: modelData.label
            Accessible.onPressAction: head.requested(modelData.key, modelData.label, cell)
            Keys.onReturnPressed: head.requested(modelData.key, modelData.label, cell)
            Keys.onEnterPressed: head.requested(modelData.key, modelData.label, cell)
            Keys.onSpacePressed: head.requested(modelData.key, modelData.label, cell)
            Rectangle {
                anchors.fill: parent
                anchors.margins: Metrics.gap2
                radius: Metrics.radiusItem
                color: cell.filtered ? Theme.color.stateSelected : (hover.hovered || cell.activeFocus) && cell.modelData.filter ? Theme.color.stateHover : "transparent"
            }
            Text {
                anchors.left: parent.left
                anchors.right: arrow.left
                anchors.verticalCenter: parent.verticalCenter
                text: cell.modelData.label
                elide: Text.ElideRight
                font.pointSize: Type.label
                font.weight: cell.filtered ? Type.bold : Type.medium
                color: cell.filtered ? Theme.color.accent : Theme.color.textSecondary
            }
            Icon {
                id: arrow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: cell.modelData.filter ? Metrics.iconSm + Metrics.gap4 : 0
                visible: cell.modelData.filter === true
                glyph: cell.filtered ? Icons.filter : Icons.chevronDown
                size: Metrics.iconSm
                color: cell.filtered ? Theme.color.accent : Theme.color.textTertiary
            }
            HoverHandler { id: hover; cursorShape: cell.modelData.filter ? Qt.PointingHandCursor : Qt.ArrowCursor }
            TapHandler { enabled: cell.modelData.filter === true; onTapped: head.requested(cell.modelData.key, cell.modelData.label, cell) }
        }
    }
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
}

import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The column titles of a track list, over the same widths as its rows.
Item {
    id: head

    property TrackColumns cols
    // A list that can be reordered from its titles, and the order in use.
    property bool sortable: false
    property string order

    signal sortRequested(string order)

    readonly property var titles: [
        { x: cols.indexX, w: cols.index - Metrics.gap8, right: true, text: "#", sort: "" },
        { x: cols.titleX, w: cols.titleW, right: false, text: I18n.tr("Title", I18n.rev), sort: "title" },
        { x: cols.artistX, w: cols.artistW, right: false, text: I18n.tr("Artist", I18n.rev), sort: "artist" },
        { x: cols.albumX, w: cols.albumW, right: false, text: I18n.tr("Album", I18n.rev), sort: "album" },
        { x: cols.sourceX, w: cols.sourceWidth, right: false, text: I18n.tr("Source", I18n.rev), sort: "" },
        { x: cols.qualityX, w: cols.quality, right: false, text: I18n.tr("Quality", I18n.rev), sort: "" },
        { x: cols.ratingX, w: cols.rating, right: false, text: I18n.tr("Rating", I18n.rev), sort: "" },
        { x: cols.playsX, w: cols.plays, right: true, text: I18n.tr("Plays", I18n.rev), sort: "plays" },
        { x: cols.timeX, w: cols.time, right: true, text: I18n.tr("Time", I18n.rev), sort: "duration" }
    ]

    width: cols.width
    height: Metrics.rowHeader

    Repeater {
        model: head.titles

        Text {
            required property var modelData
            readonly property bool active: head.sortable && modelData.sort !== "" && modelData.sort === head.order
            x: modelData.x
            width: Math.max(0, modelData.w - Metrics.gap8)
            visible: modelData.w > 0
            anchors.verticalCenter: parent.verticalCenter
            text: modelData.text
            horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
            elide: Text.ElideRight
            font.pointSize: Type.label
            font.weight: active ? Type.bold : Type.medium
            color: active ? Theme.color.textPrimary : Theme.color.textSecondary

            HoverHandler { enabled: head.sortable && modelData.sort !== ""; cursorShape: Qt.PointingHandCursor }
            TapHandler {
                enabled: head.sortable && modelData.sort !== ""
                onTapped: head.sortRequested(modelData.sort)
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }
}

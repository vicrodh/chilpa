import QtQuick
import com.blitzfc.chilpa
import "../Theme"

// The column titles of a track list, over the same widths as its rows.
Item {
    id: head

    property TrackColumns cols

    readonly property var titles: [
        { x: cols.indexX, w: cols.index - Metrics.gap8, right: true, text: "#" },
        { x: cols.titleX, w: cols.titleW, right: false, text: I18n.tr("Title", I18n.rev) },
        { x: cols.artistX, w: cols.artistW, right: false, text: I18n.tr("Artist", I18n.rev) },
        { x: cols.albumX, w: cols.albumW, right: false, text: I18n.tr("Album", I18n.rev) },
        { x: cols.qualityX, w: cols.quality, right: false, text: I18n.tr("Quality", I18n.rev) },
        { x: cols.playsX, w: cols.plays, right: true, text: I18n.tr("Plays", I18n.rev) },
        { x: cols.timeX, w: cols.time, right: true, text: I18n.tr("Time", I18n.rev) }
    ]

    width: cols.width
    height: Metrics.rowHeader

    Repeater {
        model: head.titles

        Text {
            required property var modelData
            x: modelData.x
            width: Math.max(0, modelData.w - Metrics.gap8)
            visible: modelData.w > 0
            anchors.verticalCenter: parent.verticalCenter
            text: modelData.text
            horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
            elide: Text.ElideRight
            font.pointSize: Type.label
            font.weight: Type.medium
            color: Theme.color.textSecondary
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }
}

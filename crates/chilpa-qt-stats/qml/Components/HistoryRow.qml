// hot-path: one per listen of the history's page.
import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One listen: when it started, the record's cover, the track and whose
// and from which record, how much of it was heard, and how it sounded --
// its format, a mark when it was bit-perfect -- or that it was skipped.
// A click opens its record.
Item {
    id: row

    // {clock, title, artist, album, played, length, skipped, bit_perfect, quality, art, album_id}
    property var listen: ({})
    property int index: 0

    signal opened(int album)

    height: Charts.historyRow

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        radius: Metrics.radiusRow
        hovered: area.containsMouse
    }
    Text {
        id: clock
        x: Metrics.gap12
        width: Charts.timeColumn
        anchors.verticalCenter: parent.verticalCenter
        text: row.listen.clock || ""
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    ArtTile {
        id: cover
        anchors.left: clock.right
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.historyCover
        height: Charts.historyCover
        path: row.listen.art || ""
        radius: Metrics.radiusItem
        tint: row.index
    }
    Column {
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: played.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: row.listen.title || ""
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: [row.listen.artist, row.listen.album].filter(part => part).join(" — ")
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
    Text {
        id: played
        anchors.right: quality.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.playedColumn
        horizontalAlignment: Text.AlignRight
        text: row.listen.length ? row.listen.played + " / " + row.listen.length : row.listen.played || ""
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    Row {
        id: quality
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.qualityColumn
        layoutDirection: Qt.RightToLeft
        spacing: Metrics.gap6

        Chip {
            anchors.verticalCenter: parent.verticalCenter
            visible: row.listen.skipped === true
            text: I18n.tr("Skipped", I18n.rev)
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: row.listen.bit_perfect === true
            glyph: Icons.circleCheck
            size: Metrics.iconSm
            color: Theme.color.statusPositive
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: row.listen.quality || ""
            font.pointSize: Type.small
            font.features: { "tnum": 1 }
            color: Theme.color.textSecondary
        }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: (row.listen.album_id || 0) > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if ((row.listen.album_id || 0) > 0) row.opened(row.listen.album_id)
    }
}

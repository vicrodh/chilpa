// hot-path: one per listen of the history's page.
import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One listen in the history's fixed columns: when it started, the track
// (with its record's cover and two lines when rich, one line when
// compact), whether it was skipped, its format and a mark when it was
// bit-perfect. A click opens its record.
Item {
    id: row

    // {clock, title, artist, album, played, length, skipped, bit_perfect, quality, art, album_id}
    property var listen: ({})
    property int index: 0
    property bool rich: true

    signal opened(int album)

    height: rich ? Charts.historyRow : Metrics.rowCompact

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        radius: row.rich ? Metrics.radiusRow : 0
        hovered: area.containsMouse
    }
    // Under the cells, so the skip's tip still finds the pointer.
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: (row.listen.album_id || 0) > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if ((row.listen.album_id || 0) > 0) row.opened(row.listen.album_id)
    }
    Text {
        id: clock
        x: Charts.historyInset
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
        visible: row.rich
        width: row.rich ? Charts.historyCover : 0
        height: Charts.historyCover
        path: row.rich ? row.listen.art || "" : ""
        radius: Metrics.radiusItem
        tint: row.index
    }
    Item {
        id: track
        objectName: "historyTrack"
        anchors.left: cover.right
        anchors.leftMargin: row.rich ? Metrics.gap12 : 0
        anchors.right: skipped.left
        anchors.rightMargin: Metrics.gap12
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        Text {
            id: title
            y: row.rich ? parent.height / 2 - height - Metrics.gap2 / 2 : (parent.height - height) / 2 // derived: two lines around the middle
            // Measured apart: bound to its own implicit width, an elided
            // text loops.
            width: row.rich ? parent.width : Math.min(Math.ceil(measure.advanceWidth(text)), parent.width * Charts.historyTitleShare)
            text: row.listen.title || ""
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        FontMetrics {
            id: measure
            font: title.font
        }
        Text {
            x: row.rich ? 0 : title.width + Metrics.gap8
            y: row.rich ? parent.height / 2 + Metrics.gap2 / 2 : (parent.height - height) / 2 // derived: two lines around the middle
            width: parent.width - x
            text: [row.listen.artist, row.listen.album].filter(part => part).join(" — ")
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
    Item {
        id: skipped
        anchors.right: quality.left
        width: Charts.skippedColumn
        height: parent.height

        Chip {
            id: chip
            anchors.verticalCenter: parent.verticalCenter
            visible: row.listen.skipped === true
            text: I18n.tr("Skipped", I18n.rev)
        }
        HoverHandler {
            enabled: chip.visible
            onHoveredChanged: hovered ? TipState.show(I18n.trArgs("Heard {} of {}", [row.listen.played || "", row.listen.length || "?"], I18n.rev), chip)
                                      : TipState.hide(chip)
        }
    }
    Text {
        id: quality
        objectName: "historyQuality"
        anchors.right: perfect.left
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.qualityColumn
        text: row.listen.quality || ""
        elide: Text.ElideRight
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textSecondary
    }
    Item {
        id: perfect
        anchors.right: parent.right
        anchors.rightMargin: Charts.historyInset
        width: Charts.perfectColumn
        height: parent.height

        Icon {
            anchors.centerIn: parent
            visible: row.listen.bit_perfect === true
            glyph: Icons.circleCheck
            size: Metrics.iconSm
            color: Theme.color.statusPositive
        }
    }
}

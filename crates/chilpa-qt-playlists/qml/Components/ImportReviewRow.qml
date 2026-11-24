import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One song against one destination, on one line: the song as the list
// named it, and what was found for it, from where, in what quality and how
// surely; or that nothing was, and which source did not answer.
Item {
    id: row
    property int rowIndex: 0
    property int number: 0
    property var original: null
    property string destination
    property var match: null
    property var sourceInfo: ({})
    property string quality
    property string problem
    property string problemDetail
    readonly property string matchTitle: match && original && (match.title || "").toLowerCase() !== (original.title || "").toLowerCase() ? match.title || "" : ""
    implicitHeight: Metrics.playlistRowHeight
    ListRowSurface { anchors.fill: parent; rowIndex: row.rowIndex; hovered: hover.hovered }
    HoverHandler { id: hover }
    Text {
        id: count
        x: Metrics.gap8; width: ImportFrame.number
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        text: row.number > 0 ? String(row.number) : ""
        font.pointSize: Type.meta; color: Theme.color.textTertiary
    }
    Text {
        id: song
        x: count.x + count.width + Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: (row.width - x - arrow.width - Metrics.gap8 * 3) / 2 // derived: the song and its match share what is left
        textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body
        text: row.original ? (row.original.title || "") + " — " + (row.original.artist || "") : row.destination
        color: row.original ? Theme.color.textPrimary : Theme.color.textSecondary
    }
    Icon {
        id: arrow
        x: song.x + song.width + Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: Lucide.arrowRight; iconFamily: Lucide.font; size: Metrics.iconSm; color: Theme.color.textTertiary
    }
    SourceIcon {
        id: mark
        visible: !!row.match
        x: arrow.x + arrow.width + Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        sourceInfo: row.sourceInfo
    }
    Text {
        id: found
        objectName: "importFound"
        x: row.match ? mark.x + mark.width + Metrics.gap6 : mark.x
        width: (trouble.visible ? trouble.x - Metrics.gap8 : row.width - Metrics.gap8) - x // derived: up to the problem or the edge
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.small
        text: row.match ? [row.matchTitle, row.quality, Math.round((row.match.confidence || 0) * 100) + "%"].filter(Boolean).join(" · ") : I18n.tr("Not found", I18n.rev)
        color: row.match ? Theme.color.textSecondary : Theme.color.textTertiary
    }
    Text {
        id: trouble
        objectName: "importTrouble"
        visible: text !== ""
        anchors.right: parent.right; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, row.width / 3) // derived: a third of the row at most
        textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.statusWarning
        text: row.problem
        HoverHandler { onHoveredChanged: hovered && row.problemDetail !== "" ? TipState.show(row.problemDetail, trouble) : TipState.hide(trouble) }
    }
}

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
    // How many other options the song has for this destination.
    property int optionCount: 0
    signal optionsAsked(Item anchor)
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
        objectName: "importSong"
        x: count.x + count.width + Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: (row.width - x - arrow.width - Metrics.gap8 * 3) / 2 // derived: the song and its match share what is left
        textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body
        // A video named only by who uploaded it shows that channel.
        text: row.original ? [row.original.title || "", row.original.artist || row.original.uploader || ""].filter(Boolean).join(" — ") : row.destination
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
        width: (more.visible ? more.x : trouble.visible ? trouble.x : row.width) - Metrics.gap8 - x // derived: up to the options, the problem or the edge
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.small
        text: row.match ? [row.matchTitle, row.match.version || "", row.quality, Math.round((row.match.confidence || 0) * 100) + "%"].filter(Boolean).join(" · ") : I18n.tr("Not found", I18n.rev)
        color: row.match ? Theme.color.textSecondary : Theme.color.textTertiary
    }
    PillButton {
        id: more
        objectName: "importOptions"
        visible: row.optionCount > 0
        kind: "ghost"
        anchors.right: trouble.visible ? trouble.left : parent.right; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.trArgs("{} more", [String(row.optionCount)], I18n.rev)
        glyph: Lucide.chevronDown; iconFamily: Lucide.font; glyphAfter: true
        onClicked: row.optionsAsked(more)
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

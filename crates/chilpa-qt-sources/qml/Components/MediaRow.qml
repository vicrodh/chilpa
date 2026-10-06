import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

Rectangle {
    id: row
    property var entry: ({})
    property bool compact: false
    signal opened()
    signal played()
    signal saved()
    signal queued()
    signal heard()
    implicitHeight: compact ? Metrics.button40 + Metrics.gap16 : Metrics.button40 * 2 + Metrics.gap16 // derived: proportional space within the parent
    color: entry.sounding ? Theme.color.stateSelected : hover.hovered ? Theme.color.fillField : "transparent"
    radius: Metrics.radiusCard
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.opened() }
    MediaArtwork { id: art; x: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter; width: row.compact ? Metrics.button40 : Metrics.button40 + Metrics.gap16; height: width; title: row.entry.title || ""; source: row.entry.image || ""; station: row.entry.live === true }
    Column {
        anchors.left: art.right; anchors.leftMargin: Metrics.gap12; anchors.right: actions.left; anchors.rightMargin: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap4
        Text { width: parent.width; text: row.entry.title || ""; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
        Text { width: parent.width; text: row.entry.playable === false ? row.entry.whyNot : [row.entry.meta, row.entry.subtitle, row.entry.heard].filter(Boolean).join(" · "); elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        Text { visible: !row.compact && text !== ""; width: parent.width; text: row.entry.description || ""; maximumLineCount: 2; wrapMode: Text.Wrap; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary; textFormat: Text.PlainText }
        Rectangle { visible: row.entry.progress > 0; width: parent.width * (row.entry.progress || 0); height: Metrics.gap2; color: Theme.color.accent }
    }
    Row {
        id: actions; anchors.right: parent.right; anchors.rightMargin: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap4
        IconButton { visible: row.entry.finite === true; glyph: Icons.check; tip: row.entry.played ? I18n.tr("Mark as unplayed", I18n.rev) : I18n.tr("Mark as heard", I18n.rev); onClicked: row.heard() }
        IconButton { glyph: row.entry.finite ? Icons.playlistAdd : row.entry.saved ? Icons.heartFilled : Icons.heart; tip: row.entry.finite ? I18n.tr("Add to queue", I18n.rev) : I18n.tr("Save", I18n.rev); onClicked: row.entry.finite ? row.queued() : row.saved() }
        IconButton { visible: row.entry.kind === "item"; glyph: Icons.playerPlayFilled; round: true; filled: true; tip: row.entry.whyNot || I18n.tr("Play", I18n.rev); enabled: row.entry.playable !== false; onClicked: row.played() }
    }
}

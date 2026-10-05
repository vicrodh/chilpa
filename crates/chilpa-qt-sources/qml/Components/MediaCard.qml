import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Item {
    id: card
    property var entry: ({})
    property bool station: false
    signal opened()
    signal played()
    signal saved()
    implicitHeight: art.height + title.height + meta.height + Metrics.gap16
    MediaArtwork { id: art; width: parent.width; height: width; title: card.entry.title || ""; source: card.entry.image || ""; station: card.station }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: card.opened() }
    Row {
        anchors.right: art.right; anchors.bottom: art.bottom; anchors.margins: Metrics.gap8; spacing: Metrics.gap8
        IconButton { visible: card.station; glyph: Icons.playerPlayFilled; round: true; filled: true; enabled: card.entry.playable !== false; tip: card.entry.whyNot || I18n.tr("Play", I18n.rev); onClicked: card.played() }
        IconButton { glyph: card.entry.saved ? Icons.heartFilled : Icons.heart; round: true; filled: card.entry.saved === true; tip: card.entry.saved ? I18n.tr("Remove from saved", I18n.rev) : I18n.tr("Save", I18n.rev); onClicked: card.saved() }
    }
    Text { id: title; anchors.top: art.bottom; anchors.topMargin: Metrics.gap8; width: parent.width; text: card.entry.title || ""; maximumLineCount: 2; elide: Text.ElideRight; wrapMode: Text.Wrap; font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
    Text { id: meta; anchors.top: title.bottom; anchors.topMargin: Metrics.gap4; width: parent.width; text: card.entry.playable === false ? card.entry.whyNot : card.entry.subtitle || ""; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
}

import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Item {
    id: hero
    property var entry: ({})
    property bool saved: false
    property bool expanded: false
    signal subscribed()
    signal played()
    implicitHeight: Math.max(art.height, details.height)
    MediaArtwork { id: art; width: Math.min(Metrics.pageHero, hero.width / 3); height: width; source: hero.entry.image || ""; title: hero.entry.title || "" } // derived: proportional space within the parent
    Column {
        id: details; anchors.left: art.right; anchors.leftMargin: Metrics.gap24; anchors.right: parent.right; spacing: Metrics.gap10
        Text { width: parent.width; text: hero.entry.title || ""; color: Theme.color.textPrimary; font.pointSize: Type.display; font.weight: Type.bold; wrapMode: Text.Wrap }
        Text { width: parent.width; text: hero.entry.subtitle || ""; color: Theme.color.textSecondary; font.pointSize: Type.body; wrapMode: Text.Wrap }
        Text { width: parent.width; text: hero.entry.description || ""; textFormat: Text.PlainText; color: Theme.color.textSecondary; font.pointSize: Type.body; wrapMode: Text.Wrap; maximumLineCount: hero.expanded ? 1000 : 3; elide: Text.ElideRight }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { visible: !!hero.entry.description; text: hero.expanded ? I18n.tr("Read less", I18n.rev) : I18n.tr("Read more", I18n.rev); onClicked: hero.expanded = !hero.expanded }
            PillButton { text: hero.saved ? I18n.tr("Subscribed", I18n.rev) : I18n.tr("Subscribe", I18n.rev); onClicked: hero.subscribed() }
            PillButton { text: I18n.tr("Play", I18n.rev); kind: "primary"; onClicked: hero.played() }
        }
        Text { width: parent.width; text: (hero.entry.facts || []).filter(f => !f.url).map(f => f.value).join(" · "); wrapMode: Text.Wrap; color: Theme.color.textSecondary; font.pointSize: Type.meta }
    }
}

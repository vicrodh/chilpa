import QtQuick
import QtQuick.Controls.Basic as Controls
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
import "../Components"
import "../Patterns"

Flickable {
    id: page
    readonly property var document: JSON.parse(Sources.snapshot || "{}")
    readonly property var entry: document.detail || ({})
    readonly property bool radio: entry.live === true
    readonly property string key: Navigation.routeKey
    function ask(action) { Sources.command(action, entry.id || "", entry.source || "") }
    function open() { const cut = key.indexOf(" "); if (cut >= 0 && Navigation.screen === "sourceEntry") Sources.command("detail", key.slice(cut + 1), key.slice(0, cut)) }
    Component.onCompleted: open()
    onKeyChanged: Qt.callLater(page.open)
    Timer { interval: Motion.poll; repeat: true; running: page.visible && !!(page.entry.sounding || page.entry.metadata_loading || (page.entry.extras || {}).loading); onTriggered: Sources.refresh() }
    contentHeight: body.height + Metrics.contentBottom; clip: true
    Controls.ScrollBar.vertical: ViewportScrollBar {}
    Column {
        id: body; x: Metrics.contentX; y: Metrics.contentTop; width: page.width - Metrics.contentX * 2; spacing: Metrics.gap24 // derived: page inset on both sides
        PillButton {
            objectName: "episodeParent"
            visible: !page.radio && !!page.entry.parent
            text: (page.entry.parent || {}).title || I18n.tr("Go to podcast", I18n.rev)
            glyph: Icons.chevronLeft
            kind: "ghost"
            onClicked: { Sources.command("parent", page.entry.id, page.entry.source); Navigation.goKey("source", page.entry.source) }
        }
        Text { visible: page.radio || !page.entry.parent; text: page.entry.name || I18n.tr("Sources", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.body }
        Item {
            width: parent.width; height: Math.max(art.height, identity.height)
            MediaArtwork { id: art; width: Math.min(page.radio ? Metrics.pageHero : Metrics.pageHero + Metrics.gap24, body.width / 3); height: width; title: page.entry.title || ""; source: page.entry.image || ""; station: page.radio } // derived: bounded artwork beside the reading column
            Column {
                id: identity; anchors.left: art.right; anchors.leftMargin: Metrics.gap24; anchors.right: parent.right; spacing: Metrics.gap12
                Text { width: parent.width; text: page.entry.title || ""; wrapMode: Text.Wrap; font.pointSize: Type.title; font.weight: Type.bold; color: Theme.color.textPrimary }
                Text { width: parent.width; text: [page.entry.subtitle, page.entry.meta].filter(Boolean).join(" · "); wrapMode: Text.Wrap; font.pointSize: Type.body; color: Theme.color.textSecondary }
                Text { width: parent.width; visible: page.entry.playable === false; text: page.entry.whyNot || ""; wrapMode: Text.Wrap; color: Theme.color.statusWarning; font.pointSize: Type.body }
                Flow {
                    width: parent.width; spacing: Metrics.gap8
                    PillButton { text: page.radio ? I18n.tr("Listen live", I18n.rev) : page.entry.progress > 0 && !page.entry.played ? I18n.tr("Resume", I18n.rev) : I18n.tr("Play", I18n.rev); kind: "primary"; enabled: page.entry.playable === true; onClicked: page.ask("play") }
                    PillButton { text: page.entry.saved ? I18n.tr("Saved", I18n.rev) : I18n.tr("Save", I18n.rev); onClicked: page.ask("save") }
                    PillButton { visible: !page.radio; text: I18n.tr("Add to queue", I18n.rev); enabled: page.entry.playable === true; onClicked: page.ask("queue") }
                    PillButton { visible: !page.radio; text: page.entry.played ? I18n.tr("Mark as unplayed", I18n.rev) : I18n.tr("Mark as heard", I18n.rev); onClicked: page.ask("heard") }
                }
                Text { visible: !!page.entry.heard; text: page.entry.heard || ""; color: Theme.color.textSecondary; font.pointSize: Type.meta }
            }
        }
        Rectangle {
            width: parent.width; height: live.height + Metrics.gap24; visible: page.radio && page.entry.sounding === true
            radius: Metrics.radiusPanel; color: Theme.color.fillField
            Column { id: live; x: Metrics.gap12; y: Metrics.gap12; width: parent.width - Metrics.gap24; spacing: Metrics.gap8
                Text { text: I18n.tr("On air", I18n.rev); font.pointSize: Type.section; font.weight: Type.heading; color: Theme.color.accent }
                Text { width: parent.width; text: page.entry.song || page.entry.title || ""; wrapMode: Text.Wrap; font.pointSize: Type.body; color: Theme.color.textPrimary }
            }
        }
        EpisodeMaterial { visible: !page.radio; width: parent.width; entry: page.entry }
        Flow {
            width: parent.width; spacing: Metrics.gap24
            Repeater {
                model: page.entry.facts || []
                Column {
                    required property var modelData
                    width: Math.min(Metrics.selectWidth, body.width); spacing: Metrics.gap4
                    Text { text: modelData.label; color: Theme.color.textSecondary; font.pointSize: Type.meta }
                    Text { width: parent.width; text: modelData.value; wrapMode: Text.Wrap; color: modelData.url ? Theme.color.accent : Theme.color.textPrimary; font.pointSize: Type.body; textFormat: Text.PlainText
                        HoverHandler { enabled: !!modelData.url; cursorShape: Qt.PointingHandCursor }
                        TapHandler { enabled: !!modelData.url; onTapped: if (/^https?:\/\//.test(modelData.url)) Qt.openUrlExternally(modelData.url) }
                    }
                }
            }
        }
        Text { text: page.document.attribution || ""; color: Theme.color.textSecondary; font.pointSize: Type.meta }
    }
}

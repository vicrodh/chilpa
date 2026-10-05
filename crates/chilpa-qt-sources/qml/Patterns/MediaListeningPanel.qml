import QtQuick
import QtQuick.Controls.Basic as Controls
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Item {
    id: panel
    property var document: ({})
    readonly property var entry: document.playing_entry || ({})
    readonly property var history: (document.on_air_history || []).filter(row => row.key === entry.key).slice().reverse()
    signal closed()
    Timer { interval: Motion.poll; running: panel.visible; repeat: true; onTriggered: Sources.refresh() }
    Row {
        id: heading; x: Metrics.gap12; y: Metrics.gap8; spacing: Metrics.gap8
        Text { width: panel.width - Metrics.button28 - Metrics.gap12 * 3; anchors.verticalCenter: parent.verticalCenter; text: panel.entry.live ? I18n.tr("On air", I18n.rev) : I18n.tr("Episode notes", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.semibold; elide: Text.ElideRight } // derived: title shares the heading with its close action
        IconButton { glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: panel.closed() }
    }
    Flickable {
        anchors.top: heading.bottom; anchors.topMargin: Metrics.gap12; anchors.bottom: parent.bottom; width: parent.width; clip: true
        contentHeight: content.height + Metrics.gap24
        Controls.ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
        Column {
            id: content; x: Metrics.gap12; width: parent.width - Metrics.gap24; spacing: Metrics.gap16
            Text { width: parent.width; text: panel.entry.title || ""; wrapMode: Text.Wrap; font.pointSize: Type.section; font.weight: Type.heading; color: Theme.color.textPrimary }
            Text { visible: panel.entry.live === true; width: parent.width; text: panel.entry.song || panel.entry.title || ""; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.color.accent; font.pointSize: Type.body }
            PillButton { text: I18n.tr("Full details", I18n.rev); onClicked: Navigation.goKey("sourceEntry", panel.entry.key) }
            EpisodeMaterial { visible: panel.entry.live !== true; width: parent.width; entry: panel.entry }
            Repeater {
                model: panel.entry.live ? panel.history : []
                Column {
                    required property var modelData
                    width: content.width; spacing: Metrics.gap4
                    Text { text: new Date(parent.modelData.time * 1000).toLocaleTimeString(Qt.locale(), Locale.ShortFormat); color: Theme.color.textSecondary; font.pointSize: Type.meta }
                    TextEdit { width: parent.width; text: parent.modelData.text; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; color: Theme.color.textPrimary; selectionColor: Theme.color.accent; font.pointSize: Type.body }
                }
            }
        }
    }
}

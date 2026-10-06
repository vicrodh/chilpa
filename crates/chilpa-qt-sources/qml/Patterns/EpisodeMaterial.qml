import QtQuick
import QtQuick.Controls.Basic as Controls
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

Column {
    id: material
    property var entry: ({})
    property string tab: "notes"
    readonly property var metadata: (entry.presentation || {}).metadata || ({})
    readonly property var extra: entry.extras || ({})
    readonly property var people: { try { return JSON.parse(metadata.people || "[]") } catch (_) { return [] } }
    function open(kind) { tab = kind; if (kind !== "notes") Sources.command("media_extra", entry.id, kind) }
    function jump(seconds) { if (entry.sounding) Player.seek(seconds); else Sources.command("play_from", entry.id, String(seconds)) }
    function clock(seconds) { return Math.floor(seconds / 60) + ":" + String(Math.floor(seconds % 60)).padStart(2, "0") }
    spacing: Metrics.gap16
    Flow {
        width: parent.width; spacing: Metrics.gap8
        PillButton { text: I18n.tr("Episode notes", I18n.rev); kind: material.tab === "notes" ? "primary" : "secondary"; onClicked: material.open("notes") }
        PillButton { visible: !!material.metadata.chapters_url; text: I18n.tr("Chapters", I18n.rev); kind: material.tab === "chapters" ? "primary" : "secondary"; onClicked: material.open("chapters") }
        PillButton { visible: !!material.metadata.transcript_url; text: I18n.tr("Transcript", I18n.rev); kind: material.tab === "transcript" ? "primary" : "secondary"; onClicked: material.open("transcript") }
    }
    Text { visible: material.tab === "notes"; width: Math.min(parent.width, Metrics.selectWidth * 3); text: material.entry.description || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.body; color: Theme.color.textSecondary } // derived: bounded reading width
    Text { visible: material.tab !== "notes" && (!!material.extra.error || !!material.extra.loading); width: parent.width; text: material.extra.error || I18n.tr("Loading…", I18n.rev); wrapMode: Text.Wrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
    PillButton { visible: material.tab !== "notes" && !!material.extra.error; text: I18n.tr("Try again", I18n.rev); onClicked: material.open(material.tab) }
    ListView {
        id: cues
        currentIndex: {
            if (!material.entry.sounding) return -1
            let current = -1
            const rows = material.extra.rows || []
            for (let i = 0; i < rows.length; i++) { if (rows[i].start === undefined || rows[i].start > Player.position) break; current = i }
            return current
        }
        onCurrentIndexChanged: if (material.tab === "transcript" && currentIndex >= 0 && !moving) positionViewAtIndex(currentIndex, ListView.Contain)
        width: parent.width; height: Math.min(Metrics.panesHeight * 2, contentHeight); clip: true // derived: bounded scrolling reading panel
        visible: material.tab !== "notes" && material.extra.kind === material.tab
        model: material.extra.rows || []; spacing: Metrics.gap4
        Controls.ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
        delegate: Rectangle {
            id: cue
            required property var modelData
            required property int index
            readonly property bool timed: modelData.start !== undefined
            readonly property bool current: timed && material.entry.sounding && Player.position >= modelData.start && (index + 1 >= (material.extra.rows || []).length || Player.position < material.extra.rows[index + 1].start)
            width: ListView.view.width; height: words.implicitHeight + Metrics.gap16
            radius: Metrics.radiusRow; color: current ? Theme.color.stateSelected : "transparent"
            Text { x: Metrics.gap8; y: Metrics.gap8; width: Metrics.button40; text: cue.timed ? material.clock(cue.modelData.start) : ""; font.pointSize: Type.meta; color: Theme.color.textSecondary }
            Text { id: words; x: Metrics.button40 + Metrics.gap16; y: Metrics.gap8; width: parent.width - x - Metrics.gap8; text: cue.modelData.text || ""; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: cue.current ? Theme.color.accent : Theme.color.textPrimary; font.pointSize: Type.body }
            HoverHandler { enabled: cue.timed; cursorShape: Qt.PointingHandCursor }
            TapHandler { enabled: cue.timed; onTapped: material.jump(cue.modelData.start) }
        }
    }
    Flow {
        width: parent.width; spacing: Metrics.gap12; visible: material.people.length > 0
        Repeater { model: material.people; Text { required property var modelData; text: (modelData.name || "") + (modelData.role ? " · " + modelData.role : ""); color: Theme.color.textSecondary; font.pointSize: Type.meta } }
    }
}

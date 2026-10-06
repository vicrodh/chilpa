import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "../Components"

// The page of one entry of a source: a station, an episode. What it is,
// what is on it right now, what its plugin knows of it, and play, queue
// and save. The route's key is the plugin's id, a space and the entry's id.
Flickable {
    id: page

    readonly property string key: Navigation.routeKey
    readonly property var entry: JSON.parse(Sources.snapshot || "{}").detail || ({})
    readonly property bool known: entry.id !== undefined

    function ask(action) { Sources.command(action, page.entry.id || "", page.entry.source || "") }
    function open() {
        const cut = key.indexOf(" ")
        // On the way out the key is already another screen's.
        if (cut < 0 || Navigation.screen !== "sourceEntry") return
        Sources.command("detail", key.slice(cut + 1), key.slice(0, cut))
    }

    contentHeight: body.height + Metrics.contentTop * 2 // derived: padding above and below
    clip: true

    Component.onCompleted: open()
    onKeyChanged: Qt.callLater(page.open)
    Timer {
        interval: Motion.poll
        repeat: true
        running: page.entry.metadata_loading === true
        onTriggered: Sources.refresh()
    }
    Connections {
        target: Player
        function onTitleChanged() { Sources.refresh() }
        function onPlayingChanged() { Sources.refresh() }
        function onArtChanged() { Sources.refresh() }
    }

    Column {
        id: body
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap12

        PageHero {
            objectName: "entryHero"
            width: parent.width
            visible: page.known
            label: page.entry.name || ""
            name: page.entry.title || ""
            art: page.entry.art ? "file://" + page.entry.art : page.entry.image || ""
            meta: [page.entry.finite && page.entry.meta ? page.entry.subtitle + " · " + page.entry.meta : page.entry.subtitle || "", page.entry.quality_label || ""].filter(Boolean).join(" · ")
            canPlay: page.entry.playable === true
            canShuffle: false
            canQueue: page.entry.finite === true
            onPlay: page.ask("play")
            onQueue: page.ask("queue")
        }

        Row {
            visible: page.known
            spacing: Metrics.gap8

            PillButton {
                objectName: "entrySave"
                glyph: page.entry.saved ? Icons.heartFilled : Icons.heart
                filled: page.entry.saved === true
                text: page.entry.saved ? I18n.tr("Saved", I18n.rev) : I18n.tr("Save", I18n.rev)
                onClicked: page.ask("save")
            }
            PillButton {
                visible: page.entry.finite === true
                glyph: Icons.check
                text: page.entry.played ? I18n.tr("Heard", I18n.rev) : I18n.tr("Mark as heard", I18n.rev)
                onClicked: page.ask("heard")
            }
        }

        Text {
            objectName: "entrySong"
            width: parent.width
            visible: text !== ""
            text: page.entry.song ? I18n.trArgs("Now playing: {}", [page.entry.song], I18n.rev) : page.entry.playable === false ? page.entry.whyNot : ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            font.weight: Type.medium
            color: page.entry.song ? Theme.color.accent : Theme.color.statusNegative
        }

        Text {
            width: parent.width
            visible: text !== ""
            text: page.entry.description || ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }

        Repeater {
            model: page.entry.facts || []

            Row {
                required property var modelData
                spacing: Metrics.gap12

                Text {
                    width: Metrics.screenSearch
                    text: modelData.label
                    elide: Text.ElideRight
                    font.pointSize: Type.small
                    color: Theme.color.textTertiary
                }
                Text {
                    width: body.width - Metrics.screenSearch - Metrics.gap12 // derived: what the label leaves
                    text: modelData.value
                    elide: Text.ElideRight
                    font.pointSize: Type.small
                    color: modelData.url ? Theme.color.accent : Theme.color.textPrimary

                    HoverHandler { enabled: !!modelData.url; cursorShape: Qt.PointingHandCursor }
                    TapHandler { enabled: !!modelData.url; onTapped: Qt.openUrlExternally(modelData.url) }
                }
            }
        }

        Text {
            visible: !page.known
            text: I18n.tr("This entry is no longer at hand. Open it again from its source.", I18n.rev)
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
}

import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"

// The live player stays visible while another buffer is being edited: the
// art, what plays, and under it three tabs -- its progress and quality, its
// lyrics from edge to edge with their settings at the end of the tabs'
// row, and what is known about it. The tab in view is kept between visits.
Item {
    id: card
    objectName: "listenNow"
    property string tab: ["playing", "lyrics", "about"].indexOf(SettingsModel.kept("ui.queue.panel")) >= 0 ? SettingsModel.kept("ui.queue.panel") : "playing"
    readonly property var tabs: ["playing", "lyrics", "about"]
    function show(name) { tab = name; SettingsModel.keep("ui.queue.panel", name) }
    // The lyrics as the player bar shows them, given by the screen.
    property Component lyrics

    Column {
        id: head
        width: parent.width
        spacing: Metrics.gap12
        Text {
            text: I18n.tr("Now playing", I18n.rev)
            color: Theme.color.textTertiary
            font.pointSize: Type.label
        }
        ArtTile {
            width: parent.width
            height: width
            source: Player.art
            radius: Metrics.radiusCard
        }
        Text {
            width: parent.width
            text: Player.title
            wrapMode: Text.Wrap
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        CreditLine { width: parent.width; credit: Player.artist; trackPath: Player.path; color: Theme.color.accent; font.pointSize: Type.meta }
        Text {
            width: parent.width
            text: Player.album
            wrapMode: Text.Wrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Item {
            width: parent.width
            height: tabs.height
            SegmentedControl {
                id: tabs
                objectName: "queuePanelTabs"
                options: [{glyph: Icons.waveSine, tip: I18n.tr("Now playing", I18n.rev)}, {glyph: Icons.quote, tip: I18n.tr("Lyrics", I18n.rev)},
                    {glyph: Icons.infoCircle, tip: I18n.tr("About", I18n.rev)}]
                current: card.tabs.indexOf(card.tab)
                onActivated: (index) => card.show(card.tabs[index])
            }
            // The lyrics' display settings, at the far end of their tab's row.
            IconButton {
                objectName: "queueLyricsOptions"
                visible: lyricsTab.item !== null
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                tip: I18n.tr("Display settings", I18n.rev)
                glyph: Icons.adjustmentsHorizontal
                size: Metrics.button26
                checked: visible && lyricsTab.item.optionsOpen
                onClicked: lyricsTab.item.toggleOptions()
            }
        }
    }

    Item {
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        width: parent.width

        Column {
            objectName: "queuePanelPlaying"
            visible: card.tab === "playing"
            width: parent.width
            spacing: Metrics.gap12
            SeekBar {
                width: parent.width
                height: Metrics.gap12
                position: Player.position
                duration: Player.duration
                onSeek: (seconds) => Player.seek(seconds)
            }
            Row {
                width: parent.width
                Text { width: parent.width / 2; text: Player.elapsed; font.pointSize: Type.small; color: Theme.color.textTertiary } // derived: half of the clock row
                Text { width: parent.width / 2; horizontalAlignment: Text.AlignRight; text: Player.remaining; font.pointSize: Type.small; color: Theme.color.textTertiary } // derived: half of the clock row
            }
            Text {
                width: parent.width
                text: Player.qualityLong
                wrapMode: Text.Wrap
                color: Theme.color.textSecondary
                font.pointSize: Type.small
            }
        }
        // Only the tab in view is built: lyrics follow the song while shown.
        Loader {
            id: lyricsTab
            objectName: "queuePanelLyrics"
            anchors.fill: parent
            active: card.tab === "lyrics"
            sourceComponent: card.lyrics
        }
        Loader {
            objectName: "queuePanelAbout"
            anchors.fill: parent
            active: card.tab === "about"
            sourceComponent: QueueAbout {}
        }
    }
}

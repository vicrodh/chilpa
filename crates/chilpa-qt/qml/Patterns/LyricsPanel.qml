import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The lyrics of what plays. Synced lyrics keep the line being sung in the
// middle and lit; unsynced ones are simply text to read.
Item {
    id: panel

    signal closed()

    PanelHeader {
        id: head
        width: parent.width
        title: I18n.tr("Lyrics", I18n.rev)

        IconButton {
            tip: I18n.tr("Close", I18n.rev)
            glyph: Icons.x
            size: Metrics.button26
            onClicked: panel.closed()
        }
    }

    ListView {
        id: lines
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        anchors.leftMargin: Metrics.gap16
        anchors.rightMargin: Metrics.gap16
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: Player.lyrics
        spacing: Metrics.gap8
        currentIndex: Player.lyricsSynced ? Player.lyricsLine : -1
        highlightRangeMode: Player.lyricsSynced ? ListView.ApplyRange : ListView.NoHighlightRange
        preferredHighlightBegin: height / 3 // derived: the sung line a third of the way down
        preferredHighlightEnd: height / 3 + Metrics.shelfHeader // derived: one line's room
        highlightMoveDuration: Motion.panel
        boundsBehavior: Flickable.StopAtBounds

        delegate: Text {
            required property int index
            required property string modelData
            width: ListView.view.width
            text: modelData
            wrapMode: Text.Wrap
            font.pointSize: Type.lyric
            font.weight: Type.bold
            color: !Player.lyricsSynced || index === Player.lyricsLine ? Theme.color.textPrimary : Theme.color.textTertiary
        }
    }

    Text {
        visible: Player.lyrics.length === 0
        anchors.centerIn: parent
        width: parent.width - Metrics.gap16 * 2 // derived: padding on both sides
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: Player.playing ? I18n.tr("No lyrics for this track.", I18n.rev) : I18n.tr("Lyrics show here while a track plays.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
}

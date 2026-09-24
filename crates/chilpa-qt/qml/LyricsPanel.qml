// The lyrics of what is playing.
//
// If it carries timings, the current line is marked and the list moves by
// itself. If not, it is shown whole and still: unsynchronized lyrics that
// jump would be lying to whoever is reading them.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"

    ListView {
        id: list
        anchors.fill: parent
        anchors.margins: 14
        clip: true
        model: Chilpa.lyrics
        currentIndex: Chilpa.lyricsLine
        onCurrentIndexChanged: if (Chilpa.lyricsSynced && currentIndex >= 0)
            positionViewAtIndex(currentIndex, ListView.Center)

        delegate: Label {
            width: list.width
            padding: 3
            wrapMode: Text.Wrap
            text: modelData
            font.pixelSize: active ? 16 : 14
            font.weight: active ? Font.DemiBold : Font.Normal
            color: active ? "#d7272b" : "#411d1f"
            opacity: !Chilpa.lyricsSynced || active ? 1 : 0.55
            readonly property bool active: index === Chilpa.lyricsLine
            Behavior on opacity { NumberAnimation { duration: 150 } }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: Chilpa.lyrics.length === 0
        text: Chilpa.playing ? "Sin letra para esta pista" : "Nada sonando"
        color: "#7a411d1f"
    }
}

// La letra de lo que suena.
//
// Si trae tiempos, el renglón que va se marca y la lista se mueve sola. Si
// no, se muestra completa y quieta: una letra sin sincronizar que brinca
// sería mentirle al que la está leyendo.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

Rectangle {
    id: raiz
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"

    ListView {
        id: lista
        anchors.fill: parent
        anchors.margins: 14
        clip: true
        model: Chilpa.lyrics
        currentIndex: Chilpa.lyricsLine
        onCurrentIndexChanged: if (Chilpa.lyricsSynced && currentIndex >= 0)
            positionViewAtIndex(currentIndex, ListView.Center)

        delegate: Label {
            width: lista.width
            padding: 3
            wrapMode: Text.Wrap
            text: modelData
            font.pixelSize: activo ? 16 : 14
            font.weight: activo ? Font.DemiBold : Font.Normal
            color: activo ? "#d7272b" : "#411d1f"
            opacity: !Chilpa.lyricsSynced || activo ? 1 : 0.55
            readonly property bool activo: index === Chilpa.lyricsLine
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

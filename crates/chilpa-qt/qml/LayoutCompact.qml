// El acomodo compacto: la biblioteca se queda con la ventana y lo que suena
// vive en una barra de abajo, como en un reproductor de toda la vida.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    anchors.fill: parent
    anchors.margins: 16
    spacing: 10

    OutputPanel { Layout.fillWidth: true }

    ActionsPanel { Layout.alignment: Qt.AlignLeft }

    // El grueso de la ventana: lo que se está mirando y, a la derecha, la
    // letra de lo que suena.
    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 10

        Results {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Chilpa.searchText !== "" || Chilpa.openAlbum !== ""
        }

        Library {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Chilpa.searchText === "" && Chilpa.openAlbum === ""
        }

        LyricsPanel {
            Layout.preferredWidth: 300
            Layout.fillHeight: true
            visible: Chilpa.lyrics.length > 0
        }
    }

    QueuePanel {
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        visible: Chilpa.queue.length > 1
    }

    // La barra de abajo: transporte y lo que suena, en un renglón.
    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        TransportPanel { Layout.alignment: Qt.AlignVCenter }
        NowPlayingPanel {
            Layout.fillWidth: true
            Layout.preferredHeight: 112
            Layout.minimumHeight: 112
            compact: true
        }
        VisualizerPanel {
            Layout.preferredWidth: 260
            Layout.preferredHeight: 112
            visible: Chilpa.playing
        }
    }
}

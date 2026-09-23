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

    PanelSalida { Layout.fillWidth: true }

    PanelAcciones { Layout.alignment: Qt.AlignLeft }

    // El grueso de la ventana: lo que se está mirando.
    Resultados {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: Chilpa.searchText !== "" || Chilpa.openAlbum !== ""
    }

    Biblioteca {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: Chilpa.searchText === "" && Chilpa.openAlbum === ""
    }

    PanelCola {
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        visible: Chilpa.queue.length > 1
    }

    // La barra de abajo: transporte y lo que suena, en un renglón.
    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        PanelTransporte { Layout.alignment: Qt.AlignVCenter }
        PanelAhoraSuena {
            Layout.fillWidth: true
            Layout.preferredHeight: 112
            compacto: true
        }
    }
}

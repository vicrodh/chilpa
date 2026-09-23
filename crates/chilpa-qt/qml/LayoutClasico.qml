// El acomodo clásico: todo en una columna, de arriba abajo.
//
// Un layout no tiene lógica: elige qué paneles se ven y dónde. Todo lo que
// necesitan lo leen del puente, así que agregar otro acomodo es agregar un
// `Layout<nombre>.qml` al lado de este y elegirlo en las preferencias.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 14

    PanelSalida { Layout.fillWidth: true }

    PanelAcciones { Layout.alignment: Qt.AlignLeft }

    PanelTransporte { Layout.alignment: Qt.AlignLeft }

    PanelAhoraSuena {
        Layout.fillWidth: true
        Layout.preferredHeight: 152
        Layout.alignment: Qt.AlignTop
    }

    // Los resultados, cuando hay algo escrito en la búsqueda o un álbum
    // abierto.
    Resultados {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.minimumHeight: 200
        visible: Chilpa.searchText !== "" || Chilpa.openAlbum !== ""
    }

    Biblioteca {
        Layout.fillWidth: true
        Layout.fillHeight: true
        // Una fila de portadas cabe completa, con sus tres renglones de
        // texto; menos que esto los recorta.
        Layout.minimumHeight: 260
        visible: Chilpa.albums.length > 0 && Chilpa.searchText === "" && Chilpa.openAlbum === ""
    }

    // La cola se asoma solo cuando hay más de una pista.
    PanelCola {
        Layout.fillWidth: true
        Layout.preferredHeight: 150
        visible: Chilpa.queue.length > 1
    }

    // Sin cola ni biblioteca, este relleno se queda con el espacio que sobra
    // y todo lo demás permanece arriba. Un elemento invisible no ocupa lugar
    // en un Layout, así que desaparece en cuanto hay algo que mostrar.
    Item {
        Layout.fillHeight: true
        visible: Chilpa.queue.length <= 1 && Chilpa.albums.length === 0
    }
}

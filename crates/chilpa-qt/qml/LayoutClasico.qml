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

    // Lo que suena y, a un lado, su letra cuando la hay.
    //
    // Con mínimo, no solo con preferido: cuando la ventana se queda corta,
    // un ColumnLayout encoge lo que puede, y esta tarjeta encogida se come
    // el reloj.
    RowLayout {
        Layout.fillWidth: true
        Layout.preferredHeight: 152
        Layout.minimumHeight: 152
        Layout.alignment: Qt.AlignTop
        spacing: 14

        PanelAhoraSuena { Layout.fillWidth: true; Layout.fillHeight: true }

        PanelLetra {
            Layout.preferredWidth: 300
            Layout.fillHeight: true
            visible: Chilpa.lyrics.length > 0
        }
    }

    PanelVisualizador {
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        Layout.minimumHeight: 110
        visible: Chilpa.playing
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
        // Una fila de portadas con sus renglones de texto y, arriba, las
        // pestañas. Por debajo de esto se recorta el último renglón de cada
        // ficha, que es lo primero que se puede perder sin que estorbe.
        Layout.minimumHeight: 260
        visible: Chilpa.albums.length > 0 && Chilpa.searchText === "" && Chilpa.openAlbum === ""
    }

    // La cola se asoma solo cuando hay más de una pista.
    PanelCola {
        Layout.fillWidth: true
        Layout.preferredHeight: 150
        // La cola es lo que cede primero cuando la ventana se queda corta:
        // siempre se puede desplazar por dentro.
        Layout.minimumHeight: 60
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

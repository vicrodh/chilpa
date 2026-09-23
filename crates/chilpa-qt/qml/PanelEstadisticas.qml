// Lo que se ha escuchado.
//
// Todo sale del log de reproducciones, que es append-only: los números se
// vuelven a sumar cada vez que se miran, no se llevan en un contador que se
// pueda desincronizar sin que nadie se entere.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: raiz
    spacing: 10

    function campos(renglon) {
        return renglon.split("\t")
    }

    // El resumen, en renglones de etiqueta y valor.
    Flow {
        Layout.fillWidth: true
        spacing: 18

        Repeater {
            model: Chilpa.stats
            delegate: ColumnLayout {
                spacing: 0
                readonly property var datos: raiz.campos(modelData)
                Label {
                    text: datos[1]
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                    color: "#411d1f"
                }
                Label {
                    text: datos[0]
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 16

        ListaDePistas {
            Layout.fillWidth: true
            Layout.fillHeight: true
            titulo: "Más escuchadas"
            pistas: Chilpa.topTracks
        }

        ListaDePistas {
            Layout.fillWidth: true
            Layout.fillHeight: true
            titulo: "Lo último que sonó"
            pistas: Chilpa.recentTracks
            conVeces: false
        }
    }
}

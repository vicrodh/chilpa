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
    id: root
    spacing: 10

    function fields(line) {
        return line.split("\t")
    }

    // El resumen, en renglones de etiqueta y valor.
    Flow {
        Layout.fillWidth: true
        spacing: 18

        Repeater {
            model: Chilpa.stats
            delegate: ColumnLayout {
                spacing: 0
                readonly property var cols: root.fields(modelData)
                Label {
                    text: cols[1]
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                    color: "#411d1f"
                }
                Label {
                    text: cols[0]
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

        TrackList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            title: "Más escuchadas"
            tracks: Chilpa.topTracks
        }

        TrackList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            title: "Lo último que sonó"
            tracks: Chilpa.recentTracks
            withPlayCounts: false
        }
    }
}

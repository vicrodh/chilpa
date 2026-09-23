// Elegir la salida, buscar y exigir bit-perfect.
//
// Los tres controles que cualquier layout necesita tener a mano. Lo que
// eligen no se queda aquí: va al puente, y de ahí lo lee todo lo demás, así
// que este panel se puede poner arriba, en una barra lateral o en ningún
// lado sin romper nada.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

RowLayout {
    id: root
    spacing: 10

    // Para que un layout decida si quiere la caja de búsqueda aquí o aparte.
    property bool withSearch: true
    // Lo mismo con el selector de acomodo.
    property bool withLayout: true

    Component.onCompleted: {
        Chilpa.refreshOutputs()
        // Sin salida recordada suena la primera de la lista, y el puente
        // tiene que saberlo antes del primer play.
        if (Chilpa.outputIds.indexOf(Chilpa.device) < 0 && Chilpa.outputIds.length > 0)
            Chilpa.device = Chilpa.outputIds[0]
    }

    Label { text: "Salida"; color: "#411d1f" }

    ComboBox {
        id: outputs
        Layout.fillWidth: true
        model: Chilpa.outputs
        // Lo elegido vive en el puente; esto solo lo muestra. Así la lista
        // queda en su lugar aunque la salida la cambie otra cosa.
        currentIndex: {
            const i = Chilpa.outputIds.indexOf(Chilpa.device)
            return i >= 0 ? i : 0
        }
        onActivated: Chilpa.choose(
            index >= 0 && index < Chilpa.outputIds.length ? Chilpa.outputIds[index] : "",
            bitPerfect.checked)
    }

    SearchBox {
        Layout.preferredWidth: 200
        visible: root.withSearch
    }

    LayoutSelector {
        Layout.preferredWidth: 130
        visible: root.withLayout
    }

    CheckBox {
        id: bitPerfect
        text: "Bit-perfect"
        checked: Chilpa.wantBitPerfect
        onToggled: Chilpa.choose(Chilpa.device, checked)
    }
}

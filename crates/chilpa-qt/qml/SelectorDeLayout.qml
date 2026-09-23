// Elegir el acomodo de la ventana.
//
// La lista la trae el puente desde los archivos que se compilaron, así que
// un acomodo nuevo aparece aquí solo.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

ComboBox {
    // El nombre del archivo es ASCII a propósito; aquí se muestra como se
    // escribe. Un acomodo sin entrada se muestra con su nombre de archivo,
    // que es feo pero nunca deja la lista incompleta.
    readonly property var etiquetas: ({ "Clasico": "Clásico", "Compacto": "Compacto" })

    model: Chilpa.layouts.map(n => etiquetas[n] !== undefined ? etiquetas[n] : n)
    currentIndex: Math.max(0, Chilpa.layouts.indexOf(Chilpa.layout))
    // El cambio se hace después: elegir aquí destruye el layout que
    // contiene a esta misma lista, y no se puede hacer a media señal.
    onActivated: {
        const nombre = Chilpa.layouts[index]
        Qt.callLater(() => Chilpa.chooseLayout(nombre))
    }
}

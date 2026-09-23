// La búsqueda, con sus atajos.
//
// Lo escrito vive en el puente (`Chilpa.searchText`), no en este campo: las
// vistas deciden qué mostrar según haya o no búsqueda, y no tienen por qué
// saber dónde quedó la caja en este layout.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

TextField {
    id: box
    placeholderText: "Buscar"
    text: Chilpa.searchText
    onTextEdited: Chilpa.search(text)

    Shortcut {
        sequence: "Ctrl+F"
        onActivated: box.forceActiveFocus()
    }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            Chilpa.search("")
            Chilpa.closeAlbum()
        }
    }
}

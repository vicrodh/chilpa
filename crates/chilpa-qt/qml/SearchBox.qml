// The search, with its shortcuts.
//
// What is typed lives in the bridge (`Chilpa.searchText`), not in this
// field: the views decide what to show depending on whether there is a
// search, and they have no reason to know where the box ended up in this
// layout.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

TextField {
    id: box
    placeholderText: Chilpa.translate("Search", Chilpa.trRev)
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

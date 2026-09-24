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
    onTextEdited: typing.restart()

    // A search runs when the typing stops, not on every key.
    //
    // It used to ask on every keystroke, and a search is the one query in
    // the library that cannot be answered from an index alone: FTS5 builds
    // the whole list of matches before it gives back a row, so typing a
    // six-letter word meant six searches of which five were thrown away
    // before anybody read them. The first letter is the worst of them,
    // because one letter matches most of a library.
    //
    // A sixth of a second is long enough that a word typed at speed asks
    // once, and short enough that it still feels like it answers as you
    // type.
    Timer {
        id: typing
        interval: 160
        onTriggered: Chilpa.search(box.text)
    }

    // Enter does not wait.
    onAccepted: {
        typing.stop()
        Chilpa.search(box.text)
    }

    Shortcut {
        sequence: "Ctrl+F"
        onActivated: box.forceActiveFocus()
    }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            typing.stop()
            Chilpa.search("")
            Chilpa.closeAlbum()
        }
    }
}

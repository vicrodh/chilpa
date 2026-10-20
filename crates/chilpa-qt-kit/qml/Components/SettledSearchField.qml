import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// A search field that says what is typed once the typing stops, trimmed:
// what a list filters by without filtering again on every key.
SearchField {
    id: field

    property string typed
    function clear() { text = ""; typed = "" }

    placeholder: I18n.tr("Search", I18n.rev)
    onTextChanged: settle.restart()
    onEscaped: text = ""

    Timer { id: settle; interval: Motion.searchSettle; onTriggered: field.typed = field.text.trim() }
}

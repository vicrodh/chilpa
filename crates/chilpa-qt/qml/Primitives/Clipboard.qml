import QtQuick

// TextEdit uses the platform clipboard on every desktop supported by Qt.
TextEdit {
    visible: false
    readOnly: true
    textFormat: TextEdit.PlainText
    function write(value) {
        text = value
        selectAll()
        copy()
        deselect()
    }
}

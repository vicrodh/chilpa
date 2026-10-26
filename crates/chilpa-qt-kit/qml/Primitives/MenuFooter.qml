import QtQuick

Column {
    id: footer
    property var entries: []
    signal chosen(int index)
    visible: entries.length > 0
    MenuEntry { width: parent.width; entry: ({separator:true}) }
    Repeater {
        model: footer.entries
        MenuEntry {
            required property int index
            required property var modelData
            objectName: "menuFooter" + index
            width: footer.width
            activeFocusOnTab: enabled
            entry: Object.assign({}, modelData, {focused:activeFocus})
            onClicked: if (enabled && !note && !separator) footer.chosen(index)
            Keys.onReturnPressed: clicked()
            Keys.onEnterPressed: clicked()
            Keys.onSpacePressed: clicked()
        }
    }
}

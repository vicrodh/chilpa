import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

PillButton {
    id: chooser
    property var sources: []
    property string sourceId: ""
    property string sourceName: ""
    signal picked(string id, string name)
    text: sourceId ? sourceName : I18n.tr("Local — supports mixed sources", I18n.rev)
    onClicked: menu.open()
    Menu {
        id: menu
        y: parent.height
        entries: [{text: I18n.tr("Local — supports mixed sources", I18n.rev), value: ""}].concat(
            chooser.sources.map(row => ({text: row.name, value: row.id, note: !row.accepts, detail: row.accepts ? "" : I18n.tr("Only tracks from this source", I18n.rev)})))
        onChosen: index => chooser.picked(entries[index].value, entries[index].text)
    }
}

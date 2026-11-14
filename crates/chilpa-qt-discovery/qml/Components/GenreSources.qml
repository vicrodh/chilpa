import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

IconButton {
    id: picker
    objectName: "genreSources"
    property var sources: []
    property var excluded: []
    signal chosen(var excluded)
    glyph: Icons.filter
    round: true
    checked: excluded.length > 0 || menu.opened
    tip: I18n.tr("Sources", I18n.rev)
    onClicked: menu.opened ? menu.close() : menu.open()
    Menu {
        id: menu
        objectName: "genreSourcesMenu"
        x: picker.width - width; y: picker.height + Metrics.gap6
        title: I18n.tr("Sources", I18n.rev)
        searchable: picker.sources.length > 5
        entries: picker.sources.map(row => ({text:row.label,sourceInfo:{icon:row.icon,local:row.key === "local:default"},checked:!picker.excluded.includes(row.key)}))
        onChosen: index => {
            const key = picker.sources[index].key
            picker.chosen(picker.excluded.includes(key) ? picker.excluded.filter(held => held !== key) : picker.excluded.concat([key]))
        }
        footerEntries: [{text:I18n.tr("Reset", I18n.rev),enabled:picker.excluded.length > 0}]
        onFooterChosen: picker.chosen([])
    }
}

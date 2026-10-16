import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The menu of a record's cover or an artist's picture, as QBZ has them:
// add one, or change and remove the one chosen; view it whole; open where
// it came from on the web; save it. Built again on every opening, so what
// it offers follows the choice.
Menu {
    id: menu
    // "album" or "artist": which words it says.
    property string kind: "album"
    // Whether the listener chose the picture shown, whether there is one
    // to show, and the web address it came from, if any.
    property bool custom: false
    property bool showing: false
    property string address: ""
    signal choose()
    signal forget()
    signal view()
    signal save()
    readonly property bool record: kind === "album"
    function build() {
        const rows = []
        if (custom) {
            rows.push({ text: record ? I18n.tr("Change cover", I18n.rev) : I18n.tr("Change image", I18n.rev), act: "choose", glyph: Icons.photoPlus })
            rows.push({ text: record ? I18n.tr("Remove cover", I18n.rev) : I18n.tr("Remove image", I18n.rev), act: "forget", glyph: Icons.trash })
        } else {
            rows.push({ text: record ? I18n.tr("Add cover", I18n.rev) : I18n.tr("Add image", I18n.rev), act: "choose", glyph: Icons.photoPlus })
        }
        if (showing)
            rows.push({ text: record ? I18n.tr("View cover", I18n.rev) : I18n.tr("View image", I18n.rev), act: "view", glyph: Icons.eye })
        if (address !== "")
            rows.push({ text: I18n.tr("Open in browser", I18n.rev), act: "browser", glyph: Icons.externalLink })
        if (showing)
            rows.push({ text: I18n.tr("Save as…", I18n.rev), act: "save", glyph: Icons.cloudDownload })
        entries = rows
    }
    // Opens where the pointer asked, with the rows of now.
    function popupAt(left, top) {
        build()
        x = left
        y = top
        open()
    }
    onChosen: (index) => {
        const act = entries[index].act
        if (act === "choose") menu.choose()
        else if (act === "forget") menu.forget()
        else if (act === "view") menu.view()
        else if (act === "browser") Qt.openUrlExternally(menu.address)
        else if (act === "save") menu.save()
    }
}

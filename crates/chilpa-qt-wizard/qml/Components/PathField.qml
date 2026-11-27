import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/LocalPaths.js" as LocalPaths
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: field
    property string value: ""
    property bool directory: false
    // The system's dialog answers whoever asked: this one, by its identity.
    readonly property string asker: "pathField " + String(field)
    signal picked(string path)
    spacing: Metrics.gap6
    Text {
        width: parent.width
        text: field.value
        textFormat: Text.PlainText
        wrapMode: Text.WrapAnywhere
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    PillButton {
        objectName: "pluginPathPicker"
        text: I18n.tr("Choose…", I18n.rev)
        onClicked: field.directory ? SystemDialogs.pickFolder(field.asker, text, "") : SystemDialogs.pickFile(field.asker, text, "[]")
    }
    // What it hands on stays a file URL, as the plugin's form has always had it.
    Connections {
        target: SystemDialogs
        function onChosen(asker, path) { if (asker === field.asker) field.picked(LocalPaths.toUrl(path)) }
    }
}

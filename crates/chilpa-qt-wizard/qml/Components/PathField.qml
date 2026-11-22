import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import QtQuick
import QtQuick.Dialogs
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: field
    property string value: ""
    property bool directory: false
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
        onClicked: field.directory ? folder.open() : file.open()
    }
    FileDialog {
        id: file
        fileMode: FileDialog.OpenFile
        onAccepted: field.picked(selectedFile.toString())
    }
    FolderDialog {
        id: folder
        onAccepted: field.picked(selectedFolder.toString())
    }
}

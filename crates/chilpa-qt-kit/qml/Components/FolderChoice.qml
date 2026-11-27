import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// A folder setting laid out as a row of Settings: its name, what it does
// and the folder in use on the left; on the right, choosing another with
// the system's dialog and going back to the default. When the words would
// be narrower than the buttons, the buttons go under them, as in SettingsRow.
Column {
    id: choice
    property string title
    property string description
    property string folder: ""
    property string defaultLabel
    property string defaultFolder: ""
    property string error: ""
    signal chosen(string folder)
    // The system's dialog answers whoever asked: this one, by its identity.
    readonly property string asker: "folderChoice " + String(choice)
    spacing: Metrics.gap6
    Item {
        id: line
        readonly property bool stacked: width - buttons.width - Metrics.gap16 < Math.max(words.least, Math.min(words.natural, buttons.width))
        width: parent.width
        height: stacked ? said.height + Metrics.gap6 + buttons.height : Math.max(said.height, buttons.height)
        Column {
            id: said
            width: line.stacked ? parent.width : parent.width - buttons.width - Metrics.gap16
            y: line.stacked ? 0 : (line.height - height) / 2 // derived: centred beside the buttons
            spacing: Metrics.gap2
            SettingText { id: words; width: parent.width; title: choice.title; description: choice.description }
            Text {
                objectName: "folderChoicePath"
                width: parent.width
                text: {
                    const folder = choice.folder || choice.defaultFolder
                    if (!folder) return choice.defaultLabel
                    try { return decodeURIComponent(folder) }
                    catch (error) { return folder }
                }
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
                font.pointSize: Type.meta
                color: Theme.color.textTertiary
            }
        }
        Row {
            id: buttons
            x: line.stacked ? 0 : line.width - width
            y: line.stacked ? said.height + Metrics.gap6 : (line.height - height) / 2 // derived: under the words, or centred beside them
            spacing: Metrics.gap8
            PillButton {
                objectName: "defaultFolder"
                visible: choice.folder !== ""
                text: choice.defaultLabel
                onClicked: choice.chosen("")
            }
            PillButton {
                objectName: "chooseFolder"
                text: I18n.tr("Choose a folder…", I18n.rev)
                // It opens where the folder in use is; a kept one can be a file URL.
                onClicked: {
                    let from = (choice.folder || choice.defaultFolder).replace(/^file:\/\//, "")
                    try { from = decodeURIComponent(from) } catch (error) {}
                    SystemDialogs.pickFolder(choice.asker, choice.title, from)
                }
            }
        }
    }
    Text {
        width: parent.width
        visible: text !== ""
        text: choice.error
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Theme.color.statusNegative
        font.pointSize: Type.small
    }
    Connections {
        target: SystemDialogs
        function onChosen(asker, path) { if (asker === choice.asker) choice.chosen(path) }
    }
}

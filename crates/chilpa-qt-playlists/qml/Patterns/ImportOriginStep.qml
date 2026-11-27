import QtQuick
import QtQuick.Dialogs
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/LocalPaths.js" as LocalPaths
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Where the playlist comes from: the origins the importer declares, in its
// order, and what the chosen one asks for. It says so while the list is read.
Column {
    id: step
    property var inputs: []
    property var choices: []
    property bool reading: false
    property bool busy: false
    property string chosen: ""
    property string file: ""
    readonly property var input: inputs.find(v => v.id === chosen) || inputs[0] || ({})
    readonly property bool fileInput: input.kind === "file"
    readonly property bool ready: fileInput ? file !== "" : origin.text.trim() !== ""
    signal findChoices(var payload)
    function payload() {
        return {input: input.id, text: origin.text, file: fileInput ? file : undefined, choice: choice.value,
            token: input.private_token ? token.text : undefined, station: Number(((input.choices || [])[stations.current] || {}).id || 0)}
    }
    spacing: Metrics.gap16
    FileDialog {
        id: picker
        title: I18n.tr("Choose playlist file", I18n.rev)
        nameFilters: ["Playlist files (*.m3u *.m3u8 *.pls *.xspf *.json)", "All files (*)"]
        onAccepted: step.file = selectedFile.toString()
    }
    SegmentedControl {
        id: kinds
        objectName: "importOrigins"
        visible: step.inputs.length > 1 && implicitWidth <= step.width
        options: step.inputs.map(v => ({text: v.name}))
        current: step.inputs.indexOf(step.input)
        enabled: !step.busy
        onActivated: index => step.chosen = step.inputs[index].id
    }
    SelectField {
        objectName: "importOriginSelect"
        visible: step.inputs.length > 1 && !kinds.visible
        width: step.width
        options: step.inputs.map(v => v.name)
        values: step.inputs.map(v => v.id)
        value: step.input.id || ""
        enabled: !step.busy
        onPicked: selected => step.chosen = selected
    }
    Column {
        width: step.width; spacing: Metrics.gap8; visible: !step.fileInput
        Text { text: step.input.name || ""; textFormat: Text.PlainText; font.pointSize: Type.small; color: Theme.color.textSecondary }
        TextField { id: origin; objectName: "importOriginText"; width: parent.width; enabled: !step.busy }
    }
    Column {
        width: step.width; spacing: Metrics.gap8; visible: step.fileInput
        Text { text: step.input.name || ""; textFormat: Text.PlainText; font.pointSize: Type.small; color: Theme.color.textSecondary }
        Row {
            width: parent.width; spacing: Metrics.gap12
            PillButton { id: choose; objectName: "importChooseFile"; text: I18n.tr("Choose file…", I18n.rev); enabled: !step.busy; onClicked: picker.open() }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - choose.width - parent.spacing
                textFormat: Text.PlainText; elide: Text.ElideMiddle; font.pointSize: Type.body
                text: step.file !== "" ? LocalPaths.fromUrl(step.file) : I18n.tr("No file chosen", I18n.rev)
                color: step.file !== "" ? Theme.color.textPrimary : Theme.color.textTertiary
            }
        }
    }
    Column {
        width: step.width; spacing: Metrics.gap8; visible: step.input.kind === "station"
        Text { text: I18n.tr("Station", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
        SegmentedControl {
            id: stations
            objectName: "importStation"
            options: (step.input.choices || []).map(v => ({text: v.name}))
            current: 0
            enabled: !step.busy
            onActivated: index => current = index
        }
    }
    Column {
        width: step.width; spacing: Metrics.gap8; visible: step.input.kind === "choices"
        PillButton {
            objectName: "importFindChoices"
            text: I18n.tr("Find playlists", I18n.rev)
            enabled: !step.busy && origin.text.trim() !== ""
            onClicked: step.findChoices({text: origin.text, token: token.text})
        }
        Text { visible: step.choices.length > 0; text: I18n.tr("Playlist", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
        SelectField {
            id: choice
            objectName: "importChoice"
            visible: step.choices.length > 0
            width: parent.width
            options: step.choices.map(v => v.name)
            values: step.choices.map(v => v.id)
            onPicked: selected => value = selected
        }
    }
    Column {
        width: step.width; spacing: Metrics.gap8; visible: !!step.input.private_token
        Text { text: I18n.tr("Optional access token for private playlists", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
        Row {
            width: parent.width; spacing: Metrics.gap8
            TextField { id: token; width: parent.width - reveal.width - parent.spacing; secret: true }
            PillButton { id: reveal; text: token.secret ? I18n.tr("Show token", I18n.rev) : I18n.tr("Hide token", I18n.rev); onClicked: token.secret = !token.secret }
        }
    }
    Text { width: step.width; visible: text !== ""; wrapMode: Text.Wrap; textFormat: Text.PlainText; text: step.input.hint || ""; font.pointSize: Type.small; color: Theme.color.textTertiary }
    Row {
        objectName: "importReading"
        width: step.width; visible: step.reading; spacing: Metrics.gap8
        BusyMark { clock: Pulse; active: step.reading; anchors.verticalCenter: parent.verticalCenter; size: Metrics.iconMd; color: Theme.color.accent }
        Text {
            width: parent.width - Metrics.iconMd - Metrics.gap8 // derived: remaining space after the status icon
            text: I18n.tr("Reading song list, please wait.", I18n.rev); wrapMode: Text.WordWrap
            font.pointSize: Type.small; color: Theme.color.textSecondary; Accessible.role: Accessible.StaticText
        }
    }
}

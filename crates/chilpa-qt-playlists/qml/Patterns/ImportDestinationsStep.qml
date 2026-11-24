import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// Where the read list goes: Chilpa's own playlist (any source, best
// quality), the library only, or a playlist in an account that takes
// writes; its name, description and folder; public only where an account
// can say so.
Flickable {
    id: step
    property var status: ({})
    property var places: ({})
    property var sources: []
    property var folders: []
    property bool busy: false
    property var selected: ["mixed"]
    property bool restored: false
    readonly property var preview: status.preview || ({})
    readonly property var settings: status.settings || ({})
    readonly property var order: ["mixed", "local"].concat(sources.filter(v => v.writable).map(v => v.id))
    readonly property bool remote: selected.some(id => id !== "mixed" && id !== "local")
    readonly property bool ready: !!preview.name && selected.length > 0
    function toggle(id) { selected = selected.indexOf(id) < 0 ? selected.concat([id]) : selected.filter(v => v !== id) }
    function matchesTargets(list) { return JSON.stringify(selected.slice().sort()) === JSON.stringify(list.slice().sort()) }
    function settingsPayload() { return {title: form.name, description: form.description, public: remote && publicToggle.checked, folder: Number(folder.value)} }
    function payload() { return Object.assign({targets: selected.slice()}, settingsPayload()) }
    onStatusChanged: if (!restored && (status.targets || []).length > 0) { selected = status.targets.slice(); restored = true }
    clip: true
    contentWidth: width
    contentHeight: column.height
    boundsBehavior: Flickable.StopAtBounds
    Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }
    Column {
        id: column
        width: step.width
        spacing: Metrics.gap20
        Column {
            width: parent.width; spacing: Metrics.gap4
            Text {
                objectName: "importPreviewName"
                width: parent.width; text: step.preview.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight
                font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary
            }
            Text {
                objectName: "importPreviewSize"
                width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; font.pointSize: Type.small; color: Theme.color.textSecondary
                text: [I18n.trCount("{} song", "{} songs", step.preview.total || 0, I18n.rev), step.preview.description || ""].filter(Boolean).join(" · ")
            }
        }
        Column {
            objectName: "importPreviewHead"
            width: parent.width; spacing: Metrics.gap2; visible: (step.preview.head || []).length > 0
            Repeater {
                model: step.preview.head || []
                Text {
                    required property var modelData
                    required property int index
                    width: parent.width; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textSecondary
                    text: (index + 1) + "  " + [modelData.title || "", modelData.artist || modelData.uploader || ""].filter(Boolean).join(" — ")
                }
            }
            Text {
                objectName: "importPreviewMore"
                visible: (step.preview.total || 0) > (step.preview.head || []).length
                textFormat: Text.PlainText; font.pointSize: Type.small; color: Theme.color.textTertiary
                text: I18n.trArgs("and {} more", [String((step.preview.total || 0) - (step.preview.head || []).length)], I18n.rev)
            }
        }
        Column {
            width: parent.width; spacing: Metrics.gap6
            Text { text: I18n.tr("Save to", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
            Repeater {
                model: step.order
                ImportDestinationRow {
                    required property string modelData
                    objectName: "importPlace_" + modelData
                    width: parent.width
                    title: (step.places[modelData] || {}).name || modelData
                    detail: (step.places[modelData] || {}).detail || ""
                    sourceInfo: (step.places[modelData] || {}).info || ({})
                    checked: step.selected.indexOf(modelData) >= 0
                    enabled: !step.busy
                    onToggled: step.toggle(modelData)
                }
            }
        }
        PlaylistForm {
            id: form
            objectName: "importForm"
            width: parent.width; localMetadata: false; enabled: !step.busy
            name: step.settings.title || step.preview.name || ""
            description: step.settings.description || step.preview.description || ""
        }
        Column {
            width: parent.width; spacing: Metrics.gap8
            Text { text: I18n.tr("Folder", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
            SelectField {
                id: folder
                objectName: "importFolder"
                width: Math.min(parent.width, Metrics.selectWidth)
                options: [I18n.tr("No folder", I18n.rev)].concat(step.folders.map(v => v.name))
                values: ["0"].concat(step.folders.map(v => String(v.id)))
                value: String(step.settings.folder || 0)
                enabled: !step.busy
                onPicked: selected => value = selected
            }
        }
        Row {
            objectName: "importPublic"
            visible: step.remote; spacing: Metrics.gap10
            Toggle { id: publicToggle; anchors.verticalCenter: parent.verticalCenter; checked: step.settings.public === true; enabled: !step.busy; onToggled: on => checked = on }
            Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Public playlist", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
        }
    }
}

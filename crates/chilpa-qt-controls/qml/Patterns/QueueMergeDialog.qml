import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Modal {
    id: dialog
    objectName: "queueMergeDialog"
    property var document: ({})
    property string error: ""
    property int selectedCount: 0
    property int trackCount: 0
    preferredWidth: Math.min(Metrics.playlistPickerWidth, parent ? parent.width - Metrics.modalInsets : 0)
    preferredHeight: header.implicitHeight + Math.max(Metrics.button32, list.contentHeight) + footer.implicitHeight
        + Metrics.gap12 * 2 + Metrics.modalInsets
    ListModel { id: queues }

    function count() {
        let selected = 0, tracks = 0
        for (let i = 0; i < queues.count; i++) {
            if (queues.get(i).chosen) { selected++; tracks += queues.get(i).songs }
        }
        selectedCount = selected; trackCount = tracks
    }
    function sync() {
        const tabs = document.tabs || []
        for (let i = queues.count - 1; i >= 0; i--) {
            const tab = tabs.find(tab => tab.id === queues.get(i).queueId)
            if (!tab) queues.remove(i)
            else { queues.setProperty(i, "label", tab.name); queues.setProperty(i, "songs", tab.count) }
        }
        for (const tab of tabs) {
            let found = false
            for (let i = 0; i < queues.count; i++) if (queues.get(i).queueId === tab.id) found = true
            if (!found) queues.append({queueId: tab.id, label: tab.name, songs: tab.count, chosen: false})
        }
        count()
    }
    function show() {
        queues.clear(); sync()
        for (let i = 0; i < queues.count; i++) queues.setProperty(i, "chosen", queues.get(i).queueId === document.selected)
        count(); error = ""; name.text = ""; originals.value = "keep"; open()
    }
    function neighbor(index, by) {
        for (let i = index + by; i >= 0 && i < queues.count; i += by) if (queues.get(i).chosen) return i
        return -1
    }
    function move(index, by) {
        const to = neighbor(index, by)
        if (to >= 0) queues.move(index, to, 1)
    }
    function combine() {
        const ids = []
        for (let i = 0; i < queues.count; i++) if (queues.get(i).chosen) ids.push(queues.get(i).queueId)
        if (originals.value === "remove") {
            removing.queueIds = JSON.stringify(ids)
            removing.queueName = name.text
            removing.open()
        } else finish(JSON.stringify(ids), name.text, true)
    }
    function finish(ids, label, keep) {
        error = Queues.merge(ids, label, keep)
        ListenList.refresh()
        if (error === "") close()
    }
    ConfirmationDialog {
        id: removing
        objectName: "queueMergeRemovalConfirmation"
        property string queueIds: "[]"
        property string queueName: ""
        title: I18n.tr("Remove original queues?", I18n.rev)
        message: I18n.tr("The selected queues will be removed after their tracks are copied to the combined queue. Their separate names and saved order will be lost.", I18n.rev)
        confirmText: I18n.tr("Combine and remove originals", I18n.rev)
        destructive: true
        targetValid: !dialog.document.pending && JSON.parse(queueIds).every(id => (dialog.document.tabs || []).some(tab => tab.id === id))
        onConfirmed: dialog.finish(queueIds,queueName,false)
    }
    onDocumentChanged: if (opened) sync()

    Column {
        id: layout
        anchors.fill: parent
        spacing: Metrics.gap12
        Column {
            id: header
            width: parent.width
            spacing: Metrics.gap12
            Text { text: I18n.tr("Combine queues", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.section; font.weight: Type.semibold }
            Text { width: parent.width; text: I18n.tr("Choose queues and arrange their order. Repeated tracks are kept.", I18n.rev); wrapMode: Text.Wrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
            Kit.TextField { id: name; objectName: "mergedQueueName"; width: parent.width; placeholder: I18n.tr("Combined queue", I18n.rev); onAccepted: if (dialog.selectedCount > 1 && !dialog.document.pending) dialog.combine() }
        }
        ListView {
            id: list
            width: parent.width
            height: Math.max(Metrics.button32, layout.height - header.height - footer.height - Metrics.gap12 - Metrics.gap12)
            clip: true
            model: queues
            spacing: Metrics.gap4
            ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
            delegate: Row {
                required property int index
                required property string queueId
                required property string label
                required property int songs
                required property bool chosen
                width: list.width - Metrics.gap12
                spacing: Metrics.gap8
                CircleCheck {
                    id: choice
                    objectName: "queueMergeChoice"
                    width: parent.width - up.width - down.width - Metrics.gap16
                    text: label + " · " + songs
                    checked: chosen
                    onToggled: { queues.setProperty(index, "chosen", choice.checked); dialog.count() }
                }
                IconButton { id: up; glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev); enabled: chosen && dialog.neighbor(index, -1) >= 0; onClicked: dialog.move(index, -1) }
                IconButton { id: down; glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev); enabled: chosen && dialog.neighbor(index, 1) >= 0; onClicked: dialog.move(index, 1) }
            }
        }
        Column {
            id: footer
            width: parent.width
            spacing: Metrics.gap8
            Text { text: I18n.trArgs("{} queues · {} tracks", [String(dialog.selectedCount), String(dialog.trackCount)], I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.body }
            Text { text: I18n.tr("What should happen to the original queues?", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body }
            SelectField { id: originals; objectName: "mergedQueueOriginals"; width: parent.width; values: ["keep", "remove"]; options: [I18n.tr("Keep original queues", I18n.rev), I18n.tr("Remove original queues", I18n.rev)]; value: "keep"; onPicked: selected => originals.value = selected }
            Text { width: parent.width; text: dialog.error; visible: text !== ""; wrapMode: Text.Wrap; color: Theme.color.accent; font.pointSize: Type.body }
            Row {
                spacing: Metrics.gap8
                PillButton { objectName: "confirmQueueMerge"; text: I18n.tr("Combine", I18n.rev); filled: true; enabled: dialog.selectedCount > 1 && !dialog.document.pending; onClicked: dialog.combine() }
                PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
            }
        }
    }
}

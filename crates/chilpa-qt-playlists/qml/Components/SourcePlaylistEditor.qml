import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Modal {
    id: editor
    property var info: ({})
    property string mode: "update"
    property string request: ""
    property string operation: ""
    property var results: []
    readonly property var state: (JSON.parse(Sources.snapshot || "{}").native_actions || {})[info.plugin] || ({})
    readonly property bool pending: state.pending === true
    readonly property string error: state.request_id === request ? state.error || "" : ""
    preferredWidth: Math.min(Metrics.confirmWidth + Metrics.confirmWidth, parent ? parent.width - Metrics.modalInsets : Metrics.confirmWidth)
    closePolicy: pending ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

    function begin(value, action) {
        info = value
        mode = action
        titleField.text = action === "create" ? "" : value.title || ""
        descriptionField.text = value.description || ""
        publicField.checked = action === "copy" ? false : value.public === true
        results = []
        find.text = ""
        request = ""
        open()
    }
    function submit(action, extra) {
        if (pending) return
        request = String(Date.now()) + ":" + action
        operation = action
        const payload = Object.assign({action:"playlist." + action, ref:info.ref, request_id:request}, extra || {})
        Sources.command("native", info.plugin, JSON.stringify(payload))
    }
    onStateChanged: {
        if (!request || state.request_id !== request || state.pending || state.error) return
        if (operation === "search") results = state.result.items || []
        else if (["create", "update", "copy", "delete"].indexOf(operation) >= 0) close()
    }
    Column {
        width: editor.width - Metrics.modalInsets
        spacing: Metrics.gap12
        Text { text: I18n.tr("Playlist", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.title }
        TextField { id: titleField; width: parent.width; enabled: !editor.pending; placeholder: I18n.tr("Name", I18n.rev) }
        TextField { id: descriptionField; width: parent.width; enabled: !editor.pending; placeholder: I18n.tr("Description", I18n.rev) }
        CircleCheck { id: publicField; enabled: !editor.pending; text: I18n.tr("Public playlist", I18n.rev) }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { text: editor.pending ? I18n.tr("Loading…", I18n.rev) : I18n.tr("Save", I18n.rev); enabled: !editor.pending && titleField.text.trim().length > 0; onClicked: editor.submit(editor.mode, {title:titleField.text, description:descriptionField.text, public:publicField.checked}) }
            PillButton { visible: editor.mode === "update"; text: I18n.tr("Delete…", I18n.rev); enabled: !editor.pending; onClicked: deletion.open() }
            PillButton { text: I18n.tr("Close", I18n.rev); enabled: !editor.pending; onClicked: editor.close() }
        }
        Text { width: parent.width; visible: !!editor.error; text: editor.error; wrapMode: Text.WordWrap; color: Theme.color.textSecondary; textFormat: Text.PlainText }
        Row {
            visible: editor.mode === "update"; width: parent.width; spacing: Metrics.gap8
            SearchField { id: find; width: parent.width - searchButton.width - Metrics.gap8; enabled: !editor.pending; placeholder: I18n.tr("Search", I18n.rev); onEscaped: text = ""; onTextChanged: editor.results = []; onAccepted: if (text.trim()) editor.submit("search", {text:text}) }
            PillButton { id: searchButton; text: I18n.tr("Search", I18n.rev); enabled: !editor.pending && find.text.trim().length > 0; onClicked: editor.submit("search", {text:find.text}) }
        }
        SourcePlaylistRows {
            width: parent.width
            visible: editor.mode === "update"
            rows: find.text ? editor.results : editor.info.entries || []
            adding: !!find.text
            busy: editor.pending
            onAdd: reference => editor.submit("add", {tracks:[reference]})
            onRemove: member => { memberRemoval.member = member; memberRemoval.reference = editor.info.ref; memberRemoval.plugin = editor.info.plugin; memberRemoval.open() }
            onMove: (member, before) => editor.submit("move", {members:[member],before:before})
        }
    }
    ConfirmationDialog {
        id: deletion
        property string reference: ""
        property string plugin: ""
        onAboutToShow: { reference = editor.info.ref; plugin = editor.info.plugin }
        title: I18n.trArgs("Delete “{}”?", [editor.info.title || ""], I18n.rev)
        message: I18n.tr("This permanently deletes the playlist at the source.", I18n.rev)
        confirmText: I18n.tr("Delete", I18n.rev)
        destructive: true
        targetValid: reference === editor.info.ref && plugin === editor.info.plugin && !editor.pending
        onConfirmed: editor.submit("delete")
    }
    ConfirmationDialog {
        id: memberRemoval
        property string member: ""
        property string reference: ""
        property string plugin: ""
        title: I18n.tr("Remove from playlist", I18n.rev)
        message: I18n.tr("The selected tracks will be removed from this playlist and its saved order.", I18n.rev)
        confirmText: I18n.tr("Remove", I18n.rev)
        destructive: true
        targetValid: reference === editor.info.ref && plugin === editor.info.plugin && !editor.pending
            && (editor.info.entries || []).some(entry => entry.member === member)
        onConfirmed: editor.submit("remove", {members:[member]})
    }

}

import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.query
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "../Components"
import "../Primitives"
import "../Theme"

// I exist only while open; catalog size never changes the modal footprint.
Modal {
    id: dialog
    objectName: "savedQueriesDialog"
    property string mode: "list"
    property string expression: ""
    property string selectedId: ""
    property string error: ""
    property bool readFailed: false
    property bool loadingCatalog: false
    property bool detached: false
    readonly property real roomWidth: detached ? Screen.desktopAvailableWidth : parent ? parent.width : Refine.savedDialogMinWide
    readonly property real roomHeight: detached ? Screen.desktopAvailableHeight : parent ? parent.height : Refine.savedDialogMinTall
    property var document: ({entries: []})
    readonly property var entries: document.entries || []
    popupType: detached ? Controls.Popup.Window : Controls.Popup.Item
    width: Math.min(roomWidth - Refine.savedDialogAround, Math.max(Refine.savedDialogMinWide, roomWidth * Refine.savedDialogShare))
    height: Math.min(roomHeight - Refine.savedDialogAround, Math.max(Refine.savedDialogMinTall, roomHeight * Refine.savedDialogShare))
    onOpened: {
        loadCatalog()
        if (mode === "save") nameField.focusInput()
        else closeButton.forceActiveFocus()
    }
    function refreshDetails() {
        const next = JSON.parse(SavedQueries.previews())
        if (next.error) { error = next.error; readFailed = true; return }
        document = next
        readFailed = false
    }
    function loadCatalog() {
        loadingCatalog = true
        error = SavedQueries.load()
        loadingCatalog = false
        readFailed = error !== ""
        if (!readFailed) refreshDetails()
    }
    Connections {
        target: SavedQueries
        function onRevChanged() { if (dialog.opened && !dialog.loadingCatalog) dialog.refreshDetails() }
    }
    function submit() {
        error = mode === "save" ? SavedQueries.save(nameField.text, expression)
                                : SavedQueries.rename(selectedId, nameField.text)
        if (error !== "") return
        if (mode === "save") close()
        else { mode = "list"; selectedId = ""; closeButton.forceActiveFocus() }
    }
    function rename(entry) {
        selectedId = entry.id
        nameField.text = entry.name
        expression = entry.expression
        mode = "rename"
        error = ""
        nameField.focusInput()
    }
    function openEntry(id) {
        const result = JSON.parse(SavedQueries.expression(id))
        error = result.error || Library.applyQuery(result.expression)
        if (error === "") close()
    }
    function askRemove(entry) {
        selectedId = entry.id
        confirmation.message = I18n.trArgs("Delete the saved query “{}”?", [entry.name], I18n.rev)
        confirmation.open()
    }

    Item {
        anchors.fill: parent
        Item {
            id: header
            width: parent.width
            height: title.height + Refine.headApart + subtitle.implicitHeight
            Text {
                id: title
                width: parent.width - closeIcon.width - Metrics.gap16
                height: closeIcon.height
                verticalAlignment: Text.AlignVCenter
                text: dialog.mode === "save" ? I18n.tr("Save query", I18n.rev)
                    : dialog.mode === "rename" ? I18n.tr("Rename query", I18n.rev) : I18n.tr("Saved queries", I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.section
                font.weight: Type.semibold
                font.letterSpacing: Type.sectionTracking
                color: Theme.color.textPrimary
            }
            SquareButton { id: closeIcon; anchors.right: parent.right; glyph: Lucide.x; iconFamily: Lucide.font; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
            Text {
                id: subtitle
                y: title.height + Refine.headApart
                width: parent.width
                text: I18n.tr("I save only the conditions. Opening a query uses the currently selected sources and keeps the queue.", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }
        Item {
            id: form
            anchors.top: header.bottom
            anchors.topMargin: Metrics.gap16
            width: parent.width
            height: Refine.savedFormTall
            visible: dialog.mode !== "list"
            Text { id: label; text: I18n.tr("Query name", I18n.rev); font.pointSize: Type.meta; font.weight: Type.medium; color: Theme.color.textSecondary }
            TextField {
                id: nameField
                objectName: "savedQueryName"
                y: label.height + Refine.innerApart
                width: parent.width
                placeholder: I18n.tr("Query name", I18n.rev)
                Accessible.name: placeholder
                onAccepted: dialog.submit()
            }
            Text {
                y: nameField.y + nameField.height + Refine.innerApart
                width: parent.width
                text: dialog.expression
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.family: Type.consoleFamily
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
        }
        SavedQueryTable {
            objectName: "savedQueriesTable"
            anchors.top: form.visible ? form.bottom : header.bottom
            anchors.topMargin: Metrics.gap16
            anchors.bottom: errorText.visible ? errorText.top : footer.top
            anchors.bottomMargin: Metrics.gap16
            width: parent.width
            entries: dialog.entries
            readFailed: dialog.readFailed
            editable: dialog.mode === "list" && !dialog.readFailed
            onOpenAsked: id => dialog.openEntry(id)
            onRenameAsked: entry => dialog.rename(entry)
            onRemoveAsked: entry => dialog.askRemove(entry)
        }
        Text {
            id: errorText
            objectName: "savedQueryError"
            anchors.bottom: footer.top
            anchors.bottomMargin: Metrics.gap12
            visible: dialog.error !== ""
            width: parent.width
            text: dialog.error
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.statusNegative
            Accessible.role: Accessible.AlertMessage
        }
        Row {
            id: footer
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Metrics.pillHeight
            spacing: Refine.headApart
            PillButton {
                visible: dialog.readFailed
                text: I18n.tr("Retry", I18n.rev)
                Accessible.name: text
                onClicked: dialog.loadCatalog()
            }
            PillButton {
                id: closeButton
                objectName: "savedQueryCancel"
                text: dialog.mode === "list" ? I18n.tr("Close", I18n.rev) : I18n.tr("Cancel", I18n.rev)
                Accessible.name: text
                onClicked: dialog.close()
            }
            PillButton {
                objectName: "savedQuerySubmit"
                visible: dialog.mode !== "list"
                enabled: !dialog.readFailed
                kind: "primary"
                text: dialog.mode === "save" ? I18n.tr("Save", I18n.rev) : I18n.tr("Rename", I18n.rev)
                Accessible.name: text
                onClicked: dialog.submit()
            }
        }
    }
    ConfirmDialog {
        id: confirmation
        objectName: "savedQueryConfirmation"
        width: Math.min(Metrics.confirmWidth, dialog.width)
        title: I18n.tr("Delete saved query", I18n.rev)
        confirmText: I18n.tr("Delete", I18n.rev)
        destructive: true
        onConfirmed: { dialog.error = SavedQueries.remove(dialog.selectedId); closeButton.forceActiveFocus() }
        onDismissed: closeButton.forceActiveFocus()
    }
}

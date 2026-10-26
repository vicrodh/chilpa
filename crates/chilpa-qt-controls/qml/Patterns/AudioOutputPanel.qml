import QtQuick
import QtQuick.Controls.Basic as QQC
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

Modal {
    id: panel
    objectName: "audioOutputPanel"
    property bool preview: false
    property var previewDocument: ({})
    readonly property var document: preview ? previewDocument : JSON.parse(AudioOutput.snapshot || "{}")
    property string page: preview ? "equipment" : AudioOutput.tab
    property Item returnFocus
    property var ownerWindow: parent ? parent.Window.window : null
    modal: false
    dim: false
    closePolicy: QQC.Popup.CloseOnEscape
    function present() {
        returnFocus = ownerWindow?.activeFocusItem || null
        open()
    }
    onClosed: {
        if (returnFocus) returnFocus.forceActiveFocus()
        if (!preview) AudioOutput.close()
    }
    Column {
        width: Math.min(Metrics.outputDetailsWidth, Math.max(0, (panel.parent?.width ?? Metrics.outputDetailsWidth) - Metrics.modalInsets - Metrics.gap24))
        spacing: Metrics.gap12
        Row {
            width: parent.width
            Text {
                width: parent.width - closeButton.width
                text: I18n.tr("Audio and output", I18n.rev)
                font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary
            }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: panel.close() }
        }
        SelectField {
            objectName: "audioInspectedOutput"
            width: parent.width
            options: (panel.document.outputs || []).map(o => o.label)
            values: (panel.document.outputs || []).map(o => o.id)
            details: (panel.document.outputs || []).map(o => o.detail)
            value: panel.document.selection || ""
            onPicked: value => { if (!panel.preview) AudioOutput.command("inspect", value) }
            Accessible.name: I18n.tr("Inspect an output", I18n.rev)
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            SegmentedControl {
                objectName: "audioOutputTabs"
                options: [{text:I18n.tr("Now", I18n.rev)}, {text:I18n.tr("Equipment", I18n.rev)}, {text:I18n.tr("Check", I18n.rev)}]
                current: Math.max(0, ["now", "equipment", "check"].indexOf(panel.page))
                onActivated: index => panel.page = ["now", "equipment", "check"][index]
            }
            PillButton {
                visible: panel.document.selection !== panel.document.current && !panel.document.selection?.startsWith("cast:")
                text: I18n.tr("Use this output", I18n.rev)
                enabled: !panel.document.busy && panel.document.canUse === true
                onClicked: if (!panel.preview) AudioOutput.command("use", panel.document.selection)
            }
        }
        Text {
            objectName: "audioOutputError"
            width: parent.width; visible: text !== ""; text: panel.document.error || ""
            textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.body; color: Theme.color.textPrimary
        }
        Flickable {
            id: scroll
            objectName: "audioOutputScroll"
            width: parent.width
            height: Math.max(Metrics.fieldHeightLarge, Math.min(Metrics.outputDetailsHeight, (panel.parent?.height ?? Metrics.outputDetailsHeight) - Metrics.modalInsets - Metrics.outputDetailsChrome))
            clip: true; boundsBehavior: Flickable.StopAtBounds
            contentWidth: width; contentHeight: body.height
            QQC.ScrollBar.vertical: QQC.ScrollBar {}
            Loader {
                id: body
                width: scroll.width
                sourceComponent: panel.page === "now" ? nowPage : panel.page === "check" ? checkPage : equipmentPage
            }
        }
    }
    Component { id: nowPage; AudioNow { document: panel.document.now || ({}) } }
    Component {
        id: equipmentPage
        AudioEquipment {
            document: panel.document
            onCommand: (action, argument) => { if (!panel.preview) AudioOutput.command(action, argument) }
            onCheckRequested: panel.page = "check"
            onSettingsRequested: {
                panel.close()
                Visit.keep("settings.focus", {section:"audio",key:"audio.hifiWizard"})
                Navigation.openSection("audio")
            }
        }
    }
    Component {
        id: checkPage
        AudioCheck {
            document: panel.document
            onCommand: (action, argument) => { if (!panel.preview) AudioOutput.command(action, argument) }
        }
    }
    Timer {
        objectName: "audioOutputRefresh"
        interval: Motion.refresh; repeat: true
        running: panel.opened && !panel.preview && panel.ownerWindow?.visible === true && panel.ownerWindow.visibility !== Window.Minimized
        onTriggered: AudioOutput.refresh()
    }
}

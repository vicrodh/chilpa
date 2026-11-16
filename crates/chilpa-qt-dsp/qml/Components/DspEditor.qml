import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import app.chilpa.dsp
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: editor
    property var state: ({})
    property var draft: ({})
    readonly property string responseData: JSON.stringify(state.curve || [])
    readonly property var response: JSON.parse(responseData)
    property string section: "graphic"
    property real maximumCurveHeight: Metrics.dspCurveHeight
    property string importName: ""
    property string importText: ""
    signal command(string action, var data)
    signal previewCommand(string action, var data)
    signal edited(var value)
    signal previewed(var value)
    signal sectionRequested(string section)
    signal findRequested(string query)
    signal importNameEdited(string value)
    signal importTextEdited(string value)
    implicitHeight: page.height
    Clipboard { id: clipboard }
    function changePreamp(gain, preview) {
        const next = JSON.parse(JSON.stringify(draft))
        next.preamp_db = gain
        preview ? previewed(next) : edited(next)
    }
    Flickable {
        id: viewport
        objectName: "dspEditorViewport"
        anchors.fill: parent
        contentWidth: editor.section === "parametric" ? Math.max(width, Metrics.dspFilterMinWidth) : width
        contentHeight: page.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        Controls.ScrollBar.horizontal: ViewportScrollBar {}
        Loader {
            id: page
            width: viewport.contentWidth
            height: item ? item.implicitHeight : 0
            sourceComponent: {
                switch (editor.section) {
                case "parametric": return parametricPage
                case "tone": return tonePage
                case "headroom": return headroomPage
                case "stereo": return stereoPage
                case "dynamics": return dynamicsPage
                case "manage": return personalPage
                case "assignments": return assignmentsPage
                case "import": return importPage
                default: return graphicPage
                }
            }
            onSourceComponentChanged: { viewport.contentX = 0; viewport.contentY = 0 }
        }
    }
    Component {
        id: graphicPage
        Column {
            spacing: Metrics.gap20
            DspGraphic {
                width: parent.width
                draft: editor.draft
                graphic: !!editor.state.graphic
                frequencies: editor.state.graphic_frequencies || []
                levels: editor.state.levels || []
                onEqChanged: on => { const next = JSON.parse(JSON.stringify(editor.draft)); next.eq_enabled = on; editor.edited(next) }
                onBandChanged: (index, gain) => editor.command("graphic_band", {index: index, gain: gain})
                onBandPreviewed: (index, gain) => editor.previewCommand("graphic_preview", {index: index, gain: gain})
                onStartRequested: editor.command("graphic_start", {})
                onAdvancedRequested: editor.sectionRequested("parametric")
                onResetRequested: editor.command("graphic_reset", {})
                onLayoutRequested: layout => editor.command("graphic_configure", layout)
            }
            PillButton { objectName: "dspRestorePrevious"; visible: !!editor.state.selection?.graphic_origin; text: I18n.tr("Restore previous EQ", I18n.rev); onClicked: editor.command("graphic_restore", {}) }
        }
    }
    Component {
        id: parametricPage
        DspEqualizer {
            draft: editor.draft
            curve: editor.response
            maximumCurveHeight: editor.maximumCurveHeight
            viewportHeight: viewport.height
            onEdited: value => editor.edited(value)
            onCommand: (action, data) => editor.command(action, data)
            onPreviewCommand: (action, data) => editor.previewCommand(action, data)
        }
    }
    Component {
        id: tonePage
        DspTone { draft: editor.draft; onEdited: value => editor.edited(value); onPreviewed: value => editor.previewed(value) }
    }
    Component {
        id: headroomPage
        Column {
            spacing: Metrics.gap24
            DspGain {
                width: parent.width
                preamp: Number(editor.draft.preamp_db || 0)
                headroom: Number(editor.state.headroom_db || 0)
                clipped: Number(editor.state.clipped || 0)
                automatic: editor.draft.auto_headroom !== false
                onCommitted: gain => editor.changePreamp(gain, false)
                onPreviewed: gain => editor.changePreamp(gain, true)
                onAutomaticChangedByUser: on => { const next = JSON.parse(JSON.stringify(editor.draft)); next.auto_headroom = on; editor.edited(next) }
                onResetMeter: editor.command("reset_meter", {})
            }
            DspMeters { width: parent.width; values: editor.state.meters || [0, 0, 0, 0]; onResetRequested: editor.command("reset_meter", {}) }
        }
    }
    Component { id: stereoPage; DspStereo { draft: editor.draft; onEdited: value => editor.edited(value) } }
    Component {
        id: dynamicsPage
        DspDynamics { draft: editor.draft; meters: editor.state.meters || [0, 0, 0, 0]; onEdited: value => editor.edited(value); onPreviewed: value => editor.previewed(value); onResetMeters: editor.command("reset_meter", {}) }
    }
    Component {
        id: personalPage
        DspPersonal { selection: editor.state.selection || ({}); presetName: editor.draft.name || ""; autosave: !!editor.state.autosave; onCommand: (action, data) => editor.command(action, data) }
    }
    Component {
        id: assignmentsPage
        DspAssignments { state: editor.state; onCommand: (action, data) => editor.command(action, data); onFindRequested: query => editor.findRequested(query) }
    }
    Component {
        id: importPage
        DspImport {
            nameText: editor.importName
            profileText: editor.importText
            onNameEdited: value => editor.importNameEdited(value)
            onProfileEdited: value => editor.importTextEdited(value)
            onSubmitted: (name, text) => editor.command("import", {name: name, text: text})
            onLibraryRequested: (name, text) => editor.command("import_many", {name: name, text: text})
            onExported: { const text = Dsp.exportPresets("[]"); if (text) clipboard.write(text) }
        }
    }
}

import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: equipment
    property var document: ({})
    signal command(string action, string argument)
    signal checkRequested()
    signal settingsRequested()
    spacing: Metrics.gap12
    AudioFact {
        visible: !!equipment.document.inspection && equipment.document.inspection.render_available === false
        heading: I18n.tr("Opening checks", I18n.rev)
        value: equipment.document.inspection?.identity?.backend === "cast"
            ? I18n.tr("A receiver uses network playback. Its physical DAC is not exposed here.", I18n.rev)
            : I18n.tr("This build can query the output but cannot render audio through it yet.", I18n.rev)
    }
    AudioFact {
        heading: I18n.tr("What this route reports", I18n.rev)
        value: equipment.document.status || ""
        note: I18n.tr("Inspecting does not select an output, reserve the DAC or start audio.", I18n.rev)
    }
    AudioFact {
        visible: equipment.document.inspection !== undefined && equipment.document.inspection !== null
        heading: I18n.tr("Identity and provenance", I18n.rev)
        value: equipment.document.inspection?.identity?.stable_id || ""
        note: equipment.document.inspection?.identity?.strong
            ? I18n.tr("Stable endpoint identity. Results also depend on its connection and configuration.", I18n.rev)
            : I18n.tr("Weak identity. Saved checks remain historical and cannot certify the connected unit.", I18n.rev)
    }
    Repeater {
        model: equipment.document.attributes || []
        AudioFact {
            required property var modelData
            heading: modelData.name
            value: modelData.value
            note: modelData.scope
        }
    }
    Repeater {
        model: equipment.document.formats || []
        AudioFact {
            required property var modelData
            heading: modelData.description
            value: modelData.scope
            note: I18n.tr("An announcement or driver query does not confirm opening or audible output.", I18n.rev)
        }
    }
    AudioFact {
        visible: !!equipment.document.inspection && !equipment.document.busy && !(equipment.document.formats || []).length
        heading: I18n.tr("Physical format combinations", I18n.rev)
        value: I18n.tr("No hardware format table is available for this route.", I18n.rev)
        note: I18n.tr("Separate input limits do not establish every combination of rate, word width and channels.", I18n.rev)
    }
    Flow {
        width: parent.width; spacing: Metrics.gap8
        PillButton {
            objectName: "audioEquipmentRefresh"
            text: I18n.tr("Read equipment details", I18n.rev)
            enabled: !equipment.document.busy
            onClicked: equipment.command("refresh", "")
        }
        PillButton {
            text: I18n.tr("Check a format…", I18n.rev)
            enabled: equipment.document.canCheck === true && !equipment.document.busy
            onClicked: equipment.checkRequested()
        }
        PillButton {
            visible: equipment.document.linuxDiagnostics === true
            text: I18n.tr("Linux audio diagnostics…", I18n.rev)
            onClicked: equipment.settingsRequested()
        }
    }
    Repeater {
        model: equipment.document.history || []
        AudioFact {
            required property var modelData
            heading: modelData.requested
            value: I18n.trArgs("Opening: {} · Audibility: {}", [modelData.status, modelData.audible], I18n.rev)
            note: (modelData.sameContext ? I18n.tr("Saved for the displayed inspection. Refresh after changing equipment or configuration.", I18n.rev)
                : I18n.tr("Historical result; the current device or configuration is not verified.", I18n.rev))
                + "\n" + Qt.formatDateTime(new Date(modelData.case.timestamp * 1000)) + "\n" + (modelData.case.error || "")
        }
    }
}

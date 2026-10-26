import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: check
    property var document: ({})
    signal command(string action, string argument)
    spacing: Metrics.gap12
    AudioFact {
        heading: I18n.tr("Check one format", I18n.rev)
        value: document.name || ""
        note: I18n.tr("This opens and closes the selected output without writing audio. Stop playback first. The queue, volume and desktop default stay as they are.", I18n.rev)
    }
    Flow {
        width: parent.width; spacing: Metrics.gap8
        SelectField {
            id: format; objectName: "audioCheckFormat"
            options: ["PCM 16", "PCM 24", "PCM 24/32", "PCM 32", "Float 32"]
            values: ["s16", "s24_packed", "s24_in32", "s32", "f32"]
            value: "s16"; enabled: !check.document.busy
            onPicked: value => format.value = value
            Accessible.name: I18n.tr("Sample format", I18n.rev)
        }
        SelectField {
            id: rate; objectName: "audioCheckRate"
            options: ["44.1 kHz", "48 kHz", "88.2 kHz", "96 kHz", "176.4 kHz", "192 kHz", "352.8 kHz", "384 kHz"]
            values: ["44100", "48000", "88200", "96000", "176400", "192000", "352800", "384000"]
            value: "44100"; enabled: !check.document.busy
            onPicked: value => rate.value = value
            Accessible.name: I18n.tr("Sample rate", I18n.rev)
        }
        SelectField {
            id: channels; objectName: "audioCheckChannels"
            options: ["1", "2", "4", "6", "8"]; values: options
            value: "2"; enabled: !check.document.busy
            onPicked: value => channels.value = value
            Accessible.name: I18n.tr("Channels", I18n.rev)
        }
        SelectField {
            id: mode; objectName: "audioCheckMode"
            options: [I18n.tr("Exclusive", I18n.rev), I18n.tr("Shared", I18n.rev)]; values: ["true", "false"]
            value: check.document.exclusiveRequested ? "true" : "false"; enabled: !check.document.busy
            onPicked: value => mode.value = value
            Accessible.name: I18n.tr("Opening mode", I18n.rev)
        }
    }
    AudioFact {
        heading: I18n.tr("Scope of this check", I18n.rev)
        value: I18n.tr("Opening and optional endpoint readback. Audibility remains untested.", I18n.rev)
        note: I18n.tr("A PCM carrier does not verify DoP. No tone or library track will play.", I18n.rev)
            + (check.document.inspection?.identity?.backend === "coreaudio" && mode.value === "false"
                ? "\n" + I18n.tr("Shared checks keep the current hardware configuration; the system may convert the input.", I18n.rev) : "")
    }
    Flow {
        width: parent.width; spacing: Metrics.gap8
        PillButton {
            objectName: "audioCheckStart"
            text: I18n.tr("Start silent check", I18n.rev)
            enabled: check.document.canCheck === true && !check.document.busy && !check.document.playing
            onClicked: check.command("check", JSON.stringify({output:check.document.selection || "",generation:check.document.inspection?.identity?.generation || "",
                format:{sample_format:format.value, sample_rate_hz:Number(rate.value), channels:Number(channels.value)}, exclusive:mode.value === "true"}))
        }
        PillButton {
            objectName: "audioCheckCancel"
            visible: check.document.busy === true && check.document.checking === true
            text: I18n.tr("Cancel", I18n.rev)
            onClicked: check.command("cancel", "")
        }
    }
    AudioFact {
        visible: !!check.document.last
        heading: I18n.tr("Requested and opened", I18n.rev)
        value: (check.document.lastRequested || "") + "\n" + (check.document.lastOpened || I18n.tr("No opening was confirmed", I18n.rev))
        note: check.document.lastReadback || I18n.tr("No physical readback is available for this route.", I18n.rev)
    }
    AudioFact {
        visible: !!check.document.last
        heading: I18n.tr("Physical format match", I18n.rev)
        value: check.document.lastPhysicalStatus || ""
        note: I18n.tr("Read at opening on the selected endpoint. This does not measure audible output.", I18n.rev)
    }
    AudioFact {
        visible: !!check.document.last
        heading: I18n.tr("Last check", I18n.rev)
        value: check.document.lastStatus || ""
        note: (check.document.last?.error || "") + "\n" + (check.document.saved ? I18n.tr("Result saved on this computer.", I18n.rev)
            : I18n.tr("The result has not been saved. Review the error before leaving.", I18n.rev))
    }
}

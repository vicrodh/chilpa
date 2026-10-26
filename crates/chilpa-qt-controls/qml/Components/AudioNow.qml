import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Column {
    id: now
    property var document: ({})
    spacing: Metrics.gap12
    AudioFact {
        objectName: "audioNowSummary"
        heading: I18n.tr("Current signal path", I18n.rev)
        value: now.document.summary || ""
    }
    AudioFact {
        visible: now.document.cast === true
        heading: I18n.tr("Reported by the receiver", I18n.rev)
        value: now.document.rendererStatus || ""
        note: now.document.rendererError || ""
    }
    Repeater {
        model: now.document.available ? [
            {heading:I18n.tr("Musical source", I18n.rev), value:now.document.source || ""},
            {heading:I18n.tr("Processing", I18n.rev), value:now.document.processing || ""},
            {heading:I18n.tr("Chilpa output", I18n.rev), value:now.document.output || ""},
            {heading:I18n.tr("Endpoint readback", I18n.rev), value:now.document.readback || "", note:now.document.readbackScope || ""}
        ] : []
        AudioFact {
            required property var modelData
            heading: modelData.heading
            value: modelData.value
            note: modelData.note || ""
        }
    }
    AudioFact {
        visible: now.document.available && !now.document.cast
        heading: I18n.tr("Exclusive mode", I18n.rev)
        value: now.document.exclusiveObserved === true ? I18n.tr("Acquired at opening", I18n.rev)
            : now.document.exclusiveObserved === false ? I18n.tr("Shared at opening", I18n.rev)
            : I18n.tr("Acquisition was not reported by this backend", I18n.rev)
        note: now.document.exclusiveRequested ? I18n.tr("Exclusive mode was requested.", I18n.rev) : I18n.tr("Shared mode was requested.", I18n.rev)
    }
}

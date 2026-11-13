import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Flow {
    id: context
    property var info: ({})
    property bool networkEnabled: true
    signal aboutRequested()
    spacing: Metrics.gap8
    Repeater {
        model: (context.info.genres || []).map(value => typeof value === "string" ? value : value.name || value.title || "").filter(Boolean).slice(0, Metrics.discoveryPreview)
        PillButton {
            required property string modelData
            text: modelData; kind: "ghost"
            onClicked: Navigation.goKey("genres", JSON.stringify({kind:"genre",name:modelData,trail:[modelData]}))
        }
    }
    PillButton {
        objectName: "artistCompositions"
        visible: (context.info.compositions || 0) > 0
        text: I18n.trCount("{} composition", "{} compositions", context.info.compositions || 0, I18n.rev)
        onClicked: Navigation.goKey("composers", JSON.stringify({kind:"composer",name:context.info.title || ""}))
    }
    PillButton { objectName: "artistAbout"; visible: context.networkEnabled; text: I18n.tr("About & connections", I18n.rev); kind: "ghost"; onClicked: context.aboutRequested() }
}

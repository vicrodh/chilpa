import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Flow {
    id: path
    property string mode: "genres"
    property string title: ""
    property var trail: []
    property var excludedSources: []
    function open(selected) {
        if (mode === "genres" && excludedSources.length) selected.excluded_sources = excludedSources
        if (Object.keys(selected).length) Navigation.goKey(mode, JSON.stringify(selected)); else Navigation.go(mode, 0)
    }
    spacing: Metrics.gap4
    PillButton { objectName: "discoveryRoot"; text: path.title; kind: "ghost"; onClicked: path.open({}) }
    Repeater {
        model: path.trail
        PillButton {
            required property string modelData
            required property int index
            kind: "ghost"; text: modelData; enabled: index < path.trail.length - 1
            maximumWidth: parent.width
            onClicked: path.open({kind:"genre",name:modelData,trail:path.trail.slice(0,index + 1)})
        }
    }
}

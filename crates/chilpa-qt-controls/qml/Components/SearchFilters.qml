import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Which sources a search asks and the quality it keeps, the same in the
// flyout and on the results page: the sources are kept between runs.
Column {
    id: filters
    required property SearchModel search
    // The sources a search asks, kept between runs: all of them to begin with.
    readonly property var chosen: search.source.split(",").filter(s => s !== "")
    function toggle(id) {
        let now = id === "all" ? (chosen.indexOf("all") >= 0 ? ["local"] : ["all"])
            : chosen.filter(s => s !== "all")
        if (id !== "all")
            now = now.indexOf(id) >= 0 ? now.filter(s => s !== id) : now.concat([id])
        search.source = (now.length > 0 ? now : ["local"]).join(",")
        SettingsModel.keep("ui.search.sources", search.source)
    }
    Component.onCompleted: search.source = SettingsModel.kept("ui.search.sources") || "all"
    spacing: Metrics.gap8

    Flickable {
        width: parent.width
        height: Metrics.chipHeight + Metrics.gap8
        contentWidth: sources.width
        clip: true; ScrollBar.horizontal: ViewportScrollBar {}
        Row {
            id: sources
            spacing: Metrics.gap6
            Repeater {
                model: [{id: "all", name: I18n.tr("All sources", I18n.rev)}, {id: "local", name: I18n.tr("Local", I18n.rev)}].concat(JSON.parse(Sources.snapshot || "{}").panels || [])
                Chip {
                    required property var modelData
                    text: modelData.name
                    active: filters.chosen.indexOf(modelData.id) >= 0 || (modelData.id !== "all" && filters.chosen.indexOf("all") >= 0)
                    onClicked: filters.toggle(modelData.id)
                }
            }
        }
    }
    Row {
        spacing: Metrics.gap6
        Repeater {
            model: [{value: "", label: I18n.tr("Any quality", I18n.rev)},
                {value: "lossless", label: I18n.tr("Lossless only", I18n.rev)},
                {value: "hi_res", label: I18n.tr("Hi-Res only", I18n.rev)}]
            Chip {
                required property var modelData
                text: modelData.label
                active: filters.search.qualityFilter === modelData.value
                onClicked: filters.search.qualityFilter = modelData.value
            }
        }
        // What a file's advisory or a source calls explicit, left out.
        Chip {
            objectName: "hideExplicit"
            text: I18n.tr("Hide explicit", I18n.rev)
            active: filters.search.hideExplicit
            onClicked: filters.search.hideExplicit = !filters.search.hideExplicit
        }
    }
}

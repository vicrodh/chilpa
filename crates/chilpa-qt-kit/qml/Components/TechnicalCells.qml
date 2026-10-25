import QtQuick
import app.chilpa.kit
import "../Theme"

Item {
    id: cells
    property var facts: ({})
    property string title
    property string artist
    property string year
    property bool header: false
    TechnicalColumns { id: columns; tableWidth: cells.width }
    Rectangle { anchors.fill: parent; visible: cells.header; color: Theme.color.surfaceWindow }
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; visible: cells.header; color: Theme.color.strokeHairline }
    Repeater {
        model: columns.columns
        Text {
            required property var modelData
            objectName: (cells.header ? "technicalHeader_" : "technicalCell_") + modelData.key
            x: modelData.x + Metrics.gap12
            width: modelData.width - Metrics.gap12 * 2 // derived: an inset on each side
            anchors.verticalCenter: parent.verticalCenter
            text: cells.header ? modelData.label : (cells.facts.mixed_fields || []).indexOf(modelData.key) >= 0
                ? I18n.tr("Mixed", I18n.rev) : columns.display(modelData.key, modelData.key === "album" ? cells.title
                    : modelData.key === "artist" ? cells.artist : modelData.key === "year" && cells.facts.year === undefined ? cells.year : cells.facts[modelData.key])
            elide: Text.ElideRight
            font.pointSize: cells.header ? Type.label : Type.row
            font.weight: cells.header ? Type.medium : Type.regular
            font.features: { "tnum": 1 }
            color: cells.header || modelData.key !== "album" ? Theme.color.textSecondary : Theme.color.textPrimary
        }
    }
}

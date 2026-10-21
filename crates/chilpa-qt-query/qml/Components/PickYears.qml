import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// Years (F3): the decades and "This year" as presets, a free range with
// either end open, and "No year".
Column {
    id: years
    // The year range the query holds: {from, to} or null.
    property var range: null
    property bool none: false
    signal edit(var change)
    readonly property int thisYear: new Date().getFullYear()
    readonly property var decades: [1950, 1960, 1970, 1980, 1990, 2000, 2010, 2020]
    spacing: Refine.pickApart

    Flow {
        width: parent.width
        spacing: Refine.chipApart
        Repeater {
            model: years.decades
            Pill {
                required property int modelData
                text: modelData + "s"
                on: !!years.range && years.range.from === modelData && years.range.to === modelData + 9
                onClicked: years.edit({ op: "range", field: "year", from: modelData, to: modelData + 9 })
            }
        }
        Pill {
            text: I18n.tr("This year", I18n.rev)
            on: !!years.range && years.range.from === years.thisYear && years.range.to === years.thisYear
            onClicked: years.edit({ op: "range", field: "year", from: years.thisYear, to: years.thisYear })
        }
    }
    Row {
        spacing: Refine.headApart
        component YearField: Rectangle {
            id: box
            property alias text: input.text
            property string placeholder
            signal settled(string value)
            width: Refine.yearField
            height: Refine.yearFieldTall
            radius: Refine.pickSearchRound
            color: Theme.color.fillField
            TextInput {
                id: input
                anchors.fill: parent
                anchors.leftMargin: Refine.pickPad
                verticalAlignment: TextInput.AlignVCenter
                font.pointSize: Type.body
                font.features: { "tnum": 1 }
                color: Theme.color.textPrimary
                validator: RegularExpressionValidator { regularExpression: /^\d{0,4}$/ }
                onTextEdited: if (text.length === 4 || text === "") box.settled(text)
                Text { anchors.fill: parent; verticalAlignment: Text.AlignVCenter; visible: input.text === ""; text: box.placeholder; font: input.font; color: Theme.color.textTertiary }
            }
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("From", I18n.rev); font.pointSize: Type.row; color: Theme.color.textSecondary }
        YearField {
            objectName: "yearFrom"
            anchors.verticalCenter: parent.verticalCenter
            text: years.range && years.range.from ? String(years.range.from) : ""
            placeholder: "1950"
            onSettled: (value) => years.edit({ op: "range", field: "year", from: value === "" ? null : Number(value), to: years.range ? years.range.to : null })
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("To", I18n.rev); font.pointSize: Type.row; color: Theme.color.textSecondary }
        YearField {
            objectName: "yearTo"
            anchors.verticalCenter: parent.verticalCenter
            text: years.range && years.range.to ? String(years.range.to) : ""
            placeholder: String(years.thisYear)
            onSettled: (value) => years.edit({ op: "range", field: "year", from: years.range ? years.range.from : null, to: value === "" ? null : Number(value) })
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Leave one empty for an open end. Both ends included.", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary }
    }
    PickRow {
        width: parent.width
        text: RefineText.noneLabel("year")
        italic: true
        actions: false
        mark: years.none ? "include" : "none"
        onIncluded: years.edit({ op: "none", field: "year", on: !years.none })
    }
}

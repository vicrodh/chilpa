import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// A field's values, inside the panel and never as a popup (F3): the fields
// down the left (wide) or as a strip of chips (narrow), the field's title
// with a search for lists, its values as the field wants them (a list with
// counts, years, stars, recency, words), and Done. Edits go up as the
// facade's JSON; the picker keeps nothing.
Item {
    id: picker
    property string field
    property bool narrow: false
    // The rows of the query, or of the group new conditions go into.
    property var rows: []
    signal edit(var change)
    signal fieldChosen(string field)
    signal closed()

    readonly property bool isList: ["genre", "artist", "composer", "label"].indexOf(field) >= 0
    function find(kind, negated) { return rows.find(r => r.field === field && r.kind === kind && (negated === undefined || !!r.negated === negated)) || null }
    readonly property var included: (find("list", false) || { values: [] }).values
    readonly property var excluded: (find("list", true) || { values: [] }).values
    readonly property bool none: !!find("none")
    readonly property var addedChoices: [[0, ""], [7, "d"], [30, "d"], [6, "w"], [3, "m"], [1, "y"]]

    implicitHeight: box.implicitHeight

    ElevatedShadow { target: box; radius: Refine.pickRound }
    Rectangle {
        id: box
        width: parent.width
        implicitHeight: Math.max(fieldsView.visible ? fieldsView.height : 0, side.implicitHeight) + Refine.pickPad * 2 // derived: padding above and below
        radius: Refine.pickRound
        color: Theme.color.surfaceRaised
        border.width: Metrics.hairline
        border.color: Theme.color.strokeHairline

        // As tall as the design's nine fields; the ones after them scroll.
        Flickable {
            id: fieldsView
            visible: !picker.narrow
            x: Refine.pickPad
            y: Refine.pickPad
            width: Refine.pickFieldsWide
            height: Math.min(fields.implicitHeight, (Refine.pickFieldRow + Metrics.hairline) * Refine.pickFieldsShown)
            contentHeight: fields.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: fields
                width: parent.width
                spacing: Metrics.hairline
                Repeater {
                    model: RefineText.order
                    Rectangle {
                        required property string modelData
                        width: fields.width - Refine.pickPad
                        height: Refine.pickFieldRow
                        radius: Refine.pickFieldRound
                        color: modelData === picker.field ? Theme.color.stateSelected : fieldArea.containsMouse ? Theme.color.stateHover : "transparent"
                        Row {
                            x: Refine.pickPad
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: Refine.pickPad
                            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: RefineText.glyph(modelData); size: Refine.pickFieldIcon; color: Theme.color.textSecondary }
                            Text { anchors.verticalCenter: parent.verticalCenter; text: RefineText.field(modelData); font.pointSize: Type.row; font.weight: modelData === picker.field ? Type.semibold : Type.regular; color: Theme.color.textPrimary }
                        }
                        MouseArea { id: fieldArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: picker.fieldChosen(modelData) }
                    }
                }
            }
        }
        Rectangle { visible: !picker.narrow; x: Refine.pickPad + Refine.pickFieldsWide; y: Refine.pickPad; width: Metrics.hairline; height: box.height - Refine.pickPad * 2; color: Theme.color.strokeHairline } // derived: the divider between fields and values
        Column {
            id: side
            x: picker.narrow ? Refine.pickPad : Refine.pickPad + Refine.pickFieldsWide + Refine.pickApart
            y: Refine.pickPad
            width: box.width - x - Refine.pickPad
            spacing: Refine.footApart
            Flickable {
                visible: picker.narrow
                width: parent.width
                height: Refine.narrowChipTall
                contentWidth: strip.implicitWidth
                clip: true
                Row {
                    id: strip
                    spacing: Refine.fieldChipsApart
                    Repeater {
                        model: RefineText.order
                        FieldChip { required property string modelData; height: Refine.narrowChipTall; radius: Refine.narrowChipRound; glyph: RefineText.glyph(modelData); text: RefineText.field(modelData); active: modelData === picker.field; onClicked: picker.fieldChosen(modelData) }
                    }
                }
            }
            Item {
                width: parent.width
                height: Refine.pickSearchTall
                Row {
                    id: title
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Refine.headApart
                    Icon { anchors.verticalCenter: parent.verticalCenter; glyph: RefineText.glyph(picker.field); size: Refine.pickTitleIcon; color: Theme.color.textSecondary }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: RefineText.field(picker.field); font.pointSize: Type.bodyLarge; font.weight: Type.heading; color: Theme.color.textPrimary }
                }
                Rectangle {
                    visible: picker.isList
                    x: title.width + Refine.headApart
                    width: parent.width - x - Refine.rowButton - Refine.headApart
                    height: Refine.pickSearchTall
                    radius: Refine.pickSearchRound
                    color: Theme.color.fillField
                    Icon { id: lens; x: Refine.pickPad; anchors.verticalCenter: parent.verticalCenter; glyph: Icons.search; size: Refine.fieldChipIcon; color: Theme.color.textTertiary }
                    TextInput {
                        // Not "search": PickValues has a property by that name.
                        id: typed
                        objectName: "pickSearch"
                        anchors.left: lens.right
                        anchors.leftMargin: Refine.innerApart
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        clip: true
                        font.pointSize: Type.row
                        color: Theme.color.textPrimary
                        Text { anchors.fill: parent; visible: typed.text === ""; text: RefineText.field(picker.field) + "…"; font: typed.font; color: Theme.color.textTertiary }
                    }
                }
                SquareButton { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; small: true; glyph: Icons.x; tip: I18n.tr("Done", I18n.rev); onClicked: picker.closed() }
            }
            Loader {
                width: parent.width
                sourceComponent: picker.isList ? listBody : picker.field === "year" ? yearBody : picker.field === "rating" ? ratingBody
                    : picker.field === "added" ? addedBody : picker.field === "plays" ? playsBody : picker.field === "type" || picker.field === "quality" ? choicesBody : wordsBody
            }
            Item {
                width: parent.width
                height: Refine.textButtonTall
                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - done.width - Refine.pickApart; text: I18n.tr("Click a value to include it, − to exclude it. Counts already apply everything else.", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary; wrapMode: Text.WordWrap }
                Rectangle {
                    id: done
                    objectName: "pickDone"
                    anchors.right: parent.right
                    width: doneText.implicitWidth + Refine.doneWide * 2 // derived: padding on both sides
                    height: Refine.textButtonTall
                    radius: Refine.pillRound
                    color: Theme.color.fillField
                    Text { id: doneText; anchors.centerIn: parent; text: I18n.tr("Done", I18n.rev); font.pointSize: Type.row; font.weight: Type.medium; color: Theme.color.textPrimary }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: picker.closed() }
                }
            }
        }
    }
    Component { id: listBody; PickValues { field: picker.field; search: typed.text; included: picker.included; excluded: picker.excluded; none: picker.none; onEdit: (change) => picker.edit(change) } }
    Component { id: yearBody; PickYears { range: picker.find("range"); none: picker.none; onEdit: (change) => picker.edit(change) } }
    Component { id: ratingBody; PickRatings { rating: picker.find("rating"); onEdit: (change) => picker.edit(change) } }
    Component {
        id: addedBody
        PickPresets {
            readonly property var held: picker.find("added")
            lead: I18n.tr("in the last", I18n.rev)
            choices: picker.addedChoices.map(c => ({ text: c[0] === 0 ? I18n.tr("Any time", I18n.rev) : RefineText.unit(c[0], c[1]),
                on: c[0] === 0 ? !held : !!held && held.n === c[0] && held.unit === c[1],
                change: c[0] === 0 ? (held ? { op: "remove", id: held.id } : null) : { op: "added", n: c[0], unit: c[1] } }))
            onEdit: (change) => picker.edit(change)
        }
    }
    Component {
        id: playsBody
        PickPresets {
            readonly property var held: picker.find("range")
            choices: [{ text: I18n.tr("Never played", I18n.rev), on: !!held && held.from === 0 && held.to === 0, change: { op: "range", field: "plays", from: 0, to: 0 } },
                      { text: I18n.tr("Played", I18n.rev), on: !!held && held.from === 1 && !held.to, change: { op: "range", field: "plays", from: 1, to: null } },
                      { text: I18n.tr("Played 10 times or more", I18n.rev), on: !!held && held.from === 10 && !held.to, change: { op: "range", field: "plays", from: 10, to: null } }]
            onEdit: (change) => picker.edit(change)
        }
    }
    Component {
        id: choicesBody
        Column {
            Repeater {
                model: picker.field === "type" ? ["album", "ep", "single", "live", "compilation", "soundtrack"] : ["lossy", "cd", "hires", "dsd"]
                PickRow {
                    required property string modelData
                    width: parent.width
                    text: RefineText.value(picker.field, modelData)
                    mark: picker.included.indexOf(modelData) >= 0 ? "include" : picker.excluded.indexOf(modelData) >= 0 ? "exclude" : "none"
                    onIncluded: picker.edit({ op: "value", field: picker.field, value: modelData, state: mark === "include" ? "none" : "include" })
                    onExcluded: picker.edit({ op: "value", field: picker.field, value: modelData, state: mark === "exclude" ? "none" : "exclude" })
                }
            }
        }
    }
    Component { id: wordsBody; PickWords { field: picker.field; onEdit: (change) => picker.edit(change) } }
}

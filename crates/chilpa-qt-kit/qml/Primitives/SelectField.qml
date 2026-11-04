import QtQuick
import "../Theme"

// A field showing the chosen option, opening a Menu of the others with a
// check on the chosen one (design/03 SelectField). `marks` puts a badge on
// an option, as the bit-perfect "BP" on an output. `rich` options say more:
// the menu is as wide as the field, each option wears its source in the
// gutter and its detail wrapped whole under it, and while it is open the
// field keeps the keyboard: Up and Down move over the options that can be
// taken, Enter or Space takes one, Escape closes it.
Rectangle {
    id: field

    property var options: []
    property var values: []
    property var marks: []
    property var enabledOptions: []
    property var sources: []
    property var details: []
    // What the closed field says instead of the whole option, where the
    // option is long: the field is then as wide as that.
    property var briefs: []
    property string placeholder: ""
    property var explicitOptions: []
    property bool explicitContent: false
    property var aiOptions: []
    property bool aiGenerated: false
    property string value
    property real menuWidthLimit: Metrics.menuMaxWidth
    property bool rich: false
    // The option the keyboard is on while a rich menu is open.
    property int highlight: -1

    signal picked(string value)

    readonly property int current: values.indexOf(value)

    // The next option from `from` that can be taken, going by `step`; -1
    // when there is none.
    function nextEnabled(from, step) {
        for (let next = from + step; next >= 0 && next < field.values.length; next += step) {
            if (field.enabledOptions[next] !== false)
                return next
        }
        return -1
    }
    function show() {
        field.highlight = field.current
        menu.open()
    }

    implicitWidth: briefs.length > 0
        ? Metrics.gap10 + sourceBadge.width + Metrics.gap6 + said.implicitWidth + Metrics.gap6 + chevron.width + Metrics.gap8 // derived: what it holds and its margins
        : Metrics.selectWidth
    implicitHeight: Metrics.fieldHeightLarge
    radius: Metrics.radiusControl
    color: area.containsMouse ? Theme.color.stateSelected : Theme.color.fillField

    SourceIcon {
        id: sourceBadge
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        visible: field.sources.length > 0
        width: visible ? implicitWidth : 0
        sourceInfo: field.sources[field.current] || ({})
    }

    AdvisoryTitle {
        explicitContent: field.explicitContent
        aiGenerated: field.aiGenerated
        id: said
        anchors.left: sourceBadge.right
        anchors.leftMargin: sourceBadge.visible ? Metrics.gap6 : 0
        anchors.right: chevron.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        // `values` and `options` can arrive one after the other when the row
        // changes in place; for that moment the index may be past `options`.
        text: field.briefs[field.current] ?? field.options[field.current] ?? field.placeholder
        elide: Text.ElideRight
        font.pointSize: Type.body
        color: field.enabledOptions[field.current] === false ? Theme.color.textTertiary : Theme.color.textPrimary
    }

    Icon {
        id: chevron
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronDown
        size: Metrics.iconSm
    }

    // Tab reaches it, Space or Enter opens it, and Up and Down step through
    // the options without opening it, as QBZ's select.
    activeFocusOnTab: enabled
    Keys.onPressed: (event) => {
        if (!field.enabled || event.isAutoRepeat)
            return
        const step = event.key === Qt.Key_Up ? -1 : event.key === Qt.Key_Down ? 1 : 0
        const take = event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter
        if (field.rich && menu.opened) {
            const next = step !== 0 ? field.nextEnabled(field.highlight, step) : -1
            if (next >= 0)
                field.highlight = next
            else if (take && field.highlight >= 0 && field.enabledOptions[field.highlight] !== false) {
                menu.close()
                field.picked(field.values[field.highlight])
            } else if (event.key === Qt.Key_Escape)
                menu.close()
            else if (step === 0)
                return
        } else if (take)
            menu.opened ? menu.close() : field.show()
        else if (step !== 0 && field.values.length > 0) {
            const next = field.nextEnabled(field.current, step)
            if (next >= 0)
                field.picked(field.values[next])
        }
        else
            return
        event.accepted = true
    }
    FocusRing {}

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        onClicked: {
            TipState.hide(field)
            field.show()
        }
        // A rich field shows its whole option already.
        onContainsMouseChanged: containsMouse && field.sources.length > 0 && !field.rich
            ? TipState.show(field.options[field.current] || "", field) : TipState.hide(field)
    }

    Menu {
        id: menu
        objectName: "selectOptions"
        // A rich menu leaves the keyboard on the field, and is short enough
        // to need no search.
        focus: !field.rich
        searchable: !field.rich && field.options.length > 5
        widthLimit: field.rich ? field.width : field.menuWidthLimit
        width: field.rich ? field.width : implicitWidth
        y: field.height + Metrics.gap4
        x: field.width - width
        entries: field.options.map((text, i) => ({
            text: text,
            explicitContent: field.explicitOptions[i] === true,
            aiGenerated: field.aiOptions[i] === true,
            enabled: field.enabledOptions[i] !== false,
            checked: i === field.current,
            rich: field.rich,
            focused: field.rich && i === field.highlight,
            leadingSource: field.rich ? field.sources[i] : undefined,
            sourceInfo: field.rich ? undefined : field.sources[i],
            detail: field.details[i] || "",
            mark: field.marks.length > i ? field.marks[i] : ""
        }))
        onChosen: (index) => field.picked(field.values[index])
    }
}

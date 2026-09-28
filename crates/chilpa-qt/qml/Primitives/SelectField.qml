import QtQuick
import "../Theme"

// A field showing the chosen option, opening a Menu of the others with a
// check on the chosen one (design/03 SelectField). `marks` puts a badge on
// an option, as the bit-perfect "BP" on an output.
Rectangle {
    id: field

    property var options: []
    property var values: []
    property var marks: []
    property var sources: []
    property var details: []
    property string value
    property real menuWidthLimit: Metrics.menuMaxWidth

    signal picked(string value)

    readonly property int current: values.indexOf(value)

    implicitWidth: Metrics.selectWidth
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

    Text {
        anchors.left: sourceBadge.right
        anchors.leftMargin: sourceBadge.visible ? Metrics.gap6 : 0
        anchors.right: chevron.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        // `values` and `options` can arrive one after the other when the row
        // changes in place; for that moment the index may be past `options`.
        text: field.options[field.current] ?? ""
        elide: Text.ElideRight
        font.pointSize: Type.body
        color: Theme.color.textPrimary
    }

    Icon {
        id: chevron
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronDown
        size: Metrics.iconSm
    }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        onClicked: {
            TipState.hide(field)
            menu.open()
        }
        onContainsMouseChanged: containsMouse && field.sources.length > 0
            ? TipState.show(field.options[field.current] || "", field) : TipState.hide(field)
    }

    Menu {
        id: menu
        widthLimit: field.menuWidthLimit
        y: field.height + Metrics.gap4
        x: field.width - width
        entries: field.options.map((text, i) => ({
            text: text,
            checked: i === field.current,
            sourceInfo: field.sources[i],
            detail: field.details[i] || "",
            mark: field.marks.length > i ? field.marks[i] : ""
        }))
        onChosen: (index) => field.picked(field.values[index])
    }
}

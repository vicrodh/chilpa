import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The pages of a long list: back, the first page, the ones around the
// current, the last, and forward, with an ellipsis where pages are left
// out. The current one is lit.
Row {
    id: pager

    property int page: 1
    property int pages: 1
    // The pages shown, in order; 0 stands for the ones left out.
    readonly property var shown: {
        const out = []
        for (let p = 1; p <= pages; ++p) {
            if (p === 1 || p === pages || Math.abs(p - page) <= 1)
                out.push(p)
            else if (out[out.length - 1] !== 0)
                out.push(0)
        }
        return out
    }

    signal chosen(int page)

    spacing: Metrics.gap4

    IconButton {
        objectName: "pagerBack"
        anchors.verticalCenter: parent.verticalCenter
        size: Charts.pagerButton
        glyph: Icons.chevronLeft
        enabled: pager.page > 1
        tip: I18n.tr("Previous page", I18n.rev)
        onClicked: pager.chosen(pager.page - 1)
    }
    Repeater {
        model: pager.shown

        Rectangle {
            id: number
            required property int modelData
            readonly property bool current: modelData === pager.page
            objectName: "page" + modelData
            width: Math.max(Charts.pagerButton, label.implicitWidth + 2 * Metrics.gap8) // derived: padding on both sides
            height: Charts.pagerButton
            radius: Metrics.radiusControl
            color: current ? Theme.color.accent : area.containsMouse && modelData > 0 ? Theme.color.stateHover : "transparent"

            Text {
                id: label
                anchors.centerIn: parent
                text: number.modelData > 0 ? number.modelData : "…"
                font.pointSize: Type.body
                font.weight: number.current ? Type.semibold : Type.regular
                font.features: { "tnum": 1 }
                color: number.current ? Theme.color.textOnAccent : Theme.color.textSecondary
            }
            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                enabled: number.modelData > 0 && !number.current
                cursorShape: Qt.PointingHandCursor
                onClicked: pager.chosen(number.modelData)
            }
        }
    }
    IconButton {
        objectName: "pagerForward"
        anchors.verticalCenter: parent.verticalCenter
        size: Charts.pagerButton
        glyph: Icons.chevronRight
        enabled: pager.page < pager.pages
        tip: I18n.tr("Next page", I18n.rev)
        onClicked: pager.chosen(pager.page + 1)
    }
}

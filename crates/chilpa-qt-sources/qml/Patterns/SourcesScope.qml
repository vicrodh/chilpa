import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// Which sources count in the Library (F8): the layers glyph, the sources on
// by name, and a popover with a switch each. Sources decide what is in the
// Library; Refine narrows inside them. The settings popover keeps its own
// copy of the switches.
Item {
    id: scope
    readonly property bool open: popup.opened
    implicitWidth: line.implicitWidth + Metrics.gap8 * 2 // derived: padding on both sides
    implicitHeight: Metrics.button28

    SourceRows { id: source; active: scope.visible }

    Rectangle {
        objectName: "sourcesScope"
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: popup.opened ? Theme.color.stateSelected : hover.hovered ? Theme.color.stateHover : "transparent"
        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: popup.opened ? popup.close() : popup.open() }
        ToolTip.visible: hover.hovered && !popup.opened
        ToolTip.text: I18n.tr("Which sources count", I18n.rev)
        Row {
            id: line
            anchors.centerIn: parent
            spacing: Metrics.gap6
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.stack2; size: Metrics.iconMd; color: Theme.color.textSecondary }
            Text { objectName: "sourcesLabel"; anchors.verticalCenter: parent.verticalCenter; text: source.label; font.pointSize: Type.row; font.weight: Type.medium; color: Theme.color.textPrimary }
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.chevronDown; size: Metrics.sourcesScopeChevron; color: Theme.color.textTertiary }
        }
    }

    Popup {
        id: popup
        objectName: "sourcesScopePopup"
        // A click outside closes it and is nothing else.
        modal: true
        dim: false
        // Escape closes it.
        focus: true
        y: scope.height + Metrics.gap6
        width: Metrics.sourcesScopeWidth
        padding: Metrics.gap6
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

        background: Item {
            Hold {}
            RectangularShadow { anchors.fill: panel; radius: panel.radius; offset.y: Metrics.shadowElevatedY; blur: Metrics.shadowElevatedBlur; color: Theme.color.shadowElevated }
            RectangularShadow { anchors.fill: panel; radius: panel.radius; offset.y: Metrics.shadowKeyY; blur: Metrics.shadowKeyBlur; color: Theme.color.shadowElevatedKey }
            Rectangle { id: panel; anchors.fill: parent; radius: Metrics.radiusMenu; color: Theme.color.surfaceRaised; Edge {} }
        }

        contentItem: Column {
            spacing: Metrics.hairline * 2 // derived: the design's 2 px between rows
            Text {
                leftPadding: Metrics.gap8
                topPadding: Metrics.gap4
                bottomPadding: Metrics.gap6
                text: I18n.tr("Which sources count", I18n.rev)
                font.pointSize: Type.label
                font.weight: Type.semibold
                color: Theme.color.textTertiary
            }
            Repeater {
                model: source.rows
                Rectangle {
                    required property var modelData
                    objectName: "sourcesScopeRow"
                    width: popup.availableWidth
                    height: Metrics.button40
                    radius: Metrics.radiusControl
                    color: rowHover.hovered ? Theme.color.stateHover : "transparent"
                    HoverHandler { id: rowHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: source.flip(modelData, !modelData.on) }
                    SourceIcon {
                        id: glyph
                        x: Metrics.gap8
                        anchors.verticalCenter: parent.verticalCenter
                        width: Metrics.iconNav
                        height: Metrics.iconNav
                        sourceInfo: ({ icon: modelData.icon, local: modelData.key === "local:default" })
                    }
                    Column {
                        anchors.left: glyph.right
                        anchors.leftMargin: Metrics.gap10
                        anchors.right: toggle.left
                        anchors.rightMargin: Metrics.gap10
                        anchors.verticalCenter: parent.verticalCenter
                        Text { width: parent.width; text: modelData.label; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
                        Text {
                            text: modelData.count >= 0 ? I18n.trCount("{} track", "{} tracks", modelData.count, I18n.rev) : modelData.on ? I18n.tr("On", I18n.rev) : I18n.tr("Off", I18n.rev)
                            font.pointSize: Type.small
                            color: Theme.color.textSecondary
                        }
                    }
                    Toggle {
                        id: toggle
                        objectName: "sourcesScope-" + modelData.key
                        compact: true
                        anchors.right: parent.right
                        anchors.rightMargin: Metrics.gap8
                        anchors.verticalCenter: parent.verticalCenter
                        checked: modelData.on
                        onToggled: (on) => source.flip(modelData, on)
                    }
                }
            }
            Text {
                width: popup.availableWidth
                leftPadding: Metrics.gap8
                rightPadding: Metrics.gap8
                topPadding: Metrics.gap6
                bottomPadding: Metrics.gap4
                text: I18n.tr("Sources decide what is in the Library. Refine narrows inside them.", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
                wrapMode: Text.WordWrap
            }
        }
    }
}

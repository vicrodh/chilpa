import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// What's new's last page, QBZ's: the version's release notes as the facade
// read them, blocks it already laid out (0 section, 1 bullet, 2 paragraph,
// 3 a line that is a link) under the sections as a contents list that, as in
// QBZ, does not jump. While they are read it says so; without them, that
// they are not available.
Item {
    id: notes

    property var doc: ({})
    readonly property bool loading: doc.loading === true
    readonly property bool ready: !loading && doc.hasBody === true

    Text {
        anchors.centerIn: parent
        visible: notes.loading
        text: I18n.tr("Loading…", I18n.rev)
        color: Theme.color.textTertiary
        font.pointSize: Type.body
    }
    Text {
        objectName: "notesMissing"
        anchors.centerIn: parent
        visible: !notes.loading && !notes.ready
        width: Math.min(parent.width, Metrics.confirmWidth)
        text: I18n.tr("Release notes are not available.", I18n.rev)
        color: Theme.color.textTertiary
        font.pointSize: Type.body
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
    }

    Flickable {
        id: flick
        anchors.fill: parent
        visible: notes.ready
        clip: true
        contentWidth: width
        contentHeight: column.height
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}

        Column {
            id: column
            // Prose reads at a book's measure; a narrow window keeps its width.
            width: Math.min(flick.width, OnboardingFrame.notesWidest)
            x: Math.round((flick.width - width) / 2) // derived: centered

            Column {
                width: parent.width
                spacing: OnboardingFrame.tocGap
                visible: (notes.doc.toc || []).length > 0
                Repeater {
                    model: notes.doc.toc || []
                    delegate: Rectangle {
                        required property var modelData
                        height: OnboardingFrame.tocChip
                        width: label.implicitWidth + OnboardingFrame.tocPad * 2 // derived: padding both sides
                        radius: Metrics.radiusControl
                        border.width: 1
                        border.color: Theme.color.strokeHairline
                        color: Theme.color.surfaceRaised
                        Text {
                            id: label
                            anchors.left: parent.left
                            anchors.leftMargin: OnboardingFrame.tocPad
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.label || ""
                            color: Theme.color.textSecondary
                            font.pointSize: OnboardingFrame.deepBulletSize
                            font.weight: Type.medium
                        }
                    }
                }
                Item { width: 1; height: Metrics.gap8 }
                Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }
                Item { width: 1; height: Metrics.gap8 }
            }

            Repeater {
                model: notes.doc.blocks || []
                delegate: Item {
                    id: block
                    required property var modelData
                    readonly property int kind: modelData.kind || 0
                    readonly property int level: modelData.level || 0
                    readonly property bool deep: kind === 1 && level >= 2
                    readonly property int above: kind === 0 ? OnboardingFrame.sectionTop : 0
                    width: column.width
                    height: line.implicitHeight + above + (kind === 0 ? OnboardingFrame.sectionBottom : OnboardingFrame.blockBottom)

                    Row {
                        id: line
                        y: block.above
                        x: block.kind === 1 ? block.level * OnboardingFrame.bulletIndent : 0
                        width: block.width - x
                        spacing: Metrics.gap8
                        Text {
                            id: glyph
                            visible: block.kind === 1
                            text: block.deep ? "◦" : "•"
                            color: block.deep ? Theme.color.textTertiary : Theme.color.textSecondary
                            font.pointSize: block.deep ? OnboardingFrame.deepBulletSize : OnboardingFrame.bulletSize
                        }
                        Text {
                            width: line.width - (glyph.visible ? glyph.width + line.spacing : 0)
                            text: block.modelData.text || ""
                            wrapMode: Text.WordWrap
                            color: block.kind === 3 ? (link.containsMouse ? Theme.color.accentHover : Theme.color.accent)
                                 : block.kind === 0 ? Theme.color.textPrimary
                                 : block.deep ? Theme.color.textTertiary : Theme.color.textSecondary
                            font.pointSize: block.kind === 0 ? OnboardingFrame.sectionSize
                                          : block.kind === 1 ? (block.deep ? OnboardingFrame.deepBulletSize : OnboardingFrame.bulletSize)
                                          : OnboardingFrame.paragraphSize
                            font.weight: block.kind === 0 ? Type.bold : block.kind === 3 ? Type.medium : Font.Normal // derived: kinds 0 and 3, a section and a link
                            MouseArea {
                                id: link
                                anchors.fill: parent
                                enabled: block.kind === 3
                                visible: enabled
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Qt.openUrlExternally(block.modelData.url || "")
                            }
                        }
                    }
                }
            }
            Item { width: 1; height: Metrics.gap8 }
        }
    }
}

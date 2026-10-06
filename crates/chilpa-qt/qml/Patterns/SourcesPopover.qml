import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

// Library Explorer's provenance and encoded-file filters. Sources are read
// from the library; tiers and formats use the facade's generic vocabulary.
Popup {
    id: popover
    property bool showAlbumLibraries: false
    property bool showTrackLibraries: false

    // A click outside closes it and is nothing else: it is not also a click
    // on what is there.
    modal: true
    dim: false

    width: Metrics.sourcesPopoverWidth
    padding: Metrics.gap12
    margins: Metrics.gap10
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Item {
        Hold {}
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised
            Edge {}
        }
    }

    contentItem: Column {
        spacing: Metrics.gap8

        Item {
            width: parent.width
            height: Metrics.sourceFilterRow
            Text {
                anchors.left: parent.left
                anchors.right: grouping.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Group album versions", I18n.rev)
                font.pointSize: Type.body
                elide: Text.ElideRight
                color: Theme.color.textPrimary
            }
            Toggle {
                id: grouping
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: Library.groupVersions
                onToggled: Library.groupAlbums(!Library.groupVersions)
            }
        }

        Text {
            text: I18n.tr("Sources", I18n.rev)
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textTertiary
        }
        Repeater {
            model: Library.sourceKeys.length

            Item {
                required property int index
                width: parent.width
                height: Metrics.sourceFilterRow
                readonly property string sourceKey: Library.sourceKeys[index]

                SourceIcon {
                    id: sourceIcon
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    width: Metrics.iconNav
                    height: Metrics.iconNav
                    sourceInfo: ({ icon: Library.sourceIcons[index] || "", local: sourceKey === "local:default" })
                }
                Column {
                    anchors.left: sourceIcon.right
                    anchors.leftMargin: Metrics.gap10
                    anchors.right: sourceToggle.left
                    anchors.rightMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        width: parent.width
                        text: sourceKey === "local:default" ? I18n.tr("Local library", I18n.rev) : (Library.sourceLabels[index] || "")
                        elide: Text.ElideRight
                        font.pointSize: Type.body
                        color: Theme.color.textPrimary
                    }
                    Text {
                        text: I18n.trCount("{} track", "{} tracks", Number(Library.sourceCounts[index]), I18n.rev)
                        font.pointSize: Type.micro
                        color: Theme.color.textTertiary
                    }
                }
                Toggle {
                    id: sourceToggle
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    checked: Library.excludedSources.indexOf(sourceKey) < 0
                    onToggled: Library.toggleFilter("source", sourceKey)
                }
            }
        }

        Rectangle { width: parent.width; height: Metrics.focusRing; color: Theme.color.strokeHairline }
        SourceAlbumLibraries { width: parent.width; tracks: popover.showTrackLibraries; active: popover.opened && (popover.showAlbumLibraries || popover.showTrackLibraries) }
        Text {
            text: I18n.tr("Quality", I18n.rev)
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textTertiary
        }
        Flow {
            width: parent.width
            height: childrenRect.height
            spacing: Metrics.gap6

            Repeater {
                model: [
                    { key: "lossy", label: I18n.tr("Lossy", I18n.rev) },
                    { key: "lossless", label: I18n.tr("Lossless", I18n.rev) },
                    { key: "hires", label: I18n.tr("Hi-Res", I18n.rev) },
                    { key: "dsd", label: "DSD" }
                ]
                Chip {
                    required property var modelData
                    text: modelData.label
                    active: Library.qualities.indexOf(modelData.key) >= 0
                    onClicked: Library.toggleFilter("quality", modelData.key)
                }
            }
        }

        Text {
            text: I18n.tr("Format", I18n.rev)
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textTertiary
        }
        Flow {
            width: parent.width
            height: childrenRect.height
            spacing: Metrics.gap6

            Repeater {
                model: [
                    { key: "flac", label: "FLAC" }, { key: "alac", label: "ALAC" },
                    { key: "aac", label: "AAC" }, { key: "mp3", label: "MP3" },
                    { key: "ogg", label: "Ogg" }, { key: "opus", label: "Opus" },
                    { key: "wav", label: "WAV" }, { key: "aiff", label: "AIFF" },
                    { key: "wavpack", label: "WavPack" }, { key: "ape", label: "APE" },
                    { key: "dsd", label: "DSD" }
                ]
                Chip {
                    required property var modelData
                    text: modelData.label
                    active: Library.formats.indexOf(modelData.key) >= 0
                    onClicked: Library.toggleFilter("format", modelData.key)
                }
            }
        }

        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8

            PillButton {
                enabled: Library.extraFiltered
                text: I18n.tr("Reset", I18n.rev)
                onClicked: Library.resetExtra()
            }
            PillButton {
                kind: "primary"
                text: I18n.tr("Done", I18n.rev)
                onClicked: popover.close()
            }
        }
    }
}

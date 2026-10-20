import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Popup {
    id: popover
    property bool showAlbumGrouping: true
    // The Library screen's filter columns that are on, or null where the
    // screen has none; a toggle asks the screen to turn one on or off.
    property var panes: null
    signal paneToggled(string column)
    // The Tracks screen's stars to keep, or null where a screen has none.
    property var ratings: null
    signal ratingsChosen(var ratings)
    readonly property bool starred: ratings !== null && ratings.length > 0
    readonly property real half: (popover.availableWidth - Metrics.gap16) / 2 // derived: two columns and the gap between

    // A click outside closes it and is nothing else: it is not also a click
    // on what is there.
    modal: true
    dim: false

    width: Metrics.sourcesPopoverWidth
    padding: Metrics.gap12
    margins: Metrics.gap10
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

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

        Row {
            spacing: Metrics.gap16

            Column {
                width: popover.half
                spacing: Metrics.gap8
                Item {
                    visible: popover.showAlbumGrouping
                    width: parent.width
                    height: visible ? Metrics.sourceFilterRow : 0
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
                        compact: true
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: Library.groupVersions
                        onToggled: Library.groupAlbums(!Library.groupVersions)
                    }
                }
                // One card for a record that several sources keep, wearing
                // each source's icon.
                Item {
                    visible: popover.showAlbumGrouping
                    width: parent.width
                    height: visible ? Metrics.sourceFilterRow : 0
                    Text {
                        anchors.left: parent.left
                        anchors.right: copies.left
                        anchors.rightMargin: Metrics.gap8
                        anchors.verticalCenter: parent.verticalCenter
                        text: I18n.tr("Group same album from different sources", I18n.rev)
                        font.pointSize: Type.body
                        elide: Text.ElideRight
                        color: Theme.color.textPrimary
                    }
                    Toggle {
                        id: copies
                        objectName: "groupCopies"
                        compact: true
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: Library.groupSources
                        onToggled: Library.groupCopies(!Library.groupSources)
                    }
                }

                SourceSwitches { width: parent.width; active: popover.opened }
            }
            Column {
                width: popover.half
                spacing: Metrics.gap8
                LibraryColumnChoices {
                    width: parent.width
                    selected: popover.panes
                    onToggled: column => popover.paneToggled(column)
                }
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

                TrackStarsFilter {
                    visible: popover.ratings !== null
                    width: parent.width
                    ratings: popover.ratings || []
                    onChosen: (next) => popover.ratingsChosen(next)
                }
            }
        }

        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8

            PillButton {
                objectName: "ratingsReset"
                enabled: Library.extraFiltered || popover.starred
                text: I18n.tr("Reset", I18n.rev)
                onClicked: { Library.resetExtra(); if (popover.starred) popover.ratingsChosen([]) }
            }
            PillButton {
                kind: "primary"
                text: I18n.tr("Done", I18n.rev)
                onClicked: popover.close()
            }
        }
    }
}

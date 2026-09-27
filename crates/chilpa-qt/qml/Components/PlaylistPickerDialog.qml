import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

Modal {
    id: dialog

    width: Metrics.playlistModalWidth
    height: Metrics.playlistModalHeight

    property bool creating: false; property var trackIds: []; property var rows: []
    property string notice: ""; property bool warning: false

    function show(ids) {
        trackIds = ids || []
        creating = trackIds.length === 0
        notice = ""
        warning = false
        reload()
        open()
        Qt.callLater(creating ? nameField.focusInput : search.focusInput)
    }
    function beginCreate() { show([]) }
    function reload() { rows = JSON.parse(Playlists.pickerJson(trackIds, search.text)) }

    contentItem: Column {
        spacing: Metrics.gap12

        Row {
            width: parent.width
            Text {
                width: parent.width - close.width
                text: dialog.creating ? I18n.tr("New playlist", I18n.rev)
                                      : I18n.tr("Add to playlist", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            IconButton {
                id: close
                glyph: Icons.x
                tip: I18n.tr("Close", I18n.rev)
                onClicked: dialog.close()
            }
        }

        Text {
            text: I18n.trCount("{} selected track", "{} selected tracks",
                               dialog.trackIds.length, I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }

        TextField {
            id: search
            visible: !dialog.creating
            width: parent.width
            placeholder: I18n.tr("Search playlists", I18n.rev)
            onTextChanged: dialog.reload()
        }

        PillButton {
            visible: !dialog.creating
            width: parent.width
            glyph: Icons.plus
            text: I18n.tr("Create new playlist", I18n.rev)
            onClicked: {
                dialog.creating = true
                Qt.callLater(nameField.focusInput)
            }
        }

        Text {
            visible: dialog.notice !== ""
            width: parent.width
            text: dialog.notice
            wrapMode: Text.Wrap
            font.pointSize: Type.small
            color: dialog.warning ? Theme.color.statusWarning : Theme.color.statusPositive
        }

        ListView {
            visible: !dialog.creating
            width: parent.width
            height: parent.height - y
            clip: true
            spacing: Metrics.gap4
            model: dialog.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}

            delegate: ListRowSurface {
                required property var playlist
                required property int exact
                required property int similar
                required property bool accepts

                width: ListView.view.width
                height: Metrics.playlistPickerRow
                rowIndex: index
                radius: Metrics.radiusControl
                hovered: hover.hovered
                opacity: accepts ? 1 : Metrics.disabledOpacity

                Icon {
                    id: sourceIcon
                    x: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: playlist.mixed ? Icons.playlist : Icons.server
                    size: Metrics.iconMd
                    color: Theme.color.textSecondary
                }
                Column {
                    anchors.left: sourceIcon.right
                    anchors.leftMargin: Metrics.gap10
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        width: parent.width
                        text: playlist.name
                        elide: Text.ElideRight
                        font.pointSize: Type.body
                        font.weight: Type.medium
                        color: Theme.color.textPrimary
                    }
                    Text {
                        width: parent.width
                        text: exact > 0
                            ? I18n.trCount("Already contains {} track", "Already contains {} tracks", exact, I18n.rev)
                            : similar > 0
                                ? I18n.trCount("Contains {} similar version", "Contains {} similar versions", similar, I18n.rev)
                                : playlist.provider + " · " + I18n.trCount("{} track", "{} tracks", playlist.track_count, I18n.rev)
                        elide: Text.ElideRight
                        font.pointSize: Type.small
                        color: similar > 0 ? Theme.color.statusWarning : Theme.color.textTertiary
                    }
                }
                HoverHandler { id: hover }
                TapHandler {
                    enabled: accepts
                    onTapped: {
                        const result = JSON.parse(Playlists.addTrackIds(playlist.id, dialog.trackIds))
                        dialog.warning = result.similar > 0 || result.rejected
                        if (result.rejected) dialog.notice = I18n.tr("That source does not allow tracks from here in this playlist.", I18n.rev)
                        else {
                            dialog.notice = I18n.trCount("{} track added", "{} tracks added", result.added, I18n.rev)
                            if (result.exact > 0) dialog.notice += " · " + I18n.trCount("{} was already there", "{} were already there", result.exact, I18n.rev)
                            if (result.similar > 0) dialog.notice += " · " + I18n.trCount("{} similar version is also there", "{} similar versions are also there", result.similar, I18n.rev)
                        }
                        dialog.reload()
                    }
                }
            }
        }

        Column {
            visible: dialog.creating
            width: parent.width
            spacing: Metrics.gap10

            TextField { id: nameField; width: parent.width; placeholder: I18n.tr("Playlist name", I18n.rev) }
            TextField { id: descriptionField; width: parent.width; placeholder: I18n.tr("Description", I18n.rev) }
            TextField { id: tagsField; width: parent.width; placeholder: I18n.tr("Tags, separated by commas", I18n.rev) }
            Row {
                width: parent.width
                spacing: Metrics.gap8
                TextField {
                    id: artworkField
                    width: parent.width - chooseArt.width - parent.spacing
                    placeholder: I18n.tr("Custom image", I18n.rev)
                }
                PillButton {
                    id: chooseArt
                    text: I18n.tr("Choose", I18n.rev)
                    onClicked: artworkField.text = Playlists.chooseArtwork()
                }
            }
            Item { width: 1; height: Metrics.gap8 }
            Row {
                spacing: Metrics.gap8
                PillButton {
                    text: I18n.tr("Cancel", I18n.rev)
                    onClicked: dialog.creating = false
                }
                PillButton {
                    kind: "primary"
                    text: I18n.tr("Create and add", I18n.rev)
                    enabled: nameField.text.trim() !== ""
                    onClicked: {
                        if (Playlists.createFull(nameField.text, descriptionField.text,
                                                 artworkField.text, tagsField.text, 0,
                                                 dialog.trackIds) > 0)
                            dialog.close()
                    }
                }
            }
        }
    }
}

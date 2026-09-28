import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

Modal {
    id: dialog
    objectName: "playlistPickerDialog"
    width: Math.max(Metrics.modalInsets, Math.min(Metrics.playlistModalWidth, parent.width - Metrics.modalInset))
    height: Math.max(Metrics.modalInsets, Math.min(creating ? createBody.implicitHeight + heading.height + Metrics.gap12 + Metrics.modalInsets
                             : Metrics.playlistModalHeight, parent.height - Metrics.modalInset))

    property bool creating: false
    property string trackIdsJson: "[]"
    readonly property int trackCount: JSON.parse(trackIdsJson).length
    property var rows: []
    property string notice: ""
    property bool warning: false

    function show(idsJson) {
        trackIdsJson = idsJson || "[]"
        creating = trackCount === 0
        search.text = ""
        form.reset()
        notice = ""
        warning = false
        reload()
        open()
        Qt.callLater(function() { if (dialog.creating) form.focusInput(); else search.focusInput() })
    }
    function beginCreate() { show("[]") }
    function reload() { rows = JSON.parse(Playlists.pickerJson(trackIdsJson, search.text)) }
    function create() {
        if (!form.valid) return
        if (Playlists.createFull(form.name, form.description, form.artwork, form.tags, 0, trackIdsJson) > 0)
            close()
        else {
            warning = true
            notice = I18n.tr("Could not create the playlist. Please try again.", I18n.rev)
        }
    }
    function add(id) {
        const result = JSON.parse(Playlists.addTrackIds(id, trackIdsJson))
        warning = result.similar > 0 || result.rejected
        if (result.rejected) notice = I18n.tr("Could not add tracks to this playlist.", I18n.rev)
        else {
            notice = I18n.trCount("{} track added", "{} tracks added", result.added, I18n.rev)
            if (result.exact > 0) notice += " · " + I18n.trCount("{} was already there", "{} were already there", result.exact, I18n.rev)
            if (result.similar > 0) notice += " · " + I18n.trCount("{} similar version is also there", "{} similar versions are also there", result.similar, I18n.rev)
        }
        reload()
    }
    Connections {
        target: Playlists
        function onRevChanged() { if (dialog.visible) dialog.reload() }
    }

    Column {
        anchors.fill: parent
        spacing: Metrics.gap12
        Row {
            id: heading
            width: parent.width
            Text {
                width: parent.width - closeButton.width
                text: dialog.creating ? I18n.tr("New playlist", I18n.rev) : I18n.tr("Add to playlist", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
        Flickable {
            visible: dialog.creating
            width: parent.width
            height: Math.max(0, parent.height - y)
            contentHeight: createBody.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: createBody
                width: parent.width
                spacing: Metrics.gap12
                Text {
                    visible: dialog.trackCount > 0
                    text: I18n.trCount("{} selected track", "{} selected tracks", dialog.trackCount, I18n.rev)
                    font.pointSize: Type.meta
                    color: Theme.color.textSecondary
                }
                PlaylistForm { id: form; width: parent.width; onAccepted: dialog.create() }
                Text {
                    visible: dialog.notice !== ""
                    width: parent.width
                    text: dialog.notice
                    wrapMode: Text.Wrap
                    font.pointSize: Type.small
                    color: Theme.color.statusWarning
                }
                Row {
                    anchors.right: parent.right
                    spacing: Metrics.gap8
                    PillButton {
                        text: I18n.tr("Cancel", I18n.rev)
                        onClicked: {
                            if (dialog.trackCount === 0) dialog.close()
                            else { dialog.creating = false; dialog.notice = ""; search.focusInput() }
                        }
                    }
                    PillButton {
                        objectName: "createPlaylist"
                        kind: "primary"
                        text: dialog.trackCount > 0 ? I18n.tr("Create and add", I18n.rev) : I18n.tr("Create playlist", I18n.rev)
                        enabled: form.valid
                        onClicked: dialog.create()
                    }
                }
            }
        }
        Text {
            visible: !dialog.creating
            text: I18n.trCount("{} selected track", "{} selected tracks", dialog.trackCount, I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        SearchField {
            id: search
            objectName: "playlistSearch"
            visible: !dialog.creating
            width: parent.width
            placeholder: I18n.tr("Search playlists", I18n.rev)
            onTextChanged: if (dialog.visible) dialog.reload()
            onEscaped: dialog.close()
        }
        PillButton {
            visible: !dialog.creating
            text: I18n.tr("Create new playlist", I18n.rev)
            glyph: Icons.plus
            onClicked: { dialog.creating = true; dialog.notice = ""; Qt.callLater(form.focusInput) }
        }
        Text {
            visible: !dialog.creating && dialog.notice !== ""
            width: parent.width
            text: dialog.notice
            wrapMode: Text.Wrap
            font.pointSize: Type.small
            color: dialog.warning ? Theme.color.statusWarning : Theme.color.statusPositive
        }
        ListView {
            objectName: "playlistTargets"
            visible: !dialog.creating
            width: parent.width
            height: Math.max(0, parent.height - y)
            clip: true
            spacing: Metrics.gap4
            model: dialog.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            delegate: PlaylistTargetRow {
                required property int index
                required property var modelData
                width: ListView.view.width
                rowIndex: index
                target: modelData
                onChosen: dialog.add(target.playlist.id)
            }
            Text {
                anchors.centerIn: parent
                width: parent.width
                visible: dialog.rows.length === 0
                text: search.text.trim() === "" ? I18n.tr("No playlists yet. Create one to get started.", I18n.rev)
                                               : I18n.tr("No playlists match your search.", I18n.rev)
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
        }
    }
}

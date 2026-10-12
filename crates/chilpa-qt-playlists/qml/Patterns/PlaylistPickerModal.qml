import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import app.chilpa.playlists
import "../Components"

// "Add to playlist", as QBZ's picker (PlaylistPickerModal): the search opens
// the list of playlists, a click chooses one and the footer's button adds,
// "+ Create new playlist" asks for a name in place, the playlists that
// already hold the tracks are listed beside it, and a playlist that holds
// some of them asks first whether to add the repeats. Chilpa's own: the
// mark of each playlist, and only the playlists of the tracks' own source
// when asked. Opened without tracks it is "New playlist", with the whole form.
Modal {
    id: dialog
    objectName: "playlistPickerModal"
    width: Math.max(Metrics.modalInsets, Math.min(creating ? Metrics.playlistModalWidth : Metrics.playlistPickerWidth, parent.width - Metrics.modalInset))
    height: Math.max(Metrics.modalInsets, Math.min(creating ? createBody.implicitHeight + heading.height + Metrics.gap12 + Metrics.modalInsets
                                                            : Metrics.playlistPickerHeight, parent.height - Metrics.modalInset))

    property bool creating: false
    property string trackIdsJson: "[]"
    readonly property int trackCount: JSON.parse(trackIdsJson).length
    // What the search leaves, and every playlist the tracks can go to.
    property var rows: []
    property var every: []
    property string notice: ""
    property bool warning: false
    property string sourceId: ""
    property string sourceName: ""
    property var sourceRows: []
    property var chosen: null
    property bool naming: false
    property bool listOpen: false
    property var repeatsFor: null
    property bool sameSource: SettingsModel.kept("ui.playlists.sameSource") === "true"
    readonly property var shown: rows.filter(row => !sameSource || row.same_source)
    readonly property var lastUsed: shown.find(row => Playlists.lastTarget !== 0 && row.playlist.id === Playlists.lastTarget) || null
    readonly property var alreadyIn: every.filter(row => row.exact > 0)
    readonly property bool pending: writer.pending
    closePolicy: pending ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside

    PlaylistSourceWrite {
        id: writer
        onFinished: (message, warning) => {
            dialog.notice = message; dialog.warning = warning
            if (!warning) dialog.close()
        }
    }
    function show(idsJson) {
        trackIdsJson = idsJson || "[]"
        sourceId = ""; sourceName = ""
        sourceRows = JSON.parse(Playlists.sourcesJson(trackIdsJson))
        creating = trackCount === 0
        chosen = null; naming = false; listOpen = false; repeatsFor = null
        search.text = ""; name.text = ""; search.release()
        form.reset()
        notice = ""
        warning = false
        reload()
        open()
        // As QBZ's, the search is not taken at once: taking it opens the list.
        Qt.callLater(function() { if (dialog.creating) form.focusInput() })
    }
    function beginCreate() { show("[]") }
    function targets(query) { return JSON.parse(Playlists.pickerJson(trackIdsJson, query)).filter(row => row.accepts === true) }
    function reload() { every = targets(""); rows = search.text.trim() === "" ? every : targets(search.text) }
    function choose(target) { chosen = target; naming = false; listOpen = false; search.text = ""; search.release() }
    function remoteWrite(id) { writer.submit(sourceId, id, creating ? form.name : name.text, creating ? form.description : "", trackIdsJson) }
    function create() {
        const named = creating ? form.name : name.text.trim()
        if ((creating && !form.valid) || named === "" || pending) return
        if (sourceId) { remoteWrite(0); return }
        if (Playlists.createFull(named, creating ? form.description : "", creating ? form.artwork : "", creating ? form.tags : "", 0, trackIdsJson) > 0)
            close()
        else {
            warning = true
            notice = I18n.tr("Could not create the playlist. Please try again.", I18n.rev)
        }
    }
    function confirm() {
        if (pending) return
        if (naming) return create()
        if (chosen === null) return
        if (chosen.exact > 0) repeatsFor = chosen
        else add(chosen, false)
    }
    function add(target, every) {
        if (pending || !target.accepts) return
        if (target.playlist.id < 0) {
            sourceId = target.playlist.source
            remoteWrite(target.playlist.id)
            return
        }
        const result = JSON.parse(Playlists.addTrackIds(target.playlist.id, trackIdsJson, every))
        repeatsFor = null
        if (!result.rejected) return close()
        warning = true
        notice = I18n.tr("Could not add tracks to this playlist.", I18n.rev)
    }
    Connections {
        target: Playlists
        function onRevChanged() { if (dialog.visible) dialog.reload() }
    }
    Connections {
        target: AddToPlaylist
        function onAsked(tracks) { dialog.show(tracks) }
    }

    Column {
        anchors.fill: parent
        spacing: Metrics.gap12
        enabled: !dialog.pending
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
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
            Column {
                id: createBody
                width: parent.width
                spacing: Metrics.gap12
                PlaylistSourceChooser {
                    width: parent.width; sources: dialog.sourceRows
                    sourceId: dialog.sourceId; sourceName: dialog.sourceName
                    onPicked: (id, name) => { dialog.sourceId = id; dialog.sourceName = name }
                }
                PlaylistForm { id: form; localMetadata: !dialog.sourceId; width: parent.width; onAccepted: dialog.create() }
                PlaylistNotice { width: parent.width; text: dialog.notice }
                Row {
                    anchors.right: parent.right
                    spacing: Metrics.gap8
                    PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
                    PillButton { objectName: "createPlaylist"; kind: "primary"; text: I18n.tr("Create playlist", I18n.rev); enabled: form.valid; onClicked: dialog.create() }
                }
            }
        }
        Item {
            id: body
            visible: !dialog.creating
            width: parent.width
            height: Math.max(0, parent.height - y - footer.height - parent.spacing)
            // A click on the picker's empty space closes the list.
            MouseArea { anchors.fill: parent; onClicked: dialog.listOpen = false }
            Column {
                id: choosing
                width: parent.width - (dialog.alreadyIn.length > 0 ? Metrics.playlistAlreadyIn + Metrics.gap16 : 0)
                spacing: Metrics.gap8
                Item {
                    width: parent.width
                    height: Math.max(label.implicitHeight, counter.implicitHeight)
                    Text { id: label; text: I18n.tr("Choose playlist", I18n.rev); font.pointSize: Type.small; font.weight: Type.medium; color: Theme.color.textSecondary }
                    Text { id: counter; anchors.right: parent.right; text: I18n.trCount("Adding {} track", "Adding {} tracks", dialog.trackCount, I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary }
                }
                SearchField {
                    id: search
                    objectName: "playlistSearch"
                    width: parent.width
                    placeholder: dialog.naming ? I18n.tr("+ Create new playlist", I18n.rev)
                        : dialog.chosen ? dialog.chosen.playlist.name : I18n.tr("Search playlists...", I18n.rev)
                    onEditingChanged: if (editing) dialog.listOpen = true
                    onTextChanged: if (dialog.visible) { dialog.listOpen = true; dialog.reload() }
                    onAccepted: dialog.confirm()
                    onEscaped: dialog.listOpen = false
                    IconButton {
                        visible: (dialog.chosen !== null || dialog.naming) && search.text === "" && !dialog.listOpen
                        anchors.right: parent.right
                        anchors.rightMargin: Metrics.gap6
                        anchors.verticalCenter: parent.verticalCenter
                        size: Metrics.button26
                        glyph: Icons.x
                        tip: I18n.tr("Clear", I18n.rev)
                        onClicked: { dialog.chosen = null; dialog.naming = false }
                    }
                }
                Row {
                    spacing: Metrics.gap8
                    Toggle {
                        objectName: "sameSourceOnly"
                        anchors.verticalCenter: parent.verticalCenter
                        checked: dialog.sameSource
                        onToggled: (on) => { dialog.sameSource = on; SettingsModel.keep("ui.playlists.sameSource", on ? "true" : "false") }
                    }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Show playlist only from the same source as the track", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
                }
                Text { visible: dialog.naming; topPadding: Metrics.gap8; text: I18n.tr("Playlist Name", I18n.rev); font.pointSize: Type.small; font.weight: Type.medium; color: Theme.color.textSecondary }
                TextField { id: name; objectName: "newPlaylistName"; visible: dialog.naming; width: parent.width; placeholder: I18n.tr("My Playlist", I18n.rev); onAccepted: dialog.create() }
                PlaylistSourceChooser {
                    visible: dialog.naming && dialog.sourceRows.length > 0
                    sources: dialog.sourceRows; sourceId: dialog.sourceId; sourceName: dialog.sourceName
                    onPicked: (id, name) => { dialog.sourceId = id; dialog.sourceName = name }
                }
                PlaylistNotice { width: parent.width; text: dialog.notice }
            }
            PlaylistAlreadyIn {
                visible: dialog.alreadyIn.length > 0
                anchors.right: parent.right
                width: Metrics.playlistAlreadyIn
                rows: dialog.alreadyIn
                trackCount: dialog.trackCount
                onChosen: (target) => dialog.choose(target)
            }
            PlaylistChoiceList {
                z: 1
                visible: dialog.listOpen
                y: search.y + search.height + Metrics.gap4
                width: choosing.width
                rows: dialog.shown.filter(row => row !== dialog.lastUsed)
                lastUsed: dialog.lastUsed
                searching: search.text.trim() !== ""
                onCreateAsked: { dialog.naming = true; dialog.chosen = null; dialog.listOpen = false; search.text = ""; Qt.callLater(name.focusInput) }
                onChosen: (target) => dialog.choose(target)
            }
        }
        Row {
            id: footer
            visible: !dialog.creating
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
            PillButton {
                objectName: "addToPlaylist"
                kind: "primary"
                text: dialog.pending ? I18n.tr("Saving...", I18n.rev) : dialog.naming ? I18n.tr("Create & Add", I18n.rev) : I18n.tr("Add", I18n.rev)
                enabled: !dialog.pending && (dialog.naming ? name.text.trim() !== "" : dialog.chosen !== null)
                onClicked: dialog.confirm()
            }
        }
    }
    PlaylistRepeats {
        anchors.fill: parent
        visible: dialog.repeatsFor !== null
        repeated: dialog.repeatsFor ? dialog.repeatsFor.exact : 0
        trackCount: dialog.trackCount
        busy: dialog.pending
        onAnswered: (every) => dialog.add(dialog.repeatsFor, every)
        onCancelled: dialog.repeatsFor = null
    }
}

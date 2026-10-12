import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Parts
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

Item {
    id: page
    readonly property var document: ListenList.snapshot ? JSON.parse(ListenList.snapshot) : ({})
    property string typed: ""
    property bool followPlaying: false
    property bool positioningList: false
    property int playingRow: -1
    readonly property string selectedQueue: String(document.selected || "")
    readonly property bool playingSelected: document.active === document.selected && Player.playing && !Player.paused
    function cancelFollow() { followPlaying = false }
    function positionAt(row, mode) {
        if (row < 0 || row >= tracks.count)
            return
        positioningList = true
        list.rows.positionViewAtIndex(row, mode)
        Qt.callLater(function() { page.positioningList = false })
    }
    function focusPlaying() {
        if (playingRow < 0)
            return
        followPlaying = true
        positionAt(playingRow, ListView.Center)
    }
    function goToTop() {
        cancelFollow()
        if (tracks.count > 0)
            positionAt(0, ListView.Beginning)
    }
    function refreshPlayingRow() {
        playingRow = tracks.playing_row()
        if (followPlaying)
            Qt.callLater(function() { if (page.followPlaying) page.positionAt(page.playingRow, ListView.Center) })
    }
    onSelectedQueueChanged: cancelFollow()
    onDocumentChanged: Qt.callLater(refreshPlayingRow)
    // The footer and the clock it ends by, read on the same tick as the
    // queues and apart from them, so the clock does not reload the list.
    property var foot: ({})
    property real now: Date.now()
    function tick() {
        ListenList.refresh()
        Cast.refresh()
        page.now = Date.now()
        page.foot = JSON.parse(ListenList.footer() || "{}")
    }
    Component.onCompleted: tick()
    Timer { interval: Motion.listenRefresh; running: page.visible; repeat: true; onTriggered: page.tick() }
    Parts.SleepTimer { id: sleep }
    TrackListModel {
        id: tracks
        kind: "listenlist"
        albumId: Number(page.document.selected || 0)
        key: page.typed
        filterRev: ListenList.revision + Library.remoteRev
        playingPath: Player.path
        onCountChanged: Qt.callLater(page.refreshPlayingRow)
    }
    Connections {
        target: tracks
        function onModelReset() { Qt.callLater(page.refreshPlayingRow) }
    }
    Connections {
        target: Player
        function onPathChanged() { Qt.callLater(page.refreshPlayingRow) }
    }
    ListenNow {
        id: now
        x: Metrics.contentX
        y: Metrics.contentTop
        width: Metrics.listenArt
        visible: page.width >= Metrics.listenWide
    }
    Item {
        id: content
        anchors.left: now.visible ? now.right : parent.left
        anchors.leftMargin: now.visible ? Metrics.heroGap : Metrics.contentX
        anchors.right: parent.right
        anchors.rightMargin: Metrics.contentX
        anchors.top: parent.top
        anchors.topMargin: Metrics.contentTop
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.playerSpace + Metrics.playerBottom + Metrics.gap16
        Behavior on anchors.bottomMargin { NumberAnimation { duration: Motion.panel; easing.type: Easing.OutCubic } }
        ListenBuffers {
            id: tabs
            anchors.top: parent.top
            width: parent.width
            document: page.document
        }
        Item {
            id: tools
            anchors.top: tabs.bottom
            anchors.topMargin: Metrics.gap8
            width: parent.width
            readonly property bool wrapped: width < actions.width + Metrics.gap12 + Metrics.queueSearchMin + (ends.visible ? Metrics.gap12 + ends.implicitWidth : 0)
            height: wrapped ? actions.height + Metrics.gap8 + lookup.height : Math.max(actions.height, lookup.height)
            Row {
                id: actions
                anchors.left: parent.left
                anchors.top: parent.top
                spacing: Metrics.gap8
                PillButton {
                    text: page.playingSelected ? I18n.tr("Pause", I18n.rev) : I18n.tr("Play", I18n.rev)
                    glyph: page.playingSelected ? Icons.playerPauseFilled : Icons.playerPlayFilled
                    filled: true
                    enabled: tracks.count > 0 && !page.document.pending
                    onClicked: {
                        page.forceActiveFocus()
                        if (page.playingSelected) Player.toggle()
                        else ListenList.command("play", page.document.selected, "")
                    }
                }
                IconButton { glyph: Icons.playlistAdd; size: Metrics.button32; round: true; tip: I18n.tr("Save as playlist", I18n.rev); enabled: tracks.count > 0; onClicked: { page.forceActiveFocus(); ListenList.command("save", page.document.selected, "") } }
                IconButton { glyph: Icons.trash; size: Metrics.button32; round: true; tip: I18n.tr("Clear queue", I18n.rev); onClicked: ListenList.command("clear", page.document.selected, "") }
                IconButton { glyph: Lucide.undo2; iconFamily: Lucide.font; size: Metrics.button32; round: true; tip: I18n.tr("Undo queue replacement or closing", I18n.rev); enabled: page.document.undo || false; onClicked: ListenList.command("undo", "", "") }
                IconButton { objectName: "queuesTimer"; glyph: SourceGlyphs.clock; iconFamily: SourceGlyphs.font; size: Metrics.button32; round: true; tone: Controls.sleepLeft !== "" ? "accent" : "secondary"; tip: Controls.sleepLeft !== "" ? I18n.trArgs("Stops in {}", [Controls.sleepLeft], I18n.rev) : I18n.tr("Sleep timer", I18n.rev); onClicked: sleep.open() }
            }
            Row {
                id: lookup
                x: tools.wrapped ? 0 : tools.width - width
                y: tools.wrapped ? actions.height + Metrics.gap8 : 0
                spacing: Metrics.gap12
                SearchField {
                    id: find
                    width: Math.min(Metrics.screenSearch, Math.max(Metrics.queueSearchMin, tools.width - (tools.wrapped ? 0 : actions.width + Metrics.gap12) - (ends.visible ? ends.implicitWidth + Metrics.gap12 : 0)))
                    placeholder: I18n.tr("Search queue", I18n.rev)
                    onTextChanged: { page.typed = text; if (text !== "") page.cancelFollow() }
                }
                Parts.QueueEnds {
                    id: ends
                    y: (find.height - height) / 2
                    visible: tracks.count > 0 && (page.foot.summary || "") !== ""
                    summary: page.foot.summary || ""
                    remaining: page.foot.left_ms || 0
                    now: page.now
                }
            }
        }
        TrackList {
            id: list
            anchors.top: tools.bottom
            anchors.topMargin: Metrics.gap8
            anchors.bottom: parent.bottom
            width: parent.width - Metrics.queueActionRail
            model: tracks
            groupActionsEnabled: false
            reorderable: page.typed === ""
            scrollGutter: Metrics.gap8
        }
        Connections {
            target: list.rows
            function onMovementStarted() {
                if (!page.positioningList) page.cancelFollow()
            }
        }
        Column {
            id: viewportActions
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.bottomMargin: Metrics.gap12
            spacing: Metrics.gap8
            z: 3
            Rectangle {
                width: Metrics.queueViewportAction
                height: width
                radius: Metrics.radiusItem
                color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfaceRaised
                border.width: 1
                border.color: page.followPlaying ? Theme.color.accent : Theme.color.strokeHairline
                IconButton {
                    objectName: "queueFocusPlaying"
                    anchors.fill: parent
                    size: Metrics.queueViewportAction
                    glyph: Icons.disc
                    checked: page.followPlaying
                    enabled: page.playingRow >= 0
                    tip: I18n.tr("Focus on playing", I18n.rev)
                    onClicked: page.focusPlaying()
                }
            }
            Rectangle {
                width: Metrics.queueViewportAction
                height: width
                radius: Metrics.radiusItem
                color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfaceRaised
                border.width: 1
                border.color: Theme.color.strokeHairline
                IconButton {
                    objectName: "queueBackToTop"
                    anchors.fill: parent
                    size: Metrics.queueViewportAction
                    glyph: Icons.chevronUp
                    enabled: tracks.count > 0
                    tip: I18n.tr("Go to top", I18n.rev)
                    onClicked: page.goToTop()
                }
            }
        }
        Text {
            anchors.centerIn: list
            visible: tracks.count === 0
            width: list.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: I18n.tr("Add tracks from a track menu to this queue.", I18n.rev)
            color: Theme.color.textTertiary
            font.pointSize: Type.body
        }
    }
}

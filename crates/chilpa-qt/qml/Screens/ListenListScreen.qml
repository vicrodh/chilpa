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
        Text {
            id: title
            text: I18n.tr("Queues", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        ListenBuffers {
            id: tabs
            anchors.top: title.bottom
            anchors.topMargin: Metrics.gap12
            width: parent.width
            document: page.document
        }
        Flow {
            id: tools
            anchors.top: tabs.bottom
            anchors.topMargin: Metrics.gap12
            width: parent.width
            spacing: Metrics.gap8
            PillButton {
                text: I18n.tr("Play", I18n.rev)
                glyph: Icons.playerPlayFilled
                filled: true
                enabled: tracks.count > 0 && !page.document.pending
                onClicked: { page.forceActiveFocus(); ListenList.command("play", page.document.selected, "") }
            }
            PillButton {
                text: I18n.tr("Save as playlist", I18n.rev)
                enabled: tracks.count > 0
                onClicked: { page.forceActiveFocus(); ListenList.command("save", page.document.selected, "") }
            }
            IconButton { glyph: Icons.trash; tip: I18n.tr("Clear queue", I18n.rev); onClicked: ListenList.command("clear", page.document.selected, "") }
            IconButton { glyph: Icons.refresh; tip: I18n.tr("Undo queue replacement or closing", I18n.rev); enabled: page.document.undo || false; onClicked: ListenList.command("undo", "", "") }
            IconButton { objectName: "queuesTimer"; glyph: SourceGlyphs.clock; iconFamily: SourceGlyphs.font; tone: Controls.sleepLeft !== "" ? "accent" : "secondary"; tip: Controls.sleepLeft !== "" ? I18n.trArgs("Stops in {}", [Controls.sleepLeft], I18n.rev) : I18n.tr("Sleep timer", I18n.rev); onClicked: sleep.open() }
        }
        SearchField {
            id: find
            anchors.top: tools.bottom
            anchors.topMargin: Metrics.gap12
            width: Math.min(parent.width, Metrics.screenSearch)
            placeholder: I18n.tr("Search queue", I18n.rev)
            onTextChanged: page.typed = text
        }
        Parts.QueueEnds {
            anchors.left: find.right
            anchors.leftMargin: Metrics.gap12
            anchors.verticalCenter: find.verticalCenter
            visible: tracks.count > 0 && (page.foot.summary || "") !== ""
            summary: page.foot.summary || ""
            remaining: page.foot.left_ms || 0
            now: page.now
        }
        TrackList {
            id: list
            anchors.top: find.bottom
            anchors.topMargin: Metrics.gap8
            anchors.bottom: parent.bottom
            width: parent.width
            model: tracks
            groupActionsEnabled: false
            reorderable: page.typed === ""
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

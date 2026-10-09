import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

// The queue and the history of the immersive view (03-immersive §1.4),
// one component for both places: in focus a reading column of
// min(width - 96, 720), in the split the whole slot. Two tabs with their
// counts and an underline; under Queue, the row that plays and UP NEXT
// with a Clear button; under History, RECENTLY PLAYED, most recent first.
// Rows of 56 with the cover at 40, title, artist and the duration that
// gives way to a play button on hover. A click plays that row.
Item {
    id: panel
    property bool centered: true
    property int tab: 0
    readonly property real contentWide: centered ? panel.width : Math.min(panel.width - Stage.queueSideLess, Stage.queueWide)

    TrackListModel { id: ahead; kind: "queue"; filterRev: Player.queueRev + ListenList.revision; playingPath: Player.path }
    TrackListModel { id: behind; kind: "history"; filterRev: Player.queueRev + ListenList.revision; playingPath: Player.path }
    Timer { interval: Motion.listenRefresh; repeat: true; running: panel.visible; onTriggered: { ahead.refresh(); behind.refresh() } }
    readonly property bool hasCurrent: ahead.count > 0 && (Player.path !== "")
    readonly property int upcoming: Math.max(0, ahead.count - (hasCurrent ? 1 : 0))
    readonly property string activeBuffer: {
        try {
            return JSON.parse(ListenList.snapshot).active || ""
        } catch (e) {
            return ""
        }
    }

    component SectionLabel: Text {
        color: Theme.color.immersiveTextMuted
        font.pointSize: Stage.queueHeadText
        font.weight: Type.bold
        font.letterSpacing: Stage.queueHeadTracking
    }
    Item {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.topMargin: Stage.queuePadTop
        anchors.bottomMargin: Stage.queuePadBottom
        width: panel.contentWide
        Column {
            anchors.fill: parent
            anchors.leftMargin: panel.centered ? 0 : Stage.queueFocusLess
            anchors.rightMargin: panel.centered ? 0 : Stage.queueFocusLess
            spacing: Stage.queueApart
            Item {
                width: parent.width
                height: Stage.queueTabTall
                Row {
                    anchors.left: parent.left
                    height: parent.height
                    spacing: Stage.queueTabApart
                    ImmersiveQueueTab { label: I18n.tr("Queue", I18n.rev); count: panel.upcoming; active: panel.tab === 0; onClicked: panel.tab = 0 }
                    ImmersiveQueueTab { label: I18n.tr("History", I18n.rev); count: behind.count; active: panel.tab === 1; onClicked: panel.tab = 1 }
                }
                Rectangle {
                    visible: panel.tab === 0 && panel.upcoming > 0
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: clearRow.implicitWidth + Stage.queueClearPad * 2 // derived: the label and its padding
                    height: Stage.queueClearTall
                    radius: Stage.queueClearRound
                    color: clearArea.containsMouse ? Theme.color.immersiveClearHover : Theme.color.immersiveClear
                    border.width: Metrics.hairline
                    border.color: clearArea.containsMouse ? Theme.color.immersiveClearEdgeHover : Theme.color.immersiveClearEdge
                    Row {
                        id: clearRow
                        anchors.centerIn: parent
                        spacing: Stage.queueHistoryApart + Stage.menuRowApart * 3 // derived: seven pixels
                        Text { text: Icons.trash; font.family: Icons.outline; font.pixelSize: Stage.queueClearIcon; color: Theme.color.immersiveTextStrong; opacity: clearArea.containsMouse ? 1 : 0.8; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: I18n.tr("Clear", I18n.rev); color: clearArea.containsMouse ? Theme.color.immersiveTextStrong : Theme.color.immersiveTextDim; font.pointSize: Stage.queueClearText; font.weight: Type.medium; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea { id: clearArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ListenList.command("clear", panel.activeBuffer, "") }
                }
            }
            Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.immersiveQueueLine }

            Item {
                width: parent.width
                height: parent.height - Stage.queueTabTall - Metrics.hairline - Stage.queueApart * 2 // derived: what the header leaves
                Item {
                    anchors.fill: parent
                    visible: panel.tab === 0
                    Column {
                        anchors.fill: parent
                        visible: panel.hasCurrent || panel.upcoming > 0
                        spacing: Stage.queueApart
                        Column {
                            id: nowColumn
                            visible: panel.hasCurrent
                            width: parent.width
                            spacing: Stage.menuRowLead
                            SectionLabel { text: I18n.tr("NOW PLAYING", I18n.rev) }
                            Loader {
                                width: parent.width
                                active: panel.hasCurrent
                                sourceComponent: ImmersiveQueueRow {
                                    current: true
                                    title: Player.title
                                    artist: Player.artist
                                    duration: Player.duration > 0 ? Player.clock(Player.duration) : ""
                                    art: Player.art
                                }
                            }
                        }
                        Column {
                            visible: panel.upcoming > 0
                            width: parent.width
                            height: parent.height - (nowColumn.visible ? nowColumn.implicitHeight + Stage.queueApart : 0) // derived: what the row playing leaves
                            spacing: Stage.menuRowLead
                            Row {
                                id: upHeader
                                spacing: Stage.menuApart
                                SectionLabel { text: I18n.tr("UP NEXT", I18n.rev) }
                                SectionLabel { text: "· " + panel.upcoming; color: Theme.color.immersiveTextGhost; font.weight: Type.medium; font.letterSpacing: Stage.nowTracking }
                            }
                            Item {
                                width: parent.width
                                height: parent.height - upHeader.implicitHeight - Stage.menuRowLead // derived: under its header
                            ListView {
                                    id: upList
                                    objectName: "immersiveUpNext"
                                    anchors.fill: parent
                                    model: ahead
                                    clip: true
                                    spacing: Stage.queueHistoryApart
                                    boundsBehavior: Flickable.StopAtBounds
                                    cacheBuffer: Stage.queueCache
                                    delegate: Loader {
                                        id: slot
                                        required property int index
                                        required property string title
                                        required property string artist
                                        required property string duration
                                        required property url art
                                        width: upList.width - Stage.queueApart
                                        // The row that plays is above, not here.
                                        height: index === 0 && panel.hasCurrent ? 0 : Stage.queueRowTall
                                        active: height > 0
                                        sourceComponent: ImmersiveQueueRow { title: slot.title; artist: slot.artist; duration: slot.duration; art: slot.art; onPlay: ahead.play(slot.index) }
                                    }
                                    ViewportScrollBar { viewport: upList; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom }
                                }
                            }
                        }
                    }
                    Column {
                        visible: !panel.hasCurrent && panel.upcoming === 0
                        anchors.centerIn: parent
                        width: parent.width
                        spacing: Stage.menuApart
                        Text { width: parent.width; text: I18n.tr("Your queue is empty", I18n.rev); color: Theme.color.immersiveTextDim; font.pointSize: Stage.queueEmptyText; font.weight: Type.semibold; horizontalAlignment: Text.AlignHCenter }
                        Text { width: parent.width; text: I18n.tr("Play an album or track to get started", I18n.rev); color: Theme.color.immersiveTextGhost; font.pointSize: Stage.queueEmptySubText; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap }
                    }
                }
                Item {
                    anchors.fill: parent
                    visible: panel.tab === 1
                    Column {
                        anchors.fill: parent
                        visible: behind.count > 0
                        spacing: Stage.menuRowLead
                        Row {
                            id: historyHeader
                            spacing: Stage.menuApart
                            SectionLabel { text: I18n.tr("RECENTLY PLAYED", I18n.rev) }
                            SectionLabel { text: "· " + behind.count; color: Theme.color.immersiveTextGhost; font.weight: Type.medium; font.letterSpacing: Stage.nowTracking }
                        }
                        Item {
                            width: parent.width
                            height: parent.height - historyHeader.implicitHeight - Stage.menuRowLead // derived: under its header
                        ListView {
                                id: historyList
                                objectName: "immersiveHistory"
                                anchors.fill: parent
                                model: behind
                                clip: true
                                spacing: Stage.queueHistoryApart
                                boundsBehavior: Flickable.StopAtBounds
                                cacheBuffer: Stage.queueCache
                                delegate: ImmersiveQueueRow {
                                    required property int index
                                    required property string title
                                    required property string artist
                                    required property string duration
                                    required property url art
                                    width: historyList.width - Stage.queueApart
                                    onPlay: behind.play(index)
                                }
                                ViewportScrollBar { viewport: historyList; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom }
                            }
                        }
                    }
                    Column {
                        visible: behind.count === 0
                        anchors.centerIn: parent
                        width: parent.width
                        spacing: Stage.menuApart
                        Text { width: parent.width; text: I18n.tr("Nothing played yet", I18n.rev); color: Theme.color.immersiveTextDim; font.pointSize: Stage.queueEmptyText; font.weight: Type.semibold; horizontalAlignment: Text.AlignHCenter }
                        Text { width: parent.width; text: I18n.tr("Tracks you play will appear here", I18n.rev); color: Theme.color.immersiveTextGhost; font.pointSize: Stage.queueEmptySubText; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap }
                    }
                }
            }
        }
    }
}

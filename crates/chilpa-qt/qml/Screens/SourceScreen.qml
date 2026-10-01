import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// A plugin's own tree, browsed: the way down to where it stands, a place to
// type, and one listing. One screen for every source; what is in it is the
// plugin's to say.
Item {
    id: page

    property var view: ({})
    readonly property var crumbs: view.crumbs || []
    // Something is on its way, or an episode is playing and its place is moving.
    readonly property bool waiting: view.loading === true || (view.entries || []).some(e => e.resolving || e.sounding && e.finite)
    // Where the listing on screen is from, to keep its scroll while it is the same place.
    property string place
    // True while the screen itself is changing the field, which is not the listener typing.
    property bool settling: false

    function ask(action, id, text) { Sources.command(action, id, text) }
    function read() {
        if (settling) return
        const next = JSON.parse(Sources.snapshot || "{}")
        const mine = next.source === Navigation.routeKey
        const at = mine ? next.source + "/" + (next.crumbs || []).join("/") : ""
        const y = at === place ? list.contentY : 0
        settling = true
        view = mine ? next : ({})
        if (at !== place) find.text = ""
        place = at
        list.contentY = y
        settling = false
    }

    function open() {
        ask("open", Navigation.routeKey, "")
        read()
    }

    Component.onCompleted: open()
    Connections {
        target: Sources
        function onRevisionChanged() { page.read() }
    }
    Connections {
        target: Navigation
        function onRouteKeyChanged() { page.open() }
    }
    Connections {
        target: Player
        function onPathChanged() { Sources.refresh() }
        function onPlayingChanged() { Sources.refresh() }
    }
    Timer {
        interval: Motion.poll
        repeat: true
        running: page.waiting
        onTriggered: Sources.refresh()
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Row {
            objectName: "sourceCrumbs"
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8

            Repeater {
                model: [page.view.name || ""].concat(page.crumbs)

                Text {
                    required property string modelData
                    required property int index
                    text: index > 0 ? "› " + modelData : modelData
                    font.pointSize: Type.title
                    font.weight: Type.bold
                    font.letterSpacing: Type.titleTracking
                    color: index === page.crumbs.length ? Theme.color.textPrimary : Theme.color.textTertiary

                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: page.ask("crumb", String(index), "") }
                }
            }
        }
        IconButton {
            anchors.right: find.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            visible: page.view.placeSavable === true
            round: true
            glyph: page.view.placeSaved ? Icons.heartFilled : Icons.heart
            filled: page.view.placeSaved === true
            tone: page.view.placeSaved ? "accent" : "secondary"
            tip: page.view.placeSaved ? I18n.tr("Remove from saved", I18n.rev) : I18n.tr("Save", I18n.rev)
            onClicked: page.ask("save", "", "")
        }
        SearchField {
            id: find
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.screenSearch * 1.7 // derived: room for what its hint says
            placeholder: I18n.tr("Filter, Enter to search, or a stream URL", I18n.rev)
            // Typing narrows what is on screen; Enter asks the source, or plays an address.
            onTextChanged: if (!page.settling) page.ask("filter", "", text)
            onAccepted: page.ask("search", "", text)
            onEscaped: text = ""
        }
    }

    ListView {
        id: list
        objectName: "sourceList"
        x: Metrics.contentX
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        width: head.width
        clip: true
        model: page.view.entries || []
        // Asked for once the listing has settled: reaching the end happens while the model is being set.
        onAtYEndChanged: if (atYEnd && page.view.more === true) Qt.callLater(page.ask, "more", "", "")

        delegate: SearchHit {
            required property var modelData
            readonly property bool folder: modelData.kind === "folder"
            width: list.width
            kind: 4
            title: modelData.title
            // An episode says its day and its length; whose it is, the header already says.
            detail: !modelData.playable ? modelData.whyNot : modelData.finite && modelData.meta !== "" ? modelData.meta : modelData.subtitle
            art: modelData.image
            duration: modelData.live ? I18n.tr("Live", I18n.rev) : modelData.heard
            current: modelData.sounding || modelData.resolving
            glyph: !folder ? Icons.broadcast : modelData.id === "chilpa:saved" ? Icons.heart : Icons.folder
            endGlyph: folder ? Icons.chevronRight : modelData.finite ? Icons.playlistAdd : modelData.saved ? Icons.heartFilled : Icons.heart
            endOn: modelData.saved && !modelData.finite
            endTip: folder ? I18n.tr("Open", I18n.rev) : modelData.finite ? I18n.tr("Add to queue", I18n.rev) : modelData.saved ? I18n.tr("Remove from saved", I18n.rev) : I18n.tr("Save", I18n.rev)
            onActivated: page.ask("enter", modelData.id, "")
            onEndClicked: page.ask(folder ? "enter" : modelData.finite ? "queue" : "save", modelData.id, "")
            // The right button opens the entry's own page.
            onMenu: if (!folder) Navigation.goKey("sourceEntry", modelData.key)
        }
    }

    Column {
        anchors.centerIn: list
        visible: list.count === 0
        spacing: Metrics.gap12

        Text {
            objectName: "sourceStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            text: page.view.error ? page.view.error : page.view.loading ? I18n.tr("Loading…", I18n.rev) : page.view.total > 0 ? I18n.tr("Nothing here matches.", I18n.rev) : page.view.inSaved ? I18n.tr("Nothing saved yet.", I18n.rev) : I18n.tr("Nothing here.", I18n.rev)
            font.pointSize: Type.small
            color: page.view.error ? Theme.color.statusNegative : Theme.color.textSecondary
        }
        PillButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: !!page.view.error
            text: I18n.tr("Try again", I18n.rev)
            onClicked: page.ask("retry", "", "")
        }
    }
}

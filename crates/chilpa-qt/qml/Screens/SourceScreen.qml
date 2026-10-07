import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
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
    property bool startingPlugins: false

    function ask(action, id, text) { Sources.command(action, id, text) }
    function read() {
        if (settling) return
        const next = JSON.parse(Sources.snapshot || "{}")
        if (startingPlugins && !next.starting) Qt.callLater(page.open)
        startingPlugins = next.starting === true
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
        // The key changes before the screen does: on the way out it is
        // already another screen's, and not a plugin to open.
        if (Navigation.screen !== "source") return
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
        function onRouteKeyChanged() { Qt.callLater(page.open) }
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

        Text {
            objectName: "sourceTitle"
            anchors.verticalCenter: parent.verticalCenter
            text: page.view.name || ""
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        SortButton {
            anchors.right: save.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            visible: page.view.sortable === true
            value: page.view.order || ""
            options: [
                { value: "", text: I18n.tr("Newest first", I18n.rev) },
                { value: "oldest", text: I18n.tr("Oldest first", I18n.rev) },
                { value: "unheard", text: I18n.tr("Not heard first", I18n.rev) }
            ]
            onPicked: (value) => page.ask("order", value, "")
        }
        IconButton {
            id: save
            anchors.right: find.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            // It keeps its room when hidden, so what sits beside it does not jump.
            opacity: page.view.placeSavable === true ? 1 : 0
            enabled: page.view.placeSavable === true
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

    SourceCrumbs {
        id: crumbs
        anchors.top: head.bottom
        x: head.x
        width: head.width
        // Only when it has something to say: a way back, a pick, a shortcut.
        height: page.crumbs.length > 0 || page.view.explorer === true || (page.view.shortcuts || []).length > 0 ? Metrics.breadcrumb : 0
        visible: height > 0
        view: page.view
        onStepped: (index) => page.ask("crumb", String(index), "")
        onCleared: (column) => page.ask("pick", column, "")
        onOpened: (id) => page.ask("enter", id, "")
    }

    Rectangle {
        id: panes
        anchors.top: crumbs.bottom
        x: head.x
        width: head.width
        height: page.view.explorer === true ? Metrics.panesHeight : 0
        visible: height > 0
        radius: Metrics.radiusMenu
        color: Theme.color.strokeHairline
        clip: true

        Row {
            anchors.fill: parent
            anchors.margins: 1
            spacing: 1

            // By count, not by the columns themselves: each snapshot is a new
            // object, and a column drawn again would lose what was typed in it.
            Repeater {
                model: (page.view.columns || []).length

                SourceColumn {
                    required property int index
                    objectName: "sourceColumn"
                    width: (panes.width - 2 - (page.view.columns.length - 1)) / page.view.columns.length // derived: the seams shared out
                    height: parent.height
                    column: page.view.columns[index] || ({})
                    onPicked: (id) => page.ask("pick", column.id, id)
                }
            }
        }
    }

    ListView {
        id: list
        objectName: "sourceList"
        x: Metrics.contentX
        anchors.top: panes.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        width: head.width
        clip: true
        model: page.view.entries || []
        header: SourceAbout {
            width: list.width
            entry: page.view.about || null
        }
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

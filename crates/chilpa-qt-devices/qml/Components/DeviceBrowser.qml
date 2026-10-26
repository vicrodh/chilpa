import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: panel
    property var document: ({})
    onDocumentChanged: if (search && search.text !== (document.query || "")) search.text = document.query || ""
    property bool busy: false
    readonly property string location: [document.source,document.view,document.artist,document.album,document.page].join("/")
    onLocationChanged: list.positionViewAtBeginning()
    signal albumAsked(string id)
    signal backAsked()
    signal filterEdited(string text)
    signal pageAsked(int page)
    signal reloadAsked()
    signal viewAsked(string view)
    radius: Metrics.radiusMenu
    color: Theme.color.surfacePane
    border.color: Theme.color.strokeHairline
    border.width: Metrics.hairline

    Column {
        id: head
        x: Metrics.gap8; y: Metrics.gap8
        width: parent.width - Metrics.gap8 * 2 // derived: equal insets
        spacing: Metrics.gap8
        Row {
            width: parent.width
            spacing: Metrics.gap8
            IconButton { objectName: "deviceBrowserBack"; glyph: Icons.chevronLeft; tip: I18n.tr("Back", I18n.rev); visible: panel.document.can_back === true; enabled: !panel.busy; onClicked: panel.backAsked() }
            Text {
                width: Math.max(0, parent.width - (panel.document.can_back ? Metrics.button28 + Metrics.gap8 : 0) - Metrics.button28 - Metrics.gap8)
                height: Metrics.button28
                verticalAlignment: Text.AlignVCenter
                text: panel.document.album ? panel.document.title + " › " + panel.document.album : panel.document.title || I18n.tr("Music", I18n.rev)
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.color.textPrimary
                font.pointSize: Type.body
                font.weight: Type.medium
            }
            IconButton { objectName: "deviceBrowserReload"; glyph: Icons.refresh; tip: I18n.tr("Refresh", I18n.rev); enabled: !panel.busy; onClicked: panel.reloadAsked() }
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: panel.document.notice || ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.color.textSecondary
            font.pointSize: Type.small
        }
        DeviceNotice { width: parent.width; visible: panel.document.error !== null && panel.document.error !== undefined; error: panel.document.error || null; busy: panel.busy }
        Row {
            width: parent.width
            spacing: Metrics.gap8
            SelectField {
                objectName: "deviceBrowserView"
                width: (parent.width - Metrics.gap8) / 2 // derived: two equal fields and their gap
                options: [I18n.tr("Music", I18n.rev), I18n.tr("Artists", I18n.rev), I18n.tr("Albums", I18n.rev), I18n.tr("Tracks", I18n.rev), I18n.tr("Playlists", I18n.rev)]
                values: ["music", "artists", "albums", "tracks", "playlists"]
                value: panel.document.view || "music"
                enabled: !panel.busy
                onPicked: value => panel.viewAsked(value)
            }
            SearchField {
                id: search
                objectName: "deviceBrowserSearch"
                width: (parent.width - Metrics.gap8) / 2 // derived: two equal fields and their gap
                text: panel.document.query || ""
                placeholder: I18n.tr("Search this view…", I18n.rev)
                enabled: !panel.busy
                onTextChanged: if (text !== (panel.document.query || "")) panel.filterEdited(text)
            }
        }
    }
    KeyedModel { id: music; keyField: "id"; items: panel.document.rows || [] }
    ListView {
        id: list
        objectName: "deviceMusicList"
        x: Metrics.gap4
        y: head.y + head.height + Metrics.gap8
        width: parent.width - Metrics.gap4 * 2 - Metrics.gap16 // derived: insets and scrollbar gutter
        height: Math.max(0, footer.y - y - Metrics.gap4)
        model: music
        clip: true
        reuseItems: true
        cacheBuffer: DeviceLayout.rowHeight * 2 // derived: two adjacent rows
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
        delegate: DeviceMusicRow {
            required property var model
            required property int index
            width: list.width
            rowIndex: index
            entry: model.entry
            busy: panel.busy
            onOpened: panel.albumAsked(model.entry.id)
        }
    }
    DevicePager {
        id: footer
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap4
        page: panel.document.page || 0
        pages: panel.document.pages || 1
        busy: panel.busy
        onPageAsked: page => panel.pageAsked(page)
    }
    Text {
        anchors.centerIn: list
        width: list.width - Metrics.gap16
        visible: (panel.document.rows || []).length === 0 && !panel.document.error
        text: panel.document.loading ? I18n.tr("Loading music…", I18n.rev) : panel.document.query ? I18n.tr("No matching music.", I18n.rev) : panel.document.recorded ? ((panel.document.view && panel.document.view !== "music") || panel.document.album ? I18n.tr("No matching music.", I18n.rev) : I18n.tr("No music has been synced with Chilpa to this storage yet.", I18n.rev)) : I18n.tr("This selection has no tracks.", I18n.rev)
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    CutTextTips { watching: panel }
}

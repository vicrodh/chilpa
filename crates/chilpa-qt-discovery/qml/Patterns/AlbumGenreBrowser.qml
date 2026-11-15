import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.discovery
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The owner loads this browser on demand and destroys it after closing.
Modal {
    id: browser
    objectName: "albumGenreBrowser"
    property double albumId: 0
    readonly property var document: decode(results.document)
    signal finished()
    preferredWidth: Metrics.settingsMaxWidth
    preferredHeight: Metrics.albumGenresHeight
    function decode(value) { try { return JSON.parse(value || "{}") } catch (_) { return {} } }
    function query() {
        if (!visible || !albumId) return
        results.request("album_genres", String(albumId), search.text.trim(), "{}", Library.contentRev + Library.remoteRev)
    }
    function seek() {
        if (!visible || !results.count) return
        results.seek(Math.max(0, Math.floor(rows.contentY / Metrics.rowRich)), Math.ceil((rows.contentY + rows.height) / Metrics.rowRich))
    }
    onOpened: search.forceActiveFocus()
    onVisibleChanged: {
        if (visible) query()
        else { settle.stop(); results.cancel() }
    }
    onAlbumIdChanged: close()
    onClosed: finished()
    Component.onDestruction: { settle.stop(); results.cancel() }
    Connections {
        target: Library
        function onContentRevChanged() { if (browser.visible) browser.query() }
        function onRemoteRevChanged() { if (browser.visible) browser.query() }
    }
    DiscoveryModel {
        id: results
        objectName: "albumGenreModel"
        onCountChanged: Qt.callLater(browser.seek)
    }
    Timer { id: settle; interval: Motion.searchSettle; onTriggered: browser.query() }
    Column {
        anchors.fill: parent
        spacing: Metrics.gap12
        Row {
            width: parent.width
            Text {
                width: parent.width - dismiss.width
                text: I18n.tr("Genres", I18n.rev)
                font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary
            }
            IconButton { id: dismiss; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: browser.close() }
        }
        SearchField {
            id: search
            objectName: "albumGenreSearch"
            width: parent.width
            maximumLength: Metrics.discoverySearchChars
            placeholder: I18n.tr("Search genres", I18n.rev)
            onTextChanged: if (browser.visible) settle.restart()
            onEscaped: text = ""
        }
        Item {
            width: parent.width
            height: parent.height - y
            ListView {
                id: rows
                objectName: "albumGenreRows"
                anchors.fill: parent
                model: results.count
                clip: true; reuseItems: true
                cacheBuffer: Metrics.rowRich * 2
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ViewportScrollBar {}
                onContentYChanged: browser.seek()
                onHeightChanged: Qt.callLater(browser.seek)
                delegate: Button {
                    id: row
                    required property int index
                    readonly property var entry: browser.decode(results.entry(index, results.revision))
                    objectName: "albumGenreResult:" + index
                    width: rows.width; height: Metrics.rowRich
                    enabled: !!entry.key
                    hoverEnabled: true
                    Accessible.name: entry.name || ""
                    onClicked: { const key=entry.key; browser.close(); Navigation.goKey("genres", key) }
                    Keys.onReturnPressed: clicked()
                    Keys.onEnterPressed: clicked()
                    background: Rectangle {
                        radius: Metrics.radiusRow
                        color: row.down ? Theme.color.stateSelected : row.hovered ? Theme.color.stateHover : "transparent"
                        FocusRing { visible: row.activeFocus }
                    }
                    contentItem: Text {
                        text: row.entry.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter; leftPadding: Metrics.gap12; rightPadding: Metrics.gap12
                        color: Theme.color.textPrimary; font.pointSize: Type.body
                    }
                }
            }
            Column {
                anchors.centerIn: parent
                width: parent.width
                visible: results.loading || !!browser.document.error || results.count === 0
                spacing: Metrics.gap12
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                    text: results.loading ? I18n.tr("Loading…", I18n.rev) : browser.document.error ? I18n.tr("Could not read the library", I18n.rev) : I18n.tr("No matches. Try another search.", I18n.rev)
                    color: Theme.color.textSecondary; font.pointSize: Type.body
                }
                PillButton { anchors.horizontalCenter: parent.horizontalCenter; visible: !!browser.document.error; text: I18n.tr("Retry", I18n.rev); onClicked: browser.query() }
            }
        }
    }
}

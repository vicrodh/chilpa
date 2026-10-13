import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

Item {
    id: view
    objectName: "pluginDownloads"
    required property var document
    required property var plugin
    property Component purchaseCard: null
    property bool busy: false
    property string problem: ""
    property string tab: "albums"
    property string mode: "grid"
    property string order: "purchased"
    property string quality: "all"
    property bool hideUnavailable: false
    property string formatChoice: ""
    property string selectedPurchaseKey: ""
    signal back()
    signal account()
    signal chooseFolder()
    signal action(var request)

    readonly property var selection: document.selected || null
    readonly property var progress: document.progress || ({})
    readonly property var formats: selection ? selection.formats || [] : []
    readonly property var albums: document.albums || []
    readonly property var tracks: document.tracks || []
    P.KeyedModel { id: purchaseRows; items: view.rows }
    readonly property var rows: {
        const source = tab === "albums" ? albums : tracks
        const needle = search.text.trim().toLocaleLowerCase()
        const filtered = source.filter(item => {
            if (hideUnavailable && item.downloadable === false) return false
            if (quality !== "all" && !(item.formats || []).some(f => quality === "dsd" ? f.id === 55 || f.id === 56
                    : quality === "hires" ? f.id === 7 || f.id === 27 : f.id === 6 || f.id === 5)) return false
            return !needle || [item.title, item.artist, item.album].some(text => String(text || "").toLocaleLowerCase().indexOf(needle) >= 0)
        })
        filtered.sort((a, b) => order === "title" ? String(a.title || "").localeCompare(String(b.title || ""))
            : order === "artist" ? String(a.artist || "").localeCompare(String(b.artist || ""))
            : Number(b.purchased_at || 0) - Number(a.purchased_at || 0))
        return filtered
    }
    onSelectionChanged: {
        const key = selection ? String(selection.kind) + ":" + String(selection.id) : ""
        if (key !== selectedPurchaseKey || !formats.some(format => String(format.id) === formatChoice))
            formatChoice = formats.length ? String(formats[0].id) : ""
        selectedPurchaseKey = key
    }

    Column {
        id: header
        x: Metrics.contentX; y: Metrics.contentTop
        width: Math.max(0, parent.width - Metrics.contentX * 2) // derived: page padding on both sides
        spacing: Metrics.gap12
        Row {
            width: parent.width
            spacing: Metrics.gap12
            P.PillButton { id: backButton; text: view.selection ? I18n.tr("Back to purchases", I18n.rev) : I18n.tr("Back to plugins", I18n.rev); onClicked: view.selection ? view.action({action:"back"}) : view.back() }
            Text { width: Math.max(0, parent.width - backButton.width - Metrics.gap12); text: view.plugin.name || I18n.tr("Purchases", I18n.rev); elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.titleSmall; anchors.verticalCenter: parent.verticalCenter }
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            Text {
                width: parent.width
                text: view.plugin.export_folder || I18n.tr("No download folder selected", I18n.rev)
                textFormat: Text.PlainText; elide: Text.ElideMiddle
                color: Theme.color.textSecondary; font.pointSize: Type.body
            }
            P.PillButton { id: folder; text: I18n.tr("Choose download folder…", I18n.rev); enabled: !view.busy; onClicked: view.chooseFolder() }
            P.PillButton { id: addLibrary; text: I18n.tr("Add folder to library", I18n.rev); enabled: !!view.plugin.export_folder; onClicked: LibraryFolders.add(view.plugin.export_folder) }
            P.PillButton { id: accountButton; text: I18n.tr("Account", I18n.rev); onClicked: view.account() }
        }
        Text {
            width: parent.width
            visible: view.problem !== "" || !!view.document.error
            text: view.problem || view.document.error || ""
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            color: Theme.color.statusWarning; font.pointSize: Type.body
        }
        Text {
            width: parent.width
            visible: !!view.document.report
            text: view.document.report || ""
            textFormat: Text.PlainText; color: Theme.color.textSecondary; font.pointSize: Type.body
        }
        Row {
            visible: view.document.working === true
            spacing: Metrics.gap12
            Text {
                text: I18n.trArgs("Downloading {} of {}", [view.progress.completed || 0, view.progress.total || 0], I18n.rev)
                    + (view.progress.current ? " · " + view.progress.current : "")
                color: Theme.color.textPrimary; font.pointSize: Type.body
                anchors.verticalCenter: parent.verticalCenter
            }
            P.PillButton { text: I18n.tr("Cancel download", I18n.rev); onClicked: view.action({action:"cancel"}) }
        }
        Rectangle {
            visible: view.document.working === true
            width: parent.width; height: Metrics.gap6; radius: Metrics.gap4
            color: Theme.color.fillField
            Rectangle {
                width: parent.width * Math.min(1, (view.progress.completed || 0) / Math.max(1, view.progress.total || 0))
                height: parent.height; radius: parent.radius; color: Theme.color.accent
            }
        }
        Row {
            visible: !view.selection && view.document.signed_in === true
            spacing: Metrics.gap8
            P.PillButton { text: I18n.trArgs("Albums ({})", [view.albums.length], I18n.rev); kind: view.tab === "albums" ? "primary" : "secondary"; onClicked: view.tab = "albums" }
            P.PillButton { text: I18n.trArgs("Tracks ({})", [view.tracks.length], I18n.rev); kind: view.tab === "tracks" ? "primary" : "secondary"; onClicked: view.tab = "tracks" }
            P.PillButton { text: I18n.tr("Refresh purchases", I18n.rev); enabled: !view.busy; onClicked: view.action({action:"refresh"}) }
        }
        Flow {
            width: parent.width
            visible: !view.selection && view.document.signed_in === true && view.document.loaded === true
            spacing: Metrics.gap8
            P.TextField { id: search; width: Math.max(Metrics.cardMinWidth, parent.width - sort.width - filter.width - grid.width - list.width - Metrics.gap24 - Metrics.gap8); placeholder: I18n.tr("Search purchases", I18n.rev) }
            P.SelectField { id: sort; options: [I18n.tr("Purchase date", I18n.rev), I18n.tr("Title", I18n.rev), I18n.tr("Artist", I18n.rev)]; values: ["purchased", "title", "artist"]; value: view.order; onPicked: value => view.order = value }
            P.SelectField { id: filter; options: [I18n.tr("All qualities", I18n.rev), "DSD", "Hi-Res", "CD / MP3"]; values: ["all", "dsd", "hires", "standard"]; value: view.quality; onPicked: value => view.quality = value }
            P.PillButton { id: grid; text: I18n.tr("Grid", I18n.rev); kind: view.mode === "grid" ? "primary" : "secondary"; visible: view.tab === "albums"; onClicked: view.mode = "grid" }
            P.PillButton { id: list; text: I18n.tr("List", I18n.rev); kind: view.mode === "list" ? "primary" : "secondary"; visible: view.tab === "albums"; onClicked: view.mode = "list" }
        }
        Row {
            visible: !view.selection && view.tab === "albums" && view.document.loaded === true
            spacing: Metrics.gap8
            P.Toggle { checked: view.hideUnavailable; onToggled: on => view.hideUnavailable = on }
            Text { text: I18n.tr("Hide unavailable", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.body; anchors.verticalCenter: parent.verticalCenter }
            Text { text: I18n.trArgs("{} shown", [view.rows.length], I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.meta; anchors.verticalCenter: parent.verticalCenter }
        }
    }

    Text {
        anchors.centerIn: parent
        visible: view.document.signed_in === false
        text: I18n.tr("Sign in from Account to see your purchases.", I18n.rev)
        color: Theme.color.textSecondary; font.pointSize: Type.body
    }
    Text {
        anchors.centerIn: parent
        visible: view.document.loading === true
        text: I18n.tr("Loading purchases…", I18n.rev)
        color: Theme.color.textSecondary; font.pointSize: Type.body
    }
    Text {
        anchors.centerIn: parent
        visible: !view.selection && view.document.loaded === true && !view.document.error && view.rows.length === 0
        text: I18n.tr("No purchases match the current view.", I18n.rev)
        color: Theme.color.textSecondary; font.pointSize: Type.body
    }

    GridView {
        id: cards
        objectName: "purchaseGrid"
        anchors.top: header.bottom; anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.contentBottom
        x: header.x; width: header.width
        visible: !view.selection && view.document.loaded === true && view.tab === "albums" && view.mode === "grid"
        clip: true; boundsBehavior: Flickable.StopAtBounds
        cellWidth: CardLayout.cardWidth + Metrics.gridGap
        cellHeight: cellWidth - Metrics.gridGap + Metrics.cardTextGap + Metrics.cardTextHeight + Metrics.gridGap
        model: purchaseRows
        ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8 }
        delegate: Loader {
            id: cardLoader
            required property var entry
            readonly property var modelData: entry
            width: cards.cellWidth - Metrics.gridGap; height: cards.cellHeight - Metrics.gridGap
            sourceComponent: view.purchaseCard
            Binding { target: cardLoader.item; property: "record"; value: Object.assign({}, cardLoader.modelData, {plugin:view.plugin.id}); when: !!cardLoader.item }
            Connections { target: cardLoader.item; function onOpened() { view.action({action:"select_album",id:cardLoader.modelData.id}) } }
        }
    }

    ListView {
        id: listRows
        anchors.top: header.bottom; anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.contentBottom
        x: header.x; width: header.width
        visible: !view.selection && view.document.loaded === true && (view.tab === "tracks" || view.mode === "list")
        clip: true; reuseItems: true; boundsBehavior: Flickable.StopAtBounds
        model: purchaseRows
        ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8 }
        delegate: Item {
            required property var entry
            readonly property var modelData: entry
            required property int index
            width: listRows.width - Metrics.gap12; height: Metrics.rowRich
            P.ListRowSurface { anchors.fill: parent; rowIndex: index; radius: Metrics.radiusRow; hovered: hover.hovered }
            P.ArtTile {
                id: art
                x: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter
                width: Metrics.artRow; height: width; radius: Metrics.radiusArtSmall
                source: modelData.image || ""
            }
            Column {
                anchors.left: art.right; anchors.leftMargin: Metrics.gap12
                anchors.right: parent.right; anchors.rightMargin: Metrics.gap12
                anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap4
                Text { width: parent.width; text: modelData.title || ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.semibold }
                Text { width: parent.width; text: [modelData.artist, modelData.album || modelData.year].filter(Boolean).join(" · "); textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.meta }
            }
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: view.action({action:view.tab === "albums" ? "select_album" : "select_track",id:modelData.id}) }
        }
    }

    PluginPurchaseDetail {
        anchors.top: header.bottom; anchors.topMargin: Metrics.gap16
        anchors.bottom: parent.bottom
        x: header.x; width: header.width
        visible: !!view.selection
        selection: view.selection
        document: view.document
        busy: view.busy
        formatChoice: view.formatChoice
        onFormatPicked: value => view.formatChoice = value
        onAction: request => view.action(request)
    }
}

import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"
import "../Patterns"

// A page of shelves: a source's own (Discover), or a section's "All" of its
// tabs. Each shelf is the same carousel of records as a record page's, with a
// way to see it whole; tracks, which are not cards, are a link.
ListView {
    id: page

    // "<plugin>" or "<plugin> section:<name>".
    property string key
    property bool presented: false
    Connections { target: page.Window.window; function onFrameSwapped() { if (page.visible) page.presented = true } }
    readonly property string document: Sources.page(key, Library.remoteRev) || "{}"
    readonly property var about: JSON.parse(document)
    readonly property var shelves: about.tab ? discovery.shown : about.shelves || []
    // The shelves by key, changed only when a shelf comes or goes: a page
    // read again, or a shelf that loaded more, keeps every shelf's item on
    // screen where it was, and only what changed is drawn again.
    KeyedModel { id: shelfRows; items: page.shelves }

    model: shelfRows
    spacing: Metrics.gap16
    cacheBuffer: height * 1.25 // derived: the same viewport runway as Home
    topMargin: Metrics.contentTop
    bottomMargin: Metrics.contentBottom
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    header: Item {
        width: page.width
        height: column.height + Metrics.gap16
        Column {
            id: column
            x: Metrics.contentX
            width: page.width - Metrics.contentX * 2 // derived: padding on both sides
            spacing: Metrics.gap16

            Text {
                visible: !page.about.header
                text: page.about.title || ""
                font.pointSize: Type.title
                font.weight: Type.bold
                font.letterSpacing: Type.titleTracking
                color: Theme.color.textPrimary
            }
            Item {
                width: parent.width
                height: Math.max(tabs.height + Metrics.gap8, discovery.visible ? discovery.height : 0)
                Flickable {
                    anchors.left: parent.left; anchors.right: discovery.left; anchors.rightMargin: Metrics.gap12
                    height: tabs.height + Metrics.gap8; contentWidth: tabs.width; clip: true
                    flickableDirection: Flickable.HorizontalFlick
                    ScrollBar.horizontal: ViewportScrollBar {}
                    SourceTabs { id: tabs; model: page.about.tabs || [] }
                }
                SourceDiscoveryControls { id: discovery; anchors.right: parent.right; about: page.about; visible: !!page.about.tab }
            }
            SourcePageHeader { width: parent.width; currentPage: true; tracksKey: ((page.about.shelves || []).find(shelf => shelf.tracks) || {}).key || ""; info: page.about.header || ({}); visible: !!page.about.header }

            SourceState {
                width: column.width
                visible: (page.about.shelves || []).length === 0
                source: page.about.native_page ? page.about.plugin + " native:" + page.about.reference : (page.key.indexOf(" ") < 0 ? page.key + " root" : page.key)
                about: page.about
            }
        }
    }
    delegate: Item {
        required property var entry
        width: page.width
        height: module.height
        HomeSection {
            id: module
            x: Metrics.contentX
            width: page.width - Metrics.contentX * 2 // derived: padding on both sides
            entry: Object.assign({}, parent.entry, {id:"native:" + page.key + ":" + parent.entry.id, kind:"shelf", entityKind:parent.entry.kind})
            started: page.presented
            content: sourceModule
        }
    }
    Component {
        id: sourceModule
        SourceShelf {
            property var entry: ({})
            section: Object.assign({}, entry, {kind:entry.entityKind || entry.kind})
            wrap: (page.about.header || {}).kind === "artist" && section.kind === "album"
        }
    }
}

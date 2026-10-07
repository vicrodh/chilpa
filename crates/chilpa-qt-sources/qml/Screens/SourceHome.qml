import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Components"
import "../Components"
import "../Patterns"

// A page of shelves: a source's own (Discover), or a section's "All" of its
// tabs. Each shelf is the same carousel of records as a record page's, with a
// way to see it whole; tracks, which are not cards, are a link.
Flickable {
    id: page

    // "<plugin>" or "<plugin> section:<name>".
    property string key
    readonly property string document: Sources.page(key, Library.remoteRev) || "{}"
    readonly property var about: JSON.parse(document)
    readonly property var shelves: about.tab ? discovery.shown : about.shelves || []
    // The shelves by key, changed only when a shelf comes or goes: a page
    // read again, or a shelf that loaded more, keeps every shelf's item on
    // screen where it was, and only what changed is drawn again.
    property var shelfKeys: []
    onShelvesChanged: {
        const keys = shelves.map(shelf => shelf.key || shelf.id)
        if (keys.join("\n") !== shelfKeys.join("\n"))
            shelfKeys = keys
    }
    // The first list is there before any change can say so.
    Component.onCompleted: shelvesChanged()
    function shelf(key) { return shelves.find(s => (s.key || s.id) === key) || ({}) }

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
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
        SourceTabs { model: page.about.tabs || [] }

        SourceDiscoveryControls { id: discovery; width: parent.width; about: page.about; visible: !!page.about.tab }
        SourcePageHeader { width: parent.width; currentPage: true; tracksKey: ((page.about.shelves || []).find(shelf => shelf.tracks) || {}).key || ""; info: page.about.header || ({}); visible: !!page.about.header }
        Repeater {
            model: page.shelfKeys
            SourceShelf { required property string modelData; width: column.width; section: page.shelf(modelData); wrap: (page.about.header || {}).kind === "artist" && section.kind === "album" }
        }

        SourceState {
            width: column.width
            visible: (page.about.shelves || []).length === 0
            source: page.about.native_page ? page.about.plugin + " native:" + page.about.reference : (page.key.indexOf(" ") < 0 ? page.key + " root" : page.key)
            about: page.about
        }
    }
}

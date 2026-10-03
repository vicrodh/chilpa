import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
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

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        Text {
            text: page.about.title || ""
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        SourceTabs { model: page.about.tabs || [] }

        SourceDiscoveryControls { id: discovery; width: parent.width; about: page.about; visible: !!page.about.tab }
        SourcePageHeader { width: parent.width; info: page.about.header || ({}); visible: !!page.about.header }
        Repeater {
            model: page.about.tab ? discovery.shown : page.about.shelves || []
            SourceShelf { required property var modelData; width: column.width; section: modelData }
        }

        SourceState {
            width: column.width
            visible: (page.about.shelves || []).length === 0
            source: page.about.native_page ? page.about.plugin + " native:" + page.about.reference : (page.key.indexOf(" ") < 0 ? page.key + " root" : page.key)
            about: page.about
        }
    }
}

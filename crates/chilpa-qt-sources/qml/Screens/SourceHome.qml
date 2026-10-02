import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Components"
import "../Components"

// A page of shelves: a source's own (Discover), or a section's "All" of its
// tabs. Each shelf is the same carousel of records as a record page's, with a
// way to see it whole; tracks, which are not cards, are a link.
Flickable {
    id: page

    // "<plugin>" or "<plugin> section:<name>".
    property string key
    readonly property var about: JSON.parse(Sources.page(key, Library.remoteRev) || "{}")

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

        Repeater {
            model: page.about.shelves || []

            AlbumCarousel {
                id: shelf
                required property var modelData
                width: column.width
                visible: !modelData.tracks && model.albums > 0
                round: modelData.round === true
                title: modelData.title
                link: I18n.tr("See all", I18n.rev)
                onLinkClicked: Navigation.goKey("source", modelData.key)
                model: AlbumGridModel {
                    kind: "source"
                    key: shelf.modelData.key
                    contentRev: Library.remoteRev + Library.remoteArtRev
                }
            }
        }

        Repeater {
            model: (page.about.shelves || []).filter(s => s.tracks)
            SectionHeader {
                required property var modelData
                width: column.width
                title: modelData.title
                link: I18n.tr("See all", I18n.rev)
                onLinkClicked: Navigation.goKey("source", modelData.key)
            }
        }

        SourceState {
            width: column.width
            visible: (page.about.shelves || []).length === 0
            source: page.key.indexOf(" ") < 0 ? page.key + " root" : page.key
            about: page.about
        }
    }
}

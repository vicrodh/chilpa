import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// A source's own page (Discover): a shelf for each of its listings, each the
// same carousel of records as a record page's, and a way to see it whole.
Flickable {
    id: page

    property string plugin
    readonly property var about: JSON.parse(Sources.page(plugin, Library.remoteRev) || "{}")

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

        Repeater {
            model: page.about.shelves || []

            AlbumCarousel {
                id: shelf
                required property var modelData
                width: column.width
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

        SourceState {
            width: column.width
            visible: (page.about.shelves || []).length === 0
            source: page.plugin + " root"
            about: page.about
        }
    }
}

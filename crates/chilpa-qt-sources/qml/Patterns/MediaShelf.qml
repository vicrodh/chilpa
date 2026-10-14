import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Column {
    id: shelf
    property string title: ""
    property var entries: []
    property bool station: false
    signal opened(var entry)
    signal played(var entry)
    signal saved(var entry)
    signal more()
    property int shown: 20
    readonly property int cardCount: Math.max(1, Math.min(shown, entries.length))
    readonly property real farthest: Math.max(0, rail.contentWidth - rail.width)
    readonly property real step: CardLayout.columns(width) * (CardLayout.cardWidth + Metrics.gridGap) // derived: a page of media cards
    spacing: Metrics.gap16
    SectionHeader {
        width: parent.width; title: shelf.title
        link: shelf.entries.length > shelf.shown ? I18n.tr("Load more", I18n.rev) : ""
        onLinkClicked: shelf.shown += 20
        paged: shelf.farthest > 0
        canBack: rail.contentX > 1; canForward: rail.contentX < shelf.farthest - 1
        onBack: rail.contentX = Math.max(0, rail.contentX - shelf.step)
        onForward: rail.contentX = Math.min(shelf.farthest, rail.contentX + shelf.step)
    }
    Flickable {
        id: rail
        objectName: "mediaShelfRail"
        width: parent.width; height: cards.implicitHeight + Metrics.gap8
        contentWidth: cards.width; contentHeight: height
        interactive: contentWidth > width
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds; clip: true
        ScrollBar.horizontal: ViewportScrollBar {}
        MediaGrid {
            id: cards
            width: shelf.cardCount * CardLayout.cardWidth + (shelf.cardCount - 1) * Metrics.gridGap // derived: one row of media cards
            fixedColumns: shelf.cardCount
            entries: shelf.entries.slice(0, shelf.shown); station: shelf.station
            onOpened: entry => shelf.opened(entry); onPlayed: entry => shelf.played(entry); onSaved: entry => shelf.saved(entry)
        }
    }
}

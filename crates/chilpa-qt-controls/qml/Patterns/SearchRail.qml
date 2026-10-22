import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"

// A section of the results page as a rail of cards (QBZ's carousels), paged
// by the header's arrows, with "See all" to the section's own tab.
Column {
    id: rail
    property string title
    // The section's rows, each with its place in the search model (`at`).
    property var rows: []
    signal activated(int at)
    signal menuAsked(int at, real x, real y)
    signal viewAll()
    readonly property real step: CardLayout.columns(width) * (CardLayout.cardWidth + Metrics.gridGap) // derived: a page of cards
    readonly property real farthest: Math.max(0, cards.contentWidth - cards.width)
    visible: rows.length > 0
    spacing: Metrics.gap8

    SectionHeader {
        width: parent.width
        title: rail.title
        link: I18n.tr("See all", I18n.rev)
        paged: rail.farthest > 0
        canBack: cards.contentX > 1
        canForward: cards.contentX < rail.farthest - 1
        onBack: cards.contentX = Math.max(0, cards.contentX - rail.step)
        onForward: cards.contentX = Math.min(rail.farthest, cards.contentX + rail.step)
        onLinkClicked: rail.viewAll()
    }
    ListView {
        id: cards
        width: parent.width
        height: CardLayout.cardHeight + Metrics.gap8
        orientation: ListView.Horizontal
        spacing: Metrics.gridGap
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: rail.rows
        ScrollBar.horizontal: ViewportScrollBar {}
        delegate: SearchResultCard {
            required property var modelData
            row: modelData
            onActivated: rail.activated(modelData.at)
            onMenuAsked: (x, y) => {
                const point = mapToItem(rail, x, y)
                rail.menuAsked(modelData.at, point.x, point.y)
            }
        }
    }
}

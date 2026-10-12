import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The reference scene's three-column filter with independent genre choices.
Rectangle {
    id: filter
    property var genres: []
    property var selected: []
    signal toggled(string genre)
    signal cleared()
    signal closed()
    height: Math.min(Metrics.sceneGenreMaxHeight, body.implicitHeight + Metrics.gap24)
    radius: Metrics.radiusPanel; color: Theme.color.surfaceRaised
    Edge {}
    MouseArea { anchors.fill: parent; cursorShape: Qt.ArrowCursor }
    onVisibleChanged: if (!visible) genreSearch.text = ""
    Column {
        id: body; x: Metrics.gap12; y: Metrics.gap12; width: parent.width - Metrics.gap24; spacing: Metrics.gap12
        Row {
            width: parent.width
            Text { id: title; text: I18n.tr("Filter by genre", I18n.rev); font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
            Item { width: parent.width - title.width - closeGenre.width; height: 1 }
            IconButton { id: closeGenre; size: Metrics.button24; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: filter.closed() }
        }
        SearchField { id: genreSearch; visible: filter.genres.length > 9; width: parent.width; placeholder: I18n.tr("Search genres", I18n.rev) }
        Flickable {
            width: parent.width; height: Math.min(Metrics.sceneGenreListHeight, genreGrid.height)
            contentWidth: width; contentHeight: genreGrid.height; clip: true
            Grid {
                id: genreGrid; width: parent.width; columns: Metrics.sceneGenreColumns; spacing: Metrics.gap8
                Repeater {
                    model: filter.genres.filter(g => g.toLowerCase().includes(genreSearch.text.toLowerCase()))
                    Rectangle {
                        id: choice
                        required property string modelData
                        readonly property bool chosen: filter.selected.includes(modelData)
                        width: (genreGrid.width - Metrics.gap16) / Metrics.sceneGenreColumns; height: Metrics.sceneGenreChoiceHeight
                        radius: Metrics.radiusControl
                        color: chosen ? Theme.color.stateSelected : hover.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
                        border.width: 1; border.color: chosen ? Theme.color.accent : Theme.color.strokeHairline
                        Text {
                            anchors.left: parent.left; anchors.leftMargin: Metrics.gap10
                            anchors.right: radio.left; anchors.rightMargin: Metrics.gap6
                            anchors.verticalCenter: parent.verticalCenter
                            text: choice.modelData.charAt(0).toUpperCase() + choice.modelData.slice(1)
                            textFormat: Text.PlainText; elide: Text.ElideRight
                            font.pointSize: Type.small; color: Theme.color.textPrimary
                        }
                        Rectangle {
                            id: radio
                            anchors.right: parent.right; anchors.rightMargin: Metrics.gap8
                            anchors.verticalCenter: parent.verticalCenter
                            width: Metrics.sceneGenreRadio; height: width; radius: width * Metrics.sceneCircleRadius
                            color: "transparent"; border.width: 1
                            border.color: choice.chosen ? Theme.color.accent : Theme.color.textSecondary
                            Rectangle {
                                visible: choice.chosen; anchors.centerIn: parent
                                width: Metrics.sceneGenreDot; height: width; radius: width * Metrics.sceneCircleRadius
                                color: Theme.color.accent
                            }
                        }
                        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: filter.toggled(choice.modelData) }
                    }
                }
            }
        }
        PillButton { visible: filter.selected.length > 0; text: I18n.tr("Clear filters", I18n.rev); onClicked: filter.cleared() }
    }
}

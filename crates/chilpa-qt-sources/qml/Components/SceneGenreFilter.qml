import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The scene's own genre box: three-column style, frequency-ordered genres,
// live search above nine, and a multi-selection held until it is closed.
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
            width: parent.width; height: Math.min(Metrics.sceneGenreListHeight, genreFlow.height); contentWidth: width; contentHeight: genreFlow.height; clip: true
            Flow {
                id: genreFlow; width: parent.width; spacing: Metrics.gap8
                Repeater {
                    model: filter.genres.filter(g => g.toLowerCase().includes(genreSearch.text.toLowerCase()))
                    PillButton {
                        required property string modelData
                        text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                        kind: filter.selected.includes(modelData) ? "primary" : "secondary"
                        onClicked: filter.toggled(modelData)
                    }
                }
            }
        }
        PillButton { visible: filter.selected.length > 0; text: I18n.tr("Clear filters", I18n.rev); onClicked: filter.cleared() }
    }
}

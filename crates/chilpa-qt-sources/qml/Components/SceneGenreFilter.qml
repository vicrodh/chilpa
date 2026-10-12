import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// QBZ's scene filter: a flat inventory from the current result, three fixed
// columns, independent selections, and a footer that clears and closes.
Rectangle {
    id: filter
    property var genres: []
    property var selected: []
    signal toggled(string genre)
    signal cleared()
    signal closed()
    width: Metrics.sceneGenreWidth
    readonly property bool searchShown: genres.length > 9
    readonly property var shownGenres: genres.filter(g => g.toLowerCase().includes(searchInput.text.toLowerCase()))
    height: Math.min(Metrics.sceneGenreMaxHeight, header.height + (searchShown ? searchRow.height : 0)
        + gridBand.contentHeight + footer.height)
    radius: Metrics.radiusMenu; color: Theme.color.surfaceRaised
    border.width: Metrics.hairline; border.color: Theme.color.strokeHairline
    onVisibleChanged: if (!visible) searchInput.text = ""
    MouseArea { anchors.fill: parent; cursorShape: Qt.ArrowCursor }

    Item {
        id: header
        width: parent.width; height: Metrics.sceneFilterHeader
        Row {
            x: Metrics.gap16; anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
            Icon { glyph: Icons.adjustmentsHorizontal; size: Metrics.iconMd; color: Theme.color.textPrimary; anchors.verticalCenter: parent.verticalCenter }
            Text { text: I18n.tr("Filter by genre", I18n.rev); font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary; anchors.verticalCenter: parent.verticalCenter }
        }
        IconButton {
            anchors.right: parent.right; anchors.rightMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.x; size: Metrics.button28; tip: I18n.tr("Close", I18n.rev)
            onClicked: filter.closed()
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    }
    Item {
        id: searchRow
        visible: filter.searchShown
        y: header.height; width: parent.width; height: Metrics.sceneFilterSearch
        Row {
            x: Metrics.gap16; width: parent.width - Metrics.gap16 * Metrics.sceneFilterSides
            anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
            Icon { glyph: Icons.search; size: Metrics.iconSm; color: Theme.color.textSecondary; anchors.verticalCenter: parent.verticalCenter }
            Item {
                width: parent.width - Metrics.iconSm - Metrics.gap8; height: Metrics.fieldHeight
                TextInput {
                    id: searchInput
                    anchors.fill: parent; color: Theme.color.textPrimary; font.pointSize: Type.small
                    verticalAlignment: TextInput.AlignVCenter; clip: true
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter; visible: searchInput.text === ""
                    text: I18n.tr("Search…", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary
                }
            }
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    }
    Item {
        id: gridBand
        y: header.height + (filter.searchShown ? searchRow.height : 0)
        width: parent.width; height: parent.height - y - footer.height
        readonly property int cellWidth: Math.floor((width - Metrics.sceneFilterPad * Metrics.sceneFilterSides
            - Metrics.sceneFilterGap * (Metrics.sceneGenreColumns - 1)) / Metrics.sceneGenreColumns)
        readonly property int contentHeight: Metrics.sceneFilterPad * Metrics.sceneFilterSides + (filter.shownGenres.length > 0 ? grid.height : emptyLabel.height)
        clip: true
        Flickable {
            anchors.fill: parent; contentWidth: width; contentHeight: gridBand.contentHeight
            boundsBehavior: Flickable.StopAtBounds; clip: true
            Grid {
                id: grid
                x: Metrics.sceneFilterPad; y: Metrics.sceneFilterPad
                columns: Metrics.sceneGenreColumns; spacing: Metrics.sceneFilterGap
                Repeater {
                    model: filter.shownGenres
                    Rectangle {
                        id: choice
                        required property string modelData
                        readonly property bool chosen: filter.selected.includes(modelData)
                        width: gridBand.cellWidth; height: Metrics.sceneGenreChoiceHeight
                        radius: Metrics.radiusItem
                        color: chosen ? Theme.color.accent : hover.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
                        border.width: Metrics.hairline
                        border.color: chosen ? Theme.color.accent : Theme.color.strokeHairline
                        Text {
                            x: Metrics.gap10; width: parent.width - Metrics.gap10 * Metrics.sceneFilterSides - radio.width - Metrics.gap8
                            anchors.verticalCenter: parent.verticalCenter
                            text: choice.modelData.charAt(0).toUpperCase() + choice.modelData.slice(1)
                            textFormat: Text.PlainText; elide: Text.ElideRight
                            font.pointSize: Type.small
                            color: choice.chosen ? Theme.color.textOnAccent : Theme.color.textPrimary
                        }
                        Rectangle {
                            id: radio
                            anchors.right: parent.right; anchors.rightMargin: Metrics.gap10
                            anchors.verticalCenter: parent.verticalCenter
                            width: Metrics.sceneGenreRadio; height: width; radius: width * Metrics.sceneCircleRadius
                            color: choice.chosen ? Theme.color.textOnAccent : "transparent"
                            border.width: Metrics.hairline
                            border.color: choice.chosen ? Theme.color.textOnAccent : Theme.color.textSecondary
                            Icon { anchors.centerIn: parent; visible: choice.chosen; glyph: Icons.check; size: Metrics.iconXs; color: Theme.color.accent }
                        }
                        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: filter.toggled(choice.modelData) }
                    }
                }
            }
            Text {
                id: emptyLabel
                visible: filter.shownGenres.length === 0
                x: Metrics.sceneFilterPad; y: Metrics.sceneFilterPad
                width: parent.width - Metrics.sceneFilterPad * Metrics.sceneFilterSides; height: Metrics.sceneFilterEmpty
                text: I18n.tr("No results found", I18n.rev)
                horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                font.pointSize: Type.small; color: Theme.color.textSecondary
            }
        }
    }
    Item {
        id: footer
        y: filter.height - height; width: parent.width; height: Metrics.sceneFilterFooter
        Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
        Rectangle {
            x: Metrics.gap16; y: Metrics.gap12
            width: parent.width - Metrics.gap16 * Metrics.sceneFilterSides; height: Metrics.fieldHeightLarge
            radius: Metrics.radiusItem
            opacity: filter.selected.length > 0 ? 1 : Metrics.disabledOpacity
            color: clearHover.hovered && filter.selected.length > 0 ? Theme.color.stateHover : Theme.color.surfacePanel
            Text { anchors.centerIn: parent; text: I18n.tr("Clear filter", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
            HoverHandler { id: clearHover; cursorShape: filter.selected.length > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor }
            TapHandler { onTapped: if (filter.selected.length > 0) { filter.cleared(); filter.closed() } }
        }
    }
}

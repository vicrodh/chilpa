import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The reference scene's compact search, genre, view and grouping controls.
Row {
    id: toolbar
    property var availableGenres: []
    property var activeGenres: []
    property bool canBrowse: false
    property string mode: "grid"
    property bool groupingEnabled: false
    property bool searchOpen: false
    property alias genreAnchor: genreButton
    signal searchChanged(string query)
    signal genreRequested()
    signal modeRequested()
    signal groupingChosen(bool grouped)
    spacing: Metrics.gap8

    SearchField {
        id: sceneSearch
        visible: toolbar.searchOpen; width: Metrics.sceneSearchWidth
        placeholder: I18n.tr("Search…", I18n.rev)
        onTextChanged: toolbar.searchChanged(text)
        onEscaped: toolbar.searchOpen = false
    }
    IconButton {
        visible: !toolbar.searchOpen; size: Metrics.sceneNavButton
        glyph: Icons.search; tip: I18n.tr("Search artists", I18n.rev)
        onClicked: { toolbar.searchOpen = true; Qt.callLater(sceneSearch.focusInput) }
    }
    IconButton {
        visible: toolbar.searchOpen; size: Metrics.sceneNavButton
        glyph: Icons.x; tip: I18n.tr("Close search", I18n.rev)
        onClicked: { sceneSearch.text = ""; toolbar.searchOpen = false }
    }
    IconButton {
        id: genreButton
        visible: toolbar.availableGenres.length > 1; size: Metrics.sceneNavButton
        glyph: Icons.filter; checked: toolbar.activeGenres.length > 0
        tip: toolbar.activeGenres.length > 0 ? toolbar.activeGenres.join(", ") : I18n.tr("Filter by genre", I18n.rev)
        onClicked: toolbar.genreRequested()
        Rectangle {
            visible: toolbar.activeGenres.length > 0
            x: parent.width - width / 2; y: -height / 2
            width: Metrics.sceneGenreBadge; height: width; radius: width * Metrics.sceneCircleRadius
            color: Theme.color.accent
            Text { anchors.centerIn: parent; text: toolbar.activeGenres.length; font.pointSize: Type.micro; font.weight: Type.bold; color: Theme.color.textOnAccent }
        }
    }
    IconButton {
        visible: toolbar.canBrowse; size: Metrics.sceneNavButton
        glyph: toolbar.mode === "grid" ? Icons.layoutSidebarLeftExpand : Icons.layoutGrid
        tip: toolbar.mode === "grid" ? I18n.tr("Browse view", I18n.rev) : I18n.tr("Grid view", I18n.rev)
        onClicked: toolbar.modeRequested()
    }
    PillButton {
        visible: toolbar.mode === "grid"
        text: toolbar.groupingEnabled ? I18n.tr("Group: A-Z", I18n.rev) : I18n.tr("Group: Off", I18n.rev)
        onClicked: groupMenu.open()
    }
    Menu {
        id: groupMenu
        entries: [{text:I18n.tr("Group: Off", I18n.rev),checked:!toolbar.groupingEnabled},
            {text:I18n.tr("Group: A-Z", I18n.rev),checked:toolbar.groupingEnabled}]
        onChosen: (index) => toolbar.groupingChosen(index === 1)
    }
}

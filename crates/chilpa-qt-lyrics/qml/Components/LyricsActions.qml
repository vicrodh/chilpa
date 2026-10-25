import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I keep lyric actions above the source footer, with their menu to the left.
Item {
    id: actions
    property var lyrics: null
    property bool translationAvailable: false
    property bool translating: false
    readonly property bool found: lyrics !== null && !!lyrics.document && !!lyrics.revision
    signal translationToggled()
    implicitWidth: Metrics.button28
    implicitHeight: (translationAvailable ? translationButton.height : 0) + (found ? more.height : 0)
        + (translationAvailable && found ? Metrics.gap6 : 0)
    onLyricsChanged: if (actionsMenu) actionsMenu.close()
    onVisibleChanged: if (!visible && actionsMenu) actionsMenu.close()

    IconButton {
        id: translationButton
        objectName: "translateButton"
        visible: actions.translationAvailable
        tip: actions.translating ? I18n.tr("Hide translation", I18n.rev) : I18n.tr("Show translation", I18n.rev)
        glyph: Lucide.languages
        iconFamily: Lucide.font
        round: true
        checked: actions.translating
        onClicked: actions.translationToggled()
    }
    IconButton {
        id: more
        objectName: "lyricsActionsButton"
        y: actions.translationAvailable ? translationButton.height + Metrics.gap6 : 0
        visible: actions.found
        tip: I18n.tr("More options", I18n.rev)
        glyph: Lucide.ellipsis
        iconFamily: Lucide.font
        round: true
        checked: actionsMenu.opened
        onClicked: actionsMenu.opened ? actionsMenu.close() : actionsMenu.open()
    }
    Menu {
        id: actionsMenu
        objectName: "lyricsActionsMenu"
        parent: more
        x: -width - Metrics.gap6
        y: more.height - height
        entries: [
            {text: I18n.tr("Change source", I18n.rev), glyph: Lucide.searchCode, family: Lucide.font, enabled: !!actions.lyrics && actions.lyrics.can_change_source === true},
            {text: I18n.tr("Wrong lyric", I18n.rev), glyph: Lucide.flagTriangleRight, family: Lucide.font}
        ]
        onChosen: index => {
            if (!actions.found) return
            const revision = actions.lyrics.revision
            if (index === 0) Player.changeLyricsSource(revision)
            else if (index === 1) Player.rejectLyrics(revision)
        }
    }
}

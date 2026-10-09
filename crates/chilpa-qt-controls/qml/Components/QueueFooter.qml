import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The foot of the side queue, as QBZ draws it: icons to empty it, keep it
// as a playlist and set the timer that stops playback, lit while it runs,
// and at the right end the search, a magnifier that opens to the left over
// them. Escape or its clear button empties it and folds it back.
Item {
    id: foot

    signal clear()
    signal save()
    signal timer()
    // What is typed in the search, as it is typed.
    signal searched(string text)
    // What is left of the timer, or empty while it is off.
    property string sleepLeft
    property bool searching: false

    function openSearch() {
        searching = true
        field.focusInput()
    }
    function closeSearch() {
        field.text = ""
        searching = false
        field.release()
    }

    implicitHeight: Metrics.panelHeader

    Rectangle {
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }

    Row {
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        IconButton {
            objectName: "queueClear"
            size: Metrics.button30
            glyph: SourceGlyphs.trashList
            iconFamily: SourceGlyphs.font
            tip: I18n.tr("Clear queue", I18n.rev)
            onClicked: foot.clear()
        }
        IconButton {
            objectName: "queueSave"
            size: Metrics.button30
            glyph: SourceGlyphs.addToList
            iconFamily: SourceGlyphs.font
            tip: I18n.tr("Save as playlist", I18n.rev)
            onClicked: foot.save()
        }
        IconButton {
            objectName: "queueTimer"
            size: Metrics.button30
            glyph: SourceGlyphs.clock
            iconFamily: SourceGlyphs.font
            tone: foot.sleepLeft !== "" ? "accent" : "secondary"
            tip: foot.sleepLeft !== "" ? I18n.trArgs("Stops in {}", [foot.sleepLeft], I18n.rev) : I18n.tr("Sleep timer", I18n.rev)
            onClicked: foot.timer()
        }
    }

    IconButton {
        objectName: "queueSearchOpen"
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        size: Metrics.button30
        glyph: Icons.search
        tip: foot.searching ? "" : I18n.tr("Search queue", I18n.rev)
        enabled: !foot.searching
        opacity: foot.searching ? 0 : 1
        Behavior on opacity { NumberAnimation { duration: Motion.fast } }
        onClicked: foot.openSearch()
    }
    SearchField {
        id: field
        objectName: "queueSearch"
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        height: Metrics.button30
        width: foot.searching ? foot.width - 2 * Metrics.gap6 : 0 // derived: the foot's width inside its margins
        visible: width > 0
        clip: true
        placeholder: I18n.tr("Search queue", I18n.rev)
        Behavior on width { NumberAnimation { duration: Motion.panel; easing.type: Easing.InOutQuad } }
        onTextChanged: foot.searched(text)
        onEscaped: foot.closeSearch()
        onCleared: foot.closeSearch()
    }
}

import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// The Tracks table's controls emit edits; the screen owns persistence.
ScreenHead {
    id: tools
    property string typed: ""
    property string order: "default"
    property string group: "off"
    property var shownColumns: []
    property bool selectionMode: false
    signal searched(string text)
    signal ordered(string value)
    signal grouped(string value)
    signal columned(var columns)
    signal selectionToggled()
    function focusSearch() { find.focusInput() }
    function clearSearch() { find.text = ""; settle.stop() }
    onTypedChanged: if (find.text.trim() !== typed) { find.text = typed; settle.stop() }
    flexible: find
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: tools.searched(find.text.trim())
    }
    title: I18n.tr("Tracks", I18n.rev)


    SearchField {
        id: find
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, Math.min(Metrics.screenSearch, tools.flexibleRoom))
        placeholder: I18n.tr("Search", I18n.rev)
        text: tools.typed
        onTextChanged: if (text.trim() !== tools.typed) settle.restart()
        onEscaped: text = ""
    }
    SortButton {
        anchors.verticalCenter: parent.verticalCenter
        value: tools.order
        options: [
            { value: "default", text: I18n.tr("Default", I18n.rev), fixed: true },
            { value: "artist", text: I18n.tr("Artist", I18n.rev) },
            { value: "title", text: I18n.tr("Title", I18n.rev) },
            { value: "album", text: I18n.tr("Album", I18n.rev) },
            { value: "year", text: I18n.tr("Year", I18n.rev) },
            { value: "added", text: I18n.tr("Date added", I18n.rev), descending: true },
            { value: "duration", text: I18n.tr("Duration", I18n.rev) }
        ]
        onPicked: (value) => {
            tools.ordered(value)
        }
    }
    SortButton {
        objectName: "tracksGrouping"
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.stack2
        directed: false
        value: tools.group
        options: [
            { value: "off", text: I18n.tr("No grouping", I18n.rev) },
            { value: "album", text: I18n.tr("By album", I18n.rev) },
            { value: "artist", text: I18n.tr("By artist", I18n.rev) },
            { value: "name", text: I18n.tr("By name", I18n.rev) },
            { value: "year", text: I18n.tr("By year", I18n.rev) },
            { value: "decade", text: I18n.tr("By decade", I18n.rev) },
            { value: "genre", text: I18n.tr("By genre", I18n.rev) },
            { value: "label", text: I18n.tr("By label", I18n.rev) },
            { value: "composer", text: I18n.tr("By composer", I18n.rev) },
            { value: "plays", text: I18n.tr("By plays", I18n.rev) }
        ]
        onPicked: (value) => {
            tools.grouped(value)
        }
    }
    AlbumColumnsGear {
        anchors.verticalCenter: parent.verticalCenter
        shown: tools.shownColumns
        options: [
            {name: "index", text: "#"},
            {name: "art", text: I18n.tr("Artwork", I18n.rev)},
            {name: "source", text: I18n.tr("Source", I18n.rev)},
            {name: "plays", text: I18n.tr("Plays", I18n.rev)},
            {name: "time", text: I18n.tr("Time", I18n.rev)},
            {name: "rating", text: I18n.tr("Rating", I18n.rev)},
            {name: "quality", text: I18n.tr("Quality", I18n.rev)},
            {name: "format", text: I18n.tr("Format", I18n.rev)},
            {name: "bits", text: I18n.tr("Bit depth", I18n.rev)},
            {name: "rate", text: I18n.tr("Sample rate", I18n.rev)},
            {name: "favorite", text: I18n.tr("Favorite", I18n.rev)}
        ]
        onChosen: columns => {
            tools.columned(columns)
        }
    }
    IconButton {
        anchors.verticalCenter: parent.verticalCenter
        tip: I18n.tr("Select tracks", I18n.rev)
        glyph: Icons.checkbox
        round: true
        checked: tools.selectionMode
        onClicked: {
            tools.selectionToggled()
        }
    }
}

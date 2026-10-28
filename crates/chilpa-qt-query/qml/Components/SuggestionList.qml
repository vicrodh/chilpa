import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// What "Add a condition…" finds as it is typed (F3): matches in every field,
// grouped, at most three each, with what each would leave, then the typed
// words anywhere, in a title or in a record's. Enter includes the first
// match, Shift+Enter keeps it out. Asked for once the typing settles: one
// round of counts per pause, not per key. The counts come back on their
// own, from a thread of the facade's, for the query and text they were
// asked for; what is shown stays until an answer for what is typed now
// arrives, and Enter waits for that answer rather than take an older one.
Item {
    id: list
    property string query: Library.queryText
    property string typed: ""
    property var found: ({ groups: [], words: {} })
    // The query and text `found` answers, or none.
    property var shownFor: null
    // The bridge's handle for this list: its rounds and their answers carry it.
    property int consumer: 0
    // An Enter (true for Shift+Enter) waiting for the answer to what is typed now.
    property var waiting: null
    signal edit(var change)

    implicitHeight: Math.min(Refine.suggestMax, content.implicitHeight + Refine.suggestPad * 2) // derived: padding above and below

    function current() { return list.typed.trim() }
    function shownIsCurrent() { return list.shownFor !== null && list.shownFor[0] === list.query && list.shownFor[1] === current() }
    function ask() { settle.stop(); QueryChips.suggest(list.consumer, list.query, current()) }
    Timer {
        id: settle
        interval: Motion.base
        onTriggered: list.ask()
    }
    onTypedChanged: { list.waiting = null; settle.restart() }
    // A condition changed under the list: its counts are for another query.
    onQueryChanged: { list.waiting = null; list.found = { groups: [], words: {} }; list.shownFor = null; settle.restart() }
    Component.onCompleted: { list.consumer = QueryChips.openSuggestions(); settle.restart() }
    Component.onDestruction: QueryChips.closeSuggestions(list.consumer)
    Connections {
        target: QueryChips
        function onSuggested(consumer, query, typed, found) {
            if (consumer !== list.consumer || query !== list.query || typed !== list.current()) return
            list.found = JSON.parse(found || "{\"groups\":[],\"words\":{}}")
            list.shownFor = [query, typed]
            if (list.waiting !== null) { const exclude = list.waiting; list.waiting = null; list.apply(exclude) }
        }
    }

    // The first match, included or kept out (Enter, Shift+Enter): of the
    // answer for what is typed now, or of the one asked for it at once.
    function first(exclude) {
        if (shownIsCurrent()) { apply(exclude); return }
        waiting = exclude
        ask()
    }
    function apply(exclude) {
        const group = found.groups.find(g => g.items.length > 0)
        if (group) edit({ op: "value", field: group.field, value: group.items[0].value, state: exclude ? "exclude" : "include" })
        else if (!exclude && current() !== "") edit({ op: "words", field: "words", word: current() })
    }

    ElevatedShadow { target: box; radius: Refine.pickRound }
    Rectangle {
        id: box
        anchors.fill: parent
        radius: Refine.pickRound
        color: Theme.color.surfaceRaised
        border.width: Metrics.hairline
        border.color: Theme.color.strokeHairline
        Flickable {
            anchors.fill: parent
            anchors.margins: Refine.suggestPad
            contentHeight: content.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Controls.ScrollBar.vertical: ViewportScrollBar {}
            Column {
                id: content
                width: parent.width
                component Head: Row {
                    property string glyph
                    property string text
                    height: Refine.suggestHead
                    leftPadding: Refine.pickPad
                    topPadding: Refine.innerApart
                    spacing: Refine.innerApart
                    Icon { anchors.verticalCenter: parent.verticalCenter; glyph: parent.glyph; size: Refine.chipIcon; color: Theme.color.textTertiary }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: parent.text; font.pointSize: Type.label; font.weight: Type.semibold; color: Theme.color.textTertiary }
                }
                Repeater {
                    model: list.found.groups
                    Column {
                        required property var modelData
                        width: content.width
                        Head { glyph: RefineText.glyph(modelData.field); text: RefineText.field(modelData.field) }
                        Repeater {
                            model: modelData.items
                            PickRow {
                                required property var modelData
                                objectName: "suggestion"
                                width: content.width
                                text: modelData.value
                                count: String(modelData.count)
                                mark: modelData.state
                                onIncluded: list.edit({ op: "value", field: parent.modelData.field, value: modelData.value, state: modelData.state === "include" ? "none" : "include" })
                                onExcluded: list.edit({ op: "value", field: parent.modelData.field, value: modelData.value, state: modelData.state === "exclude" ? "none" : "exclude" })
                            }
                        }
                    }
                }
                Head { glyph: Icons.letterCase; text: I18n.tr("Words", I18n.rev) }
                PickRow {
                    objectName: "suggestionWords"
                    width: content.width
                    glyph: Icons.letterCase
                    text: "“" + list.typed.trim() + "”"
                    hint: I18n.tr("in title, artist or record", I18n.rev)
                    count: String(list.found.words.anywhere || 0)
                    canExclude: false
                    onIncluded: list.edit({ op: "words", field: "words", word: list.typed.trim() })
                }
                PickRow {
                    width: content.width
                    glyph: Icons.music
                    text: I18n.tr("Title contains", I18n.rev)
                    hint: "“" + list.typed.trim() + "”"
                    count: String(list.found.words.title || 0)
                    canExclude: false
                    onIncluded: list.edit({ op: "words", field: "title", word: list.typed.trim() })
                }
                PickRow {
                    width: content.width
                    glyph: Icons.disc
                    text: I18n.tr("Album contains", I18n.rev)
                    hint: "“" + list.typed.trim() + "”"
                    count: String(list.found.words.album || 0)
                    canExclude: false
                    onIncluded: list.edit({ op: "words", field: "album", word: list.typed.trim() })
                }
                Text {
                    leftPadding: Refine.pickPad
                    topPadding: Refine.innerApart
                    bottomPadding: Refine.segTrackPad
                    text: I18n.tr("Enter includes the first match · Shift+Enter excludes it", I18n.rev)
                    font.pointSize: Type.label
                    color: Theme.color.textTertiary
                }
            }
        }
    }
}

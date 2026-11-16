import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Where the records go when the query, or a column's header, leaves none
// (F6): it says so, and offers to take one condition away, with how many
// tracks would be left for the query's, the most first. Asked of the facade
// only while there are none.
Column {
    id: zero
    property string query: Library.queryText
    // No records, and nothing loading.
    property bool active: false
    // What the columns' headers narrow; the facade's offers know only the query.
    readonly property var searches: RefineText.columnSearches(Library.queries)
    readonly property bool shown: active && (query !== "" || searches.length > 0)
    readonly property bool asking: shown && query !== ""
    // Read again when ratings are turned on or off: a rating condition then
    // counts, or does not.
    readonly property var reading: { Prefs.ratings; return asking ? JSON.parse(QueryChips.refine(query) || "{\"rows\":[]}") : ({ rows: [] }) }
    readonly property var offers: { Prefs.ratings; return asking ? JSON.parse(QueryChips.withoutEach(query) || "[]") : [] }

    // One condition to take away: what it is, what is left without it.
    component Offer: Rectangle {
        id: offer
        property string what
        property string remaining
        signal chosen()
        width: line.implicitWidth + Refine.zeroPad * 2 // derived: padding on both sides
        height: Refine.zeroRow
        radius: Refine.zeroRound
        color: area.containsMouse ? Theme.color.stateSelected : Theme.color.fillField
        Row {
            id: line
            x: Refine.zeroPad
            anchors.verticalCenter: parent.verticalCenter
            spacing: Refine.pickApart
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.x; size: Refine.fieldChipIcon; color: Theme.color.textSecondary }
            Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trArgs("Without {}", [offer.what], I18n.rev); font.pointSize: Type.row; color: Theme.color.textPrimary }
            Text { visible: offer.remaining !== ""; anchors.verticalCenter: parent.verticalCenter; text: "→ " + offer.remaining; font.pointSize: Type.row; font.features: { "tnum": 1 }; color: Theme.color.textSecondary }
        }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: offer.chosen() }
    }

    visible: shown
    topPadding: Refine.zeroTop
    spacing: Refine.pickApart
    Text { text: I18n.tr("No tracks match all of these", I18n.rev); font.pointSize: Refine.zeroTitle; font.weight: Type.heading; color: Theme.color.textPrimary }
    Text { visible: zero.offers.length + zero.searches.length > 0; text: I18n.tr("Remove one condition to get results:", I18n.rev); font.pointSize: Type.row; color: Theme.color.textSecondary }
    Repeater {
        model: zero.shown ? zero.searches : []
        Offer {
            required property var modelData
            objectName: "zeroSearchOffer"
            what: RefineText.field(modelData.column) + " " + I18n.tr("contains", I18n.rev) + " “" + modelData.text + "”"
            onChosen: Library.searchColumn(modelData.column, "")
        }
    }
    Repeater {
        model: zero.offers
        Offer {
            required property var modelData
            readonly property var row: zero.reading.rows.find(r => r.id === modelData.id) || ({ field: "", values: [] })
            objectName: "zeroOffer"
            what: RefineText.key(row) + ": " + RefineText.summary(row)
            remaining: I18n.trCount("{} track", "{} tracks", modelData.count, I18n.rev)
            onChosen: Library.applyQuery(QueryChips.refineEdit(zero.query, JSON.stringify({ op: "remove", id: modelData.id })))
        }
    }
}

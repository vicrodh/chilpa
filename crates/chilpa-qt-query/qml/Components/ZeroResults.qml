import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Where the records go when the query leaves none (F6): it says so, and
// offers to take one condition away with how many tracks would be left,
// the most first. Asked of the facade only while there are none.
Column {
    id: zero
    property string query: Library.queryText
    property bool active: false
    readonly property var reading: active ? JSON.parse(QueryChips.refine(query) || "{\"rows\":[]}") : ({ rows: [] })
    readonly property var offers: active ? JSON.parse(QueryChips.withoutEach(query) || "[]") : []

    visible: active
    topPadding: Refine.zeroTop
    spacing: Refine.pickApart
    Text { text: I18n.tr("No tracks match all of these", I18n.rev); font.pointSize: Refine.zeroTitle; font.weight: Type.heading; color: Theme.color.textPrimary }
    Text { visible: zero.offers.length > 0; text: I18n.tr("Remove one condition to get results:", I18n.rev); font.pointSize: Type.row; color: Theme.color.textSecondary }
    Repeater {
        model: zero.offers
        Rectangle {
            required property var modelData
            readonly property var row: zero.reading.rows.find(r => r.id === modelData.id) || ({ field: "", values: [] })
            objectName: "zeroOffer"
            width: offer.implicitWidth + Refine.zeroPad * 2 // derived: padding on both sides
            height: Refine.zeroRow
            radius: Refine.zeroRound
            color: area.containsMouse ? Theme.color.stateSelected : Theme.color.fillField
            Row {
                id: offer
                x: Refine.zeroPad
                anchors.verticalCenter: parent.verticalCenter
                spacing: Refine.pickApart
                Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.x; size: Refine.fieldChipIcon; color: Theme.color.textSecondary }
                Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trArgs("Without {}", [RefineText.key(parent.parent.row) + ": " + RefineText.summary(parent.parent.row)], I18n.rev); font.pointSize: Type.row; color: Theme.color.textPrimary }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "→ " + I18n.trCount("{} track", "{} tracks", parent.parent.modelData.count, I18n.rev); font.pointSize: Type.row; font.features: { "tnum": 1 }; color: Theme.color.textSecondary }
            }
            MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Library.applyQuery(QueryChips.refineEdit(zero.query, JSON.stringify({ op: "remove", id: modelData.id }))) }
        }
    }
}

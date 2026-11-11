import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// A column of field names (F4): `least` wide at least, and as wide as the
// longest name needs to fit on one line, up to a share of the room; past
// that a name wraps between its words, so the column is never narrower than
// its longest word. The names are measured as the column sets them, so it
// follows the language, the face and the platform. The defaults are the
// condition rows' label column.
Item {
    id: column
    // The fields named, once each.
    property var fields: []
    property real rowsWide: 0
    // What the column holds besides a name: its glyph, gaps and padding.
    property real glyph: Refine.labelIcon + Refine.labelApart
    property real least: Refine.labelWide
    property real share: Refine.labelShare
    property int weight: Type.regular
    readonly property real wide: {
        let name = 0, word = 0
        for (const text of names.children) if (text.implicitWidth !== undefined) name = Math.max(name, text.implicitWidth)
        for (const text of words.children) if (text.implicitWidth !== undefined) word = Math.max(word, text.implicitWidth)
        const most = Math.max(rowsWide * share, glyph + word)
        return Math.ceil(Math.max(least, Math.min(glyph + name, most)))
    }
    visible: false

    Item {
        id: names
        Repeater {
            model: column.fields
            Text { required property string modelData; text: RefineText.field(modelData); font.pointSize: Type.row; font.weight: column.weight }
        }
    }
    Item {
        id: words
        Repeater {
            model: { I18n.rev; const out = []; for (const field of column.fields) out.push(...RefineText.field(field).split(" ")); return out }
            Text { required property string modelData; text: modelData; font.pointSize: Type.row; font.weight: column.weight }
        }
    }
}

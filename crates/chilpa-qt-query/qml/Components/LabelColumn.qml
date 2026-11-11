import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The rows' label column (F4): the design's width at least, and as wide as
// the longest name the rows show needs to fit on one line, up to a share of
// the rows; past that a name wraps between its words, so the column is never
// narrower than its longest word. The names are measured as the rows set
// them, so the column follows the language, the face and the platform.
Item {
    id: column
    // The fields the rows name, once each.
    property var fields: []
    property real rowsWide: 0
    readonly property real wide: {
        const glyph = Refine.labelIcon + Refine.labelApart
        let name = 0, word = 0
        for (const text of names.children) if (text.implicitWidth !== undefined) name = Math.max(name, text.implicitWidth)
        for (const text of words.children) if (text.implicitWidth !== undefined) word = Math.max(word, text.implicitWidth)
        const most = Math.max(rowsWide * Refine.labelShare, glyph + word)
        return Math.ceil(Math.max(Refine.labelWide, Math.min(glyph + name, most)))
    }
    visible: false

    Item {
        id: names
        Repeater {
            model: column.fields
            Text { required property string modelData; text: RefineText.field(modelData); font.pointSize: Type.row }
        }
    }
    Item {
        id: words
        Repeater {
            model: { I18n.rev; const out = []; for (const field of column.fields) out.push(...RefineText.field(field).split(" ")); return out }
            Text { required property string modelData; text: modelData; font.pointSize: Type.row }
        }
    }
}

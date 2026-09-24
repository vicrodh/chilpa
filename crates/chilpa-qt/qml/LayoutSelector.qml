// Choosing the window's layout.
//
// The bridge brings the list from the files that were compiled in, so a new
// layout appears here by itself.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

ComboBox {
    // The file name is the msgid. A layout with no translation is shown by
    // its file name, which is ugly but never leaves the list incomplete.
    model: Chilpa.layouts.map(n => Chilpa.translate(n, Chilpa.trRev))
    currentIndex: Math.max(0, Chilpa.layouts.indexOf(Chilpa.layout))
    // The change happens afterwards: choosing here destroys the layout
    // containing this very list, and that cannot be done mid signal.
    onActivated: {
        const name = Chilpa.layouts[index]
        Qt.callLater(() => Chilpa.chooseLayout(name))
    }
}

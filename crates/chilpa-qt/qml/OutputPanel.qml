// Choosing the output, searching and demanding bit-perfect.
//
// The three controls any layout needs to hand. What they choose does not
// stay here: it goes to the bridge, and everything else reads it from there,
// so this panel can go on top, in a sidebar or nowhere at all without
// breaking anything.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

RowLayout {
    id: root
    spacing: 10

    // So a layout can decide whether it wants the search box here or separately.
    property bool withSearch: true
    // The same with the layout selector.
    property bool withLayout: true

    Component.onCompleted: {
        Chilpa.refreshOutputs()
        // With no remembered output the first on the list plays, and the
        // bridge has to know that before the first play.
        if (Chilpa.outputIds.indexOf(Chilpa.device) < 0 && Chilpa.outputIds.length > 0)
            Chilpa.device = Chilpa.outputIds[0]
    }

    Label { text: "Salida"; color: "#411d1f" }

    ComboBox {
        id: outputs
        Layout.fillWidth: true
        model: Chilpa.outputs
        // What is chosen lives in the bridge; this only shows it. That way
        // the list stays put even if something else changes the output.
        currentIndex: {
            const i = Chilpa.outputIds.indexOf(Chilpa.device)
            return i >= 0 ? i : 0
        }
        onActivated: Chilpa.choose(
            index >= 0 && index < Chilpa.outputIds.length ? Chilpa.outputIds[index] : "",
            bitPerfect.checked)
    }

    SearchBox {
        Layout.preferredWidth: 200
        visible: root.withSearch
    }

    LayoutSelector {
        Layout.preferredWidth: 130
        visible: root.withLayout
    }

    CheckBox {
        id: bitPerfect
        text: "Bit-perfect"
        checked: Chilpa.wantBitPerfect
        onToggled: Chilpa.choose(Chilpa.device, checked)
    }
}

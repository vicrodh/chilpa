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
    // The layout and the language live in the settings panel now. A layout
    // that wants them back in the bar switches them on: the properties are
    // what keeps that a layout's decision and not this panel's.
    property bool withLayout: false
    property bool withLanguage: false

    Component.onCompleted: {
        Chilpa.refreshOutputs()
        // With no remembered output the first on the list plays, and the
        // bridge has to know that before the first play.
        if (Chilpa.outputIds.indexOf(Chilpa.device) < 0 && Chilpa.outputIds.length > 0)
            Chilpa.device = Chilpa.outputIds[0]
    }

    // The stack first, because it decides the outputs: which exist, what they
    // are called and whether bit-perfect is on the table at all. Without it
    // here the window showed one stack's outputs and there was no way from
    // this panel to reach any other, so a machine with a USB DAC on it
    // offered "wherever the system is sending sound" and nothing else. The
    // DAC was listed all along, under a stack nobody could get to.
    Label { text: Chilpa.translate("Through", Chilpa.trRev); color: "#411d1f" }

    StackSelect { Layout.preferredWidth: 150 }

    Label { text: Chilpa.translate("Output", Chilpa.trRev); color: "#411d1f" }

    OutputSelect {
        id: outputs
        Layout.fillWidth: true
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

    LanguageSelector {
        Layout.preferredWidth: 110
        visible: root.withLanguage
    }

    CheckBox {
        id: bitPerfect
        text: "Bit-perfect"
        checked: Chilpa.wantBitPerfect
        onToggled: Chilpa.choose(Chilpa.device, checked)
    }
}

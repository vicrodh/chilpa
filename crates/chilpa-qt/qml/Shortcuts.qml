import QtQuick
import com.blitzfc.chilpa

// The window's keyboard (design/04 1). A text field keeps the keys it
// types: Space in the search field is a space, not play.
Item {
    Shortcut { sequence: "Space"; onActivated: Player.toggle() }
    Shortcut { sequence: "Ctrl+,"; onActivated: Navigation.openSection("") }
    Shortcut { sequences: ["Alt+Left", "Ctrl+["]; onActivated: Navigation.back() }
    Shortcut { sequences: ["Alt+Right", "Ctrl+]"]; onActivated: Navigation.forward() }
    Shortcut { sequence: "Ctrl+T"; onActivated: Navigation.open("library", 0) }
    Shortcut { sequence: "Ctrl+W"; onActivated: Navigation.close(Navigation.active) }
    Shortcut { sequence: "Ctrl+Q"; onActivated: Qt.quit() }
}

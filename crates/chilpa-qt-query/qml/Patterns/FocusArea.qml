import QtQuick

// I retain only the visible surface; both modes share the screen's grip.
Loader {
    property bool focusMode: false
    property Component columns
    active: visible
    sourceComponent: focusMode ? focus : columns
    Component { id: focus; FocusPanel {} }
}

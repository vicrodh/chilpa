import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I use the wireframe's output menu, anchored to its invoking control.
Menu {
    id: picker
    property Item invokingControl
    property bool preview: false
    property var previewFooter: [{act:"inspect",text:I18n.tr("Output details…", I18n.rev)}]
    property var previewEntries: [
        { act: "local", icon: "local", text: "Desktop audio", detail: I18n.tr("This computer", I18n.rev), checked: true },
        { separator: true }, { text: "Chromecast", note: true },
        { act: "connect", id: "living", icon: "cast", text: "Living Room", detail: "Chromecast Audio" },
        { separator: true }, { text: "DLNA / UPnP", note: true },
        { act: "connect", id: "receiver", icon: "dlna", text: "Network receiver", detail: "UPnP renderer" }
    ]
    readonly property var castState: JSON.parse(Cast.snapshot || "{}")
    parent: Overlay.overlay
    width: Metrics.castMenuWidth
    heightLimit: parent ? parent.height - Metrics.gap10 * 2 : Infinity // derived: the overlay margins
    title: I18n.tr("Play on", I18n.rev)
    footerEntries: (preview ? previewFooter : castState.footer || []).map(entry => Object.assign({}, entry, {glyph:Icons.chevronRight}))
    onFooterChosen: index => {
        if (!preview && footerEntries[index]?.act === "inspect") {
            if (invokingControl) invokingControl.forceActiveFocus()
            AudioOutput.show("equipment")
        }
    }
    entries: (preview ? previewEntries : castState.entries || []).map(function(entry) {
        const icons = { local: Icons.deviceSpeaker, cast: SourceGlyphs.castAudio, dlna: SourceGlyphs.dlna }
        return Object.assign({}, entry, { glyph: icons[entry.icon] || "", family: entry.icon === "cast" || entry.icon === "dlna" ? SourceGlyphs.font : "" })
    })

    function place() {
        if (!invokingControl) return
        const point = invokingControl.mapToItem(parent, invokingControl.width, 0)
        x = Math.max(Metrics.gap10, Math.min(point.x - width, parent.width - width - Metrics.gap10))
        // Over the button, or under it where there is no room over it.
        const over = point.y - implicitHeight - Metrics.gap8
        y = over >= Metrics.gap10 ? over : point.y + invokingControl.height + Metrics.gap8
    }
    function show(anchor) {
        invokingControl = anchor
        place()
        open()
    }
    onOpened: {
        if (!preview) Cast.picker(true)
        place()
    }
    onClosed: if (!preview) Cast.picker(false)
    onImplicitHeightChanged: if (opened) place()
    onChosen: (index) => {
        if (preview) return
        const entry = entries[index]
        if (entry.act === "local") Cast.disconnectDevice()
        else if (entry.act === "connect") Cast.connectDevice(entry.id)
    }
    Timer {
        interval: Motion.refresh
        repeat: true
        running: picker.opened && !picker.preview
        onTriggered: Cast.refresh()
    }
}

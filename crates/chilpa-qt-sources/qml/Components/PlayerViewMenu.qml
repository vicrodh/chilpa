import QtQuick
import app.chilpa.kit
import app.chilpa.controls
import app.chilpa.ambient
import app.chilpa.mini
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Shared

Shared.MenuCascade {
    id: viewMenu
    property bool onTop: false
    property real triggerWidth: Metrics.button28
    property real triggerHeight: Metrics.button28
    x: viewMenu.triggerWidth - width
    y: viewMenu.onTop ? viewMenu.triggerHeight + Metrics.gap10 : -height - Metrics.gap10
    readonly property var modes: ["floating", "bottom", "top"]
    readonly property var layouts: [I18n.tr("Floating", I18n.rev), I18n.tr("Full width, bottom", I18n.rev), I18n.tr("Full width, top", I18n.rev)]
    readonly property var backgrounds: [I18n.tr("Off", I18n.rev), I18n.tr("Ambient", I18n.rev), I18n.tr("Blurred art", I18n.rev),
        I18n.tr("Wallpaper", I18n.rev), I18n.tr("Wallpaper, blurred", I18n.rev)]
    function show() {
        const next = [{ text: I18n.tr("Mini player", I18n.rev), glyph: Icons.pictureInPicture, act: "mini" },
            { text: I18n.tr("Immersive", I18n.rev), glyph: Icons.arrowsMaximize, act: "immersive" }, { separator: true },
            { text: I18n.tr("Player", I18n.rev), glyph: Icons.layoutBottombarExpand, submenu: true, children: layouts.map((text, index) =>
                ({ text: text, checked: PlayerLayout.mode === modes[index], act: "layout:" + modes[index] })) },
            { text: I18n.tr("Background", I18n.rev), glyph: Icons.stack2, submenu: true,
            children: backgrounds.map((text, index) => ({ text: text, checked: Ambient.mode === index, act: "background:" + index })) }]
        for (const panel of DisplayMode.panels) next.push({text:panel.name,glyph:Icons.disc,submenu:true,children:DisplayMode.menuFor(panel.id)})
        entries = next
        open()
    }
    onActionChosen: (entry) => {
        if (entry.act === "display")
            DisplayMode.ask(entry.plugin, entry.command)
        else if (entry.act === "mini") {
            if (DisplayMode.active) DisplayMode.leave()
            Mini.enter()
        }
        else if (entry.act === "immersive") {
            if (DisplayMode.active) DisplayMode.leave()
            Immersive.openFromMenu()
        }
        else if (entry.act.startsWith("layout:"))
            PlayerLayout.mode = entry.act.split(":")[1]
        else if (entry.act.startsWith("background:"))
            Ambient.choose(Number(entry.act.split(":")[1]))
    }
}

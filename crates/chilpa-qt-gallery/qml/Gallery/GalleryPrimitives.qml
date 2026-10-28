import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TitledFlow {
    CircleCheck { text: "Selected"; checked: true }
    KeyedModel { items: [{id:"preview", title:"Stable descriptor"}] }
    ViewportScrollBar { visible: false }
    Item {
        width: Metrics.cardMinWidth
        height: Metrics.playerHeight
        ElevatedShadow { target: raised; radius: raised.radius }
        Rectangle { id: raised; anchors.fill: parent; radius: Metrics.radiusPlayer; color: Theme.color.surfacePlayer }
    }
    title: "Primitives"
    ExplicitBadge {}
    AdvisoryTitle { width: Metrics.cardMinWidth; text: "A record with an advisory"; explicitContent: true; font.pointSize: Type.body }

    Row {
        spacing: Metrics.gap8
        Icon { glyph: Icons.home; color: Theme.color.accent }
        Icon { glyph: Icons.heartFilled; filled: true; color: Theme.color.accent }
        Icon { glyph: Icons.waveSquare }
        BusyMark { clock: Pulse }
    }

    Row {
        spacing: Metrics.gap8
        IconButton { tip: "Resting"; glyph: Icons.list }
        FavoriteButton { clock: Pulse; tip: "Saving favorite"; glyph: Icons.heartFilled; busy: true }
        IconButton { tip: "Checked"; glyph: Icons.list; checked: true }
        IconButton { tip: "Accent"; glyph: Icons.arrowsShuffle; tone: "accent" }
        IconButton { tip: "Disabled"; glyph: Icons.chevronLeft; enabled: false }
        IconButton { tip: "Round"; glyph: Icons.x; round: true; size: Metrics.windowControl }
    }

    Row {
        spacing: Metrics.gap8
        PillButton { kind: "primary"; glyph: Icons.playerPlayFilled; filled: true; text: "Play" }
        PillButton { glyph: Icons.arrowsShuffle; text: "Shuffle" }
        PillButton { text: "Open popup"; onClicked: samplePopup.open() }
        PillButton { text: "Open modal"; onClicked: sampleModal.open() }
    }

    Controls.Popup {
        id: samplePopup
        modal: true
        focus: true
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnReleaseOutside
        PopupInput { surface: samplePopup }
        contentItem: PillButton { text: "Close popup"; onClicked: samplePopup.close() }
    }

    Modal {
        id: sampleModal
        width: Metrics.playlistFolderDialog
        Column {
            spacing: Metrics.gap12
            Text { text: "Modal"; font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
            Text { text: "Shared padding and surface"; color: Theme.color.textSecondary }
            PillButton { kind: "primary"; text: "Close"; onClicked: sampleModal.close() }
        }
    }

    SegmentedControl {
        options: [{ glyph: Icons.list, tip: "Compact" }, { glyph: Icons.layoutList, tip: "Rich" }]
        current: 1
    }

    SearchField { width: Metrics.sidebarWidth; placeholder: "Search"; shortcut: "Ctrl K" }

    Column {
        width: Metrics.confirmWidth
        ListRowSurface { width: parent.width; height: Metrics.rowCompact; rowIndex: 0 }
        ListRowSurface { width: parent.width; height: Metrics.rowCompact; rowIndex: 1 }
    }

    Row {
        spacing: Metrics.gap8
        Item { width: Metrics.cardMinWidth; height: Metrics.playerHeight; WindowGrips { anchors.fill: parent; window: Window.window } }
        ResizeHandle { height: Metrics.sidebarCollapsed }
        ResizeHandle { vertical: false; width: Metrics.sidebarCollapsed }
        Toggle { checked: true }
        Chip { text: "All"; active: true }
        Chip { text: "Songs" }
        TextField { placeholder: "Token"; secret: true }
        Rectangle { width: Metrics.selectWidth; height: Metrics.fieldHeight; radius: Metrics.radiusControl; color: Theme.color.fillField; FocusRing { visible: true } }
        Item { width: Metrics.selectWidth; height: Metrics.fieldHeight; FocusRelease {} }
        Swatches { colors: ["", Theme.color.accentHover, Theme.color.textSecondary]; values: ["a", "b", "c"]; value: "b" }
        LevelSlider { value: 60 }
        LevelField { value: 75 }
        FileField { action: "Choose image…"; value: "/pictures/wall.png"; clearTip: "Use the default" }
        SecretField { placeholder: "Token"; action: "Connect" }
        ProgressField { progress: "0.4"; action: "Analyze" }
        Toggle {}
        SelectField { options: ["ALSA", "PipeWire"]; values: ["alsa", "pipewire"]; marks: ["BP", ""]; value: "alsa" }
        Item {
            id: cutLine
            width: Metrics.selectWidth
            height: Metrics.fieldHeight
            Text { width: parent.width; text: "[1982] The Number of the Beast, 5644.2, Vinyl"; elide: Text.ElideMiddle; color: Theme.color.textPrimary }
            CutTextTips { watching: cutLine }
        }
        SourceIcon { sourceInfo: ({ local: true }) }
        Item { width: Metrics.button32; height: Metrics.button32; Hold {} }
        Repeater {
            model: ["opensubsonic", "navidrome", "jellyfin", "plex", "collection", "cassette", "cast-audio", "dlna", "musicbrainz", "discogs", "gandalf"]
            SourceIcon {
                required property string modelData
                sourceInfo: ({ icon: modelData })
                color: Theme.color.accent
            }
        }
        StatusField { state: "fine"; options: ["Disconnect", "Uninstall"]; values: ["disconnect", "uninstall"] }
    }

    Rectangle {
        width: Metrics.menuWidth
        height: entries.implicitHeight
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised
        Edge {}
        CreditedTitle { width: Metrics.searchFlyout; title: "Nocturne in E-flat major"; composer: "Frédéric Chopin" }
        CreditedTitle { width: Metrics.searchFlyout; title: "Duet"; credit: 'feat. <a href="guest">Guest artist</a>'; richCredits: true }
        Column {
            id: entries
            width: parent.width
            MenuEntry { width: parent.width; entry: ({ text: "Settings", hint: "Ctrl+," }) }
            MenuEntry { width: parent.width; entry: ({ separator: true }) }
            MenuEntry { width: parent.width; entry: ({ text: "USB DAC", detail: "hw:1,0", checked: true, mark: "BP" }) }
        }
        Menu { entries: [{ text: "A menu, closed" }] }
        MenuFooter { width: Metrics.menuWidth; entries: [{text:"Output details…",glyph:Icons.chevronRight}] }
    }

    Rectangle {
        width: Metrics.sidebarWidth
        height: Metrics.fieldHeight
        radius: Metrics.radiusPanel
        color: Theme.color.surfaceSidebar
        Edge {}
    }

    Rectangle {
        width: Metrics.sidebarWidth
        height: Metrics.fieldHeight
        color: Theme.color.accent
        PlaneCorners { width: parent.width }
    }

    Row {
        spacing: Metrics.gap8
        ArtTile { width: Metrics.heroArt / 2; height: width; tint: 0 } // derived: half a hero cover
        ArtTile { width: Metrics.artRow; height: width; tint: 3; radius: Metrics.radiusArtSmall }
    }

}

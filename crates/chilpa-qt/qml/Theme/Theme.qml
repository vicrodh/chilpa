pragma Singleton
import QtQuick
import com.blitzfc.chilpa

// The active theme's colors as typed properties (design/01-tokens.md).
// Each one asks Rust once; `rev` makes them all ask again when the theme,
// the system scheme or the accent changes, so the window repaints in place.
QtObject {
    id: theme

    readonly property int rev: ThemeModel.rev
    readonly property bool dark: ThemeModel.dark
    readonly property string searchLabel: I18n.tr("Search", I18n.rev)
    readonly property string emptySearchLabel: I18n.tr("No matches", I18n.rev)

    readonly property QtObject color: QtObject {
        readonly property color surfaceWindow: ThemeModel.color("surface.window", theme.rev)
        readonly property color surfaceSidebar: ThemeModel.color("surface.sidebar", theme.rev)
        readonly property color surfaceRaised: ThemeModel.color("surface.raised", theme.rev)
        readonly property color surfacePanel: ThemeModel.color("surface.panel", theme.rev)
        readonly property color surfacePane: ThemeModel.color("surface.pane", theme.rev)
        readonly property color surfacePlayer: ThemeModel.color("surface.player", theme.rev)
        readonly property color surfaceLcd: ThemeModel.color("surface.lcd", theme.rev)
        readonly property color textPrimary: ThemeModel.color("text.primary", theme.rev)
        readonly property color textSecondary: ThemeModel.color("text.secondary", theme.rev)
        readonly property color textTertiary: ThemeModel.color("text.tertiary", theme.rev)
        readonly property color textOnAccentMuted: ThemeModel.color("text.onAccentMuted", theme.rev)
        readonly property color textOnAccent: ThemeModel.color("text.onAccent", theme.rev)
        readonly property color strokeHairline: ThemeModel.color("stroke.hairline", theme.rev)
        readonly property color strokeFrame: ThemeModel.color("stroke.frame", theme.rev)
        readonly property color stateHover: ThemeModel.color("state.hover", theme.rev)
        readonly property color stateSelected: ThemeModel.color("state.selected", theme.rev)
        readonly property color stateStripe: ThemeModel.color("state.stripe", theme.rev)
        readonly property color fillField: ThemeModel.color("fill.field", theme.rev)
        readonly property color rowPlaying: ThemeModel.color("row.playing", theme.rev)
        readonly property color rowSelected: ThemeModel.color("row.selected", theme.rev)
        readonly property color accent: ThemeModel.color("accent", theme.rev)
        readonly property color casting: ThemeModel.color("casting", theme.rev)
        // The theme's own, under whatever accent was chosen.
        readonly property color accentBase: ThemeModel.color("accent.base", theme.rev)
        readonly property color accentHover: ThemeModel.color("accent.hover", theme.rev)
        readonly property color accentPressed: ThemeModel.color("accent.pressed", theme.rev)
        readonly property color statusPositive: ThemeModel.color("status.positive", theme.rev)
        readonly property color statusWarning: ThemeModel.color("status.warning", theme.rev)
        readonly property color statusNegative: ThemeModel.color("status.negative", theme.rev)
        readonly property color overlayScrim: ThemeModel.color("overlay.scrim", theme.rev)
        readonly property color overlayButton: ThemeModel.color("overlay.button", theme.rev)
        readonly property color overlayInk: ThemeModel.color("overlay.ink", theme.rev)
        readonly property color albumHeaderBase: ThemeModel.color("albumHeader.base", theme.rev)
        readonly property color albumHeaderDim: ThemeModel.color("albumHeader.dim", theme.rev)
        readonly property color albumHeaderTop: ThemeModel.color("albumHeader.top", theme.rev)
        readonly property color albumHeaderBottom: ThemeModel.color("albumHeader.bottom", theme.rev)

        // The one elevation, in two layers, and the soft one for panels.
        readonly property color shadowElevated: ThemeModel.shadowColor("elevated", 0, theme.rev)
        readonly property color shadowElevatedKey: ThemeModel.shadowColor("elevated", 1, theme.rev)
        readonly property color shadowSubtle: ThemeModel.shadowColor("subtle", 0, theme.rev)
        readonly property color shadowSegment: ThemeModel.shadowColor("segment", 0, theme.rev)
        readonly property color shadowTabActive: ThemeModel.shadowColor("tabActive", 0, theme.rev)
    }

    // The color a cover shows while it loads, varied by position so a grid
    // of placeholders does not read as one block.
    function placeholder(index) {
        return ThemeModel.placeholder(index, theme.rev)
    }

    // "system" follows the desktop: the scheme is handed to Rust, which
    // decides the theme.
    readonly property Binding followSystem: Binding {
        target: ThemeModel
        property: "systemDark"
        value: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
    }
}

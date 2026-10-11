pragma Singleton
import QtQuick
import app.chilpa
import app.chilpa.kit

// The active theme's colors as typed properties (design/01-tokens.md).
// Each one asks Rust once; `rev` makes them all ask again when the theme,
// the system scheme or the accent changes, so the window repaints in place.
QtObject {
    id: theme

    readonly property int rev: ThemeModel.rev
    readonly property bool dark: ThemeModel.dark
    // The dynamic background shows through the chrome: every surface that
    // goes translucent under it reads this one predicate.
    readonly property bool ambient: ThemeModel.ambient
    readonly property string searchLabel: I18n.tr("Search", I18n.rev)
    readonly property string emptySearchLabel: I18n.tr("No matches", I18n.rev)
    // What the sung line of lyrics can be tinted with, after the text's own
    // color and the accent: the design's four accents (design/05).
    readonly property var lyricColors: ["#A3161F", "#D8323A", "#3DAEE9", "#5C6166"]

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
        // The chip that counts what a segment holds, and the active segment's.
        readonly property color fillCount: ThemeModel.color("fill.count", theme.rev)
        readonly property color fillCountActive: ThemeModel.color("fill.countActive", theme.rev)
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
        // The veil under a cover shown whole (QBZ's lightbox, #bf000000).
        readonly property color lightboxScrim: ThemeModel.color("lightbox.scrim", theme.rev)
        // Listening stats: the band of the period's number, a colour for each
        // of artists, records and tracks, and the bars of the period before.
        readonly property color statsHero: ThemeModel.color("stats.hero", theme.rev)
        readonly property color statsArtists: ThemeModel.color("stats.artists", theme.rev)
        readonly property color statsAlbums: ThemeModel.color("stats.albums", theme.rev)
        readonly property color statsTracks: ThemeModel.color("stats.tracks", theme.rev)
        readonly property color statsPrevious: ThemeModel.color("stats.previous", theme.rev)
        readonly property color overlayButton: ThemeModel.color("overlay.button", theme.rev)
        readonly property color overlayInk: ThemeModel.color("overlay.ink", theme.rev)
        readonly property color albumHeaderBase: ThemeModel.color("albumHeader.base", theme.rev)
        readonly property color albumHeaderDim: ThemeModel.color("albumHeader.dim", theme.rev)
        readonly property color albumHeaderTop: ThemeModel.color("albumHeader.top", theme.rev)
        readonly property color albumHeaderBottom: ThemeModel.color("albumHeader.bottom", theme.rev)
        // The chrome under the dynamic background: the panels and the
        // player at half alpha, the content plane frosted with its hairline,
        // the raised fills at half alpha, and the veil over the field.
        readonly property color ambientSidebar: ThemeModel.color("ambient.sidebar", theme.rev)
        readonly property color ambientPlayer: ThemeModel.color("ambient.player", theme.rev)
        readonly property color ambientContent: ThemeModel.color("ambient.content", theme.rev)
        readonly property color ambientRaised: ThemeModel.color("ambient.raised", theme.rev)
        readonly property color ambientFrost: ThemeModel.color("ambient.frost", theme.rev)
        readonly property color ambientVeil: ThemeModel.color("ambient.veil", theme.rev)
        readonly property color ambientBar: ThemeModel.color("ambient.bar", theme.rev)
        readonly property color ambientPanel: ThemeModel.color("ambient.panel", theme.rev)
        readonly property color immersiveStage: ThemeModel.color("immersive.stage", theme.rev)
        readonly property color immersiveBlack: ThemeModel.color("immersive.black", theme.rev)
        readonly property color immersiveGlass: ThemeModel.color("immersive.glass", theme.rev)
        readonly property color immersiveGlassEdge: ThemeModel.color("immersive.glassEdge", theme.rev)
        readonly property color immersiveGlassHover: ThemeModel.color("immersive.glassHover", theme.rev)
        readonly property color immersiveMenu: ThemeModel.color("immersive.menu", theme.rev)
        readonly property color immersiveRowActive: ThemeModel.color("immersive.rowActive", theme.rev)
        readonly property color immersiveRowHover: ThemeModel.color("immersive.rowHover", theme.rev)
        readonly property color immersiveText: ThemeModel.color("immersive.text", theme.rev)
        readonly property color immersiveTextStrong: ThemeModel.color("immersive.textStrong", theme.rev)
        readonly property color immersiveTextBody: ThemeModel.color("immersive.textBody", theme.rev)
        readonly property color immersiveTextDim: ThemeModel.color("immersive.textDim", theme.rev)
        readonly property color immersiveTextMuted: ThemeModel.color("immersive.textMuted", theme.rev)
        readonly property color immersiveTextFaint: ThemeModel.color("immersive.textFaint", theme.rev)
        readonly property color immersiveTextGhost: ThemeModel.color("immersive.textGhost", theme.rev)
        readonly property color immersiveTextQuiet: ThemeModel.color("immersive.textQuiet", theme.rev)
        readonly property color immersiveLyricInactive: ThemeModel.color("immersive.lyricInactive", theme.rev)
        readonly property color immersiveFill: ThemeModel.color("immersive.fill", theme.rev)
        readonly property color immersiveNav: ThemeModel.color("immersive.nav", theme.rev)
        readonly property color immersiveSeekTrack: ThemeModel.color("immersive.seekTrack", theme.rev)
        readonly property color immersiveVolumeTrack: ThemeModel.color("immersive.volumeTrack", theme.rev)
        readonly property color immersiveVolumeTrackLocked: ThemeModel.color("immersive.volumeTrackLocked", theme.rev)
        readonly property color immersiveVolumeFill: ThemeModel.color("immersive.volumeFill", theme.rev)
        readonly property color immersiveVolumeFillLocked: ThemeModel.color("immersive.volumeFillLocked", theme.rev)
        readonly property color immersiveBubble: ThemeModel.color("immersive.bubble", theme.rev)
        readonly property color immersiveShadow: ThemeModel.color("immersive.shadow", theme.rev)
        readonly property color immersiveShadowDeep: ThemeModel.color("immersive.shadowDeep", theme.rev)
        readonly property color immersiveShadowArt: ThemeModel.color("immersive.shadowArt", theme.rev)
        readonly property color immersivePlaceholder: ThemeModel.color("immersive.placeholder", theme.rev)
        readonly property color immersiveScrim: ThemeModel.color("immersive.scrim", theme.rev)
        readonly property color immersiveScopeVeil: ThemeModel.color("immersive.scopeVeil", theme.rev)
        readonly property color immersiveTextShadow: ThemeModel.color("immersive.textShadow", theme.rev)
        readonly property color immersivePlayDisc: ThemeModel.color("immersive.playDisc", theme.rev)
        readonly property color immersivePlayDiscHover: ThemeModel.color("immersive.playDiscHover", theme.rev)
        readonly property color immersivePlayGlyph: ThemeModel.color("immersive.playGlyph", theme.rev)
        readonly property color immersiveCardGlass: ThemeModel.color("immersive.cardGlass", theme.rev)
        readonly property color immersiveCardVeil: ThemeModel.color("immersive.cardVeil", theme.rev)
        readonly property color immersiveCardGradientTop: ThemeModel.color("immersive.cardGradientTop", theme.rev)
        readonly property color immersiveCardGradientMid: ThemeModel.color("immersive.cardGradientMid", theme.rev)
        readonly property color immersiveCardGradientBottom: ThemeModel.color("immersive.cardGradientBottom", theme.rev)
        readonly property color immersiveCardEdge: ThemeModel.color("immersive.cardEdge", theme.rev)
        readonly property color immersiveQueueRow: ThemeModel.color("immersive.queueRow", theme.rev)
        readonly property color immersiveQueueRowEdge: ThemeModel.color("immersive.queueRowEdge", theme.rev)
        readonly property color immersiveQueueRowHover: ThemeModel.color("immersive.queueRowHover", theme.rev)
        readonly property color immersiveQueueCover: ThemeModel.color("immersive.queueCover", theme.rev)
        readonly property color immersiveQueueCoverEdge: ThemeModel.color("immersive.queueCoverEdge", theme.rev)
        readonly property color immersiveQueueChip: ThemeModel.color("immersive.queueChip", theme.rev)
        readonly property color immersiveQueueChipText: ThemeModel.color("immersive.queueChipText", theme.rev)
        readonly property color immersiveQueuePlay: ThemeModel.color("immersive.queuePlay", theme.rev)
        readonly property color immersiveQueuePlayHover: ThemeModel.color("immersive.queuePlayHover", theme.rev)
        readonly property color immersiveQueueLine: ThemeModel.color("immersive.queueLine", theme.rev)
        readonly property color immersiveClear: ThemeModel.color("immersive.clear", theme.rev)
        readonly property color immersiveClearHover: ThemeModel.color("immersive.clearHover", theme.rev)
        readonly property color immersiveClearEdge: ThemeModel.color("immersive.clearEdge", theme.rev)
        readonly property color immersiveClearEdgeHover: ThemeModel.color("immersive.clearEdgeHover", theme.rev)
        readonly property color immersiveSearchPanel: ThemeModel.color("immersive.searchPanel", theme.rev)
        readonly property color immersiveSearchEdge: ThemeModel.color("immersive.searchEdge", theme.rev)
        readonly property color immersiveAxis: ThemeModel.color("immersive.axis", theme.rev)
        readonly property color immersiveAxisBox: ThemeModel.color("immersive.axisBox", theme.rev)
        readonly property color immersiveRibbonBase: ThemeModel.color("immersive.ribbonBase", theme.rev)
        readonly property color immersiveGlowDefault: ThemeModel.color("immersive.glowDefault", theme.rev)
        readonly property color immersiveSeparator: ThemeModel.color("immersive.separator", theme.rev)
        // The mini player's card: its edges, fills and separators at the
        // alpha tiers QBZ draws them, white on a dark theme and black on a
        // light one, and the two that read over its backdrop.
        readonly property color miniEdge: ThemeModel.color("mini.edge", theme.rev)
        readonly property color miniCapsuleEdge: ThemeModel.color("mini.capsuleEdge", theme.rev)
        readonly property color miniSeparator: ThemeModel.color("mini.separator", theme.rev)
        readonly property color miniHover: ThemeModel.color("mini.hover", theme.rev)
        readonly property color miniCapsuleHover: ThemeModel.color("mini.capsuleHover", theme.rev)
        readonly property color miniCoverFill: ThemeModel.color("mini.coverFill", theme.rev)
        readonly property color miniRowActive: ThemeModel.color("mini.rowActive", theme.rev)
        readonly property color miniDanger: ThemeModel.color("mini.danger", theme.rev)
        readonly property color miniOnBackdrop: ThemeModel.color("mini.onBackdrop", theme.rev)
        readonly property color miniGlyphOnBackdrop: ThemeModel.color("mini.glyphOnBackdrop", theme.rev)
        readonly property color miniSeekOnBackdrop: ThemeModel.color("mini.seekOnBackdrop", theme.rev)
        readonly property color miniFooter: ThemeModel.color("mini.footer", theme.rev)

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

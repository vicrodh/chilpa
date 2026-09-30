pragma Singleton
import QtQuick

// Every measure the window uses, in logical pixels (design/01-tokens.md,
// 02-layout.md and 03-components.md). Nothing outside Theme/ writes a size by
// hand; the QML literal check fails if it does.
QtObject {
    // The window.
    readonly property int windowWidth: 1280
    readonly property int windowHeight: 800
    readonly property int windowMinWidth: 960
    readonly property int windowMinHeight: 600
    // The strip along the edges where dragging resizes a frameless window.
    readonly property int windowGrip: 6

    // Spacing.
    // The window's margin on its four sides: 6, which covers the rounded
    // corners of a macOS window.
    readonly property int spaceWindow: 6
    readonly property int contentX: 28
    readonly property int contentTop: 14
    readonly property int contentBottom: 120
    readonly property int gridGap: 20
    readonly property int section: 30
    readonly property int hitMin: 24
    readonly property int gap2: 2
    readonly property int gap4: 4
    readonly property int gap6: 6
    readonly property int gap8: 8
    readonly property int gap10: 10
    readonly property int gap12: 12
    readonly property int gap14: 14
    readonly property int gap16: 16
    readonly property int modalInset: 20
    readonly property int modalInsets: 40
    readonly property int gap22: 22

    // Radii.
    readonly property int radiusPanel: 12
    readonly property int radiusPlayer: 14
    readonly property int radiusMenu: 10
    readonly property int radiusLcd: 9
    readonly property int radiusCard: 8
    readonly property int radiusControl: 7
    readonly property int radiusItem: 6
    readonly property int radiusArtSmall: 5
    readonly property int radiusRow: 4

    // Icons: glyph sizes.
    readonly property int iconXs: 11
    readonly property int iconSm: 13
    readonly property int iconMd: 15
    readonly property int iconNav: 17
    readonly property int iconSkip: 19
    readonly property int iconPlay: 28

    // Controls.
    readonly property int button24: 24
    readonly property int button26: 26
    readonly property int button28: 28
    readonly property int button30: 30
    readonly property int button32: 32
    readonly property int button40: 40
    readonly property int chipHeight: 24
    readonly property int pillHeight: 30
    readonly property int pillPadding: 14
    readonly property int segmentWidth: 30
    readonly property int segmentHeight: 24
    readonly property int fieldHeight: 28
    readonly property int fieldPadding: 9
    readonly property int fieldHeightLarge: 32
    readonly property int selectWidth: 260
    readonly property int albumVersionMenuWidth: 560
    readonly property int castMenuWidth: 270
    readonly property int menuWidth: 236
    readonly property int menuMaxWidth: 310
    readonly property int menuIconColumn: 18
    readonly property int menuItem: 30
    readonly property int menuItemTall: 44
    readonly property int toggleWidth: 38
    readonly property int toggleHeight: 22
    readonly property int toggleKnob: 16
    readonly property int toggleInset: 3
    readonly property int focusRing: 2
    readonly property int tooltipDelay: 600
    readonly property int toastMinWidth: 280
    readonly property int toastMaxWidth: 560
    readonly property int toastBottom: 88
    readonly property int toastWithSelectionBottom: 140
    // Above every popup of the overlay.
    readonly property int tooltipLayer: 1000
    // How long a screenshot waits for covers and fonts to arrive.
    readonly property int shotDelay: 2500
    // The longest a scripted scroll test runs.
    readonly property int scrollTestSeconds: 15

    // Sidebar (02-layout.md 2).
    readonly property int sidebarWidth: 212
    readonly property int sidebarCollapsed: 56
    readonly property int sidebarSnapCollapsed: 150
    readonly property int sidebarSnapClosed: 28
    readonly property real sidebarMaxShare: 0.40
    readonly property int sidebarPadV: 12
    readonly property int sidebarPadH: 10
    readonly property int sidebarGap: 14
    readonly property int navItemHeight: 30
    readonly property int navItemPadding: 9
    readonly property int navSubIndent: 31
    readonly property int playlistRowHeight: 28
    readonly property int resizeHandle: 9
    readonly property int notchLong: 30
    readonly property int notchShort: 3
    readonly property real notchSoft: 0.35
    readonly property real notchStrong: 0.6

    // Tab bar (02-layout.md 3).
    readonly property int tabBarHeight: 42
    readonly property int tabBarTop: 6
    readonly property int tabHeight: 28
    // What a wheel reports for one notch.
    readonly property int wheelNotch: 120
    readonly property int tabMaxWidth: 180
    readonly property int tabPadLeft: 11
    readonly property int tabPadRight: 5
    readonly property int tabClose: 18
    readonly property int windowControl: 24
    // What a Mac's three controls take at the window's left, at the least.
    readonly property int windowControlsRoom: 78
    // How many times their placing is asked before giving up.
    readonly property int systemControlsTries: 25
    readonly property int dividerHeight: 18

    // Player bar (02-layout.md 7).
    readonly property int playerHeight: 70
    // The same margin as the sidebars'.
    readonly property int playerBottom: spaceWindow
    // How long the mouse is still before the bar steps away, when asked to.
    readonly property int playerIdle: 10000
    readonly property int playerMaxWidth: 980
    readonly property int playerInset: 16
    readonly property int playerPadLeft: 14
    readonly property int playerPadRight: 12
    // Tall enough for the seek bar to stay clear of who plays and the record.
    readonly property int lcdHeight: 54
    readonly property int lcdArt: 44
    readonly property int progress: 2
    readonly property int progressHover: 4
    readonly property int seekKnob: 10
    readonly property int seekHit: 10
    readonly property int volumeWidth: 64
    readonly property int volumeTrack: 3
    readonly property int volumePopupWidth: 52
    readonly property int volumeVerticalLength: 92
    readonly property int playerNoTimes: 480
    // What the wheel moves the volume by.
    readonly property int volumeStep: 5
    readonly property int playerNoVolume: 660
    readonly property int qualityTooltipWidth: 320
    readonly property int qualityTooltipLabel: 68
    readonly property int shadowElevatedY: 12
    readonly property int shadowElevatedBlur: 32
    readonly property int shadowKeyY: 1
    readonly property int shadowKeyBlur: 3
    readonly property int shadowSubtleY: 1
    readonly property int shadowSubtleBlur: 2

    // Track rows (03-components.md TrackRow).
    readonly property int rowCompact: 24
    readonly property int rowRich: 56
    readonly property int rowHeader: 24
    readonly property int rowGroup: 34
    readonly property int rowPadCompact: 10
    readonly property int rowPadRich: 8
    readonly property int colGrip: 18
    readonly property int colIndexCompact: 26
    readonly property int colIndexRich: 32
    readonly property int colArt: 52
    readonly property int artRow: 40
    readonly property int colQualityCompact: 96
    readonly property int colQualityRich: 118
    readonly property int colRating: 82
    // The favorite's heart and the button that opens the row's menu.
    // The heart and the menu button, a cell each.
    readonly property int rowAction: 28
    readonly property int colMore: rowAction * 2 // derived: two cells
    readonly property int colPlaysCompact: 60
    readonly property int colPlaysRich: 92
    readonly property int colFavorite: 32
    readonly property int colTimeCompact: 44
    readonly property int colTimeRich: 48
    readonly property int playingBarWidth: 2
    readonly property int playingBox: 12
    // Enough to build every row of a list that grows instead of scrolling.
    readonly property int fitContentBuffer: 100000

    // Album cards and grids (02-layout.md 4, 03-components.md AlbumCard).
    readonly property int cardMinWidth: 180
    readonly property int gridMinColumns: 3
    readonly property int cardTextGap: 8
    readonly property int cardTextHeight: 40
    readonly property int overlayPad: 8
    readonly property int overlayCheck: 26
    readonly property int overlayPlay: 40
    readonly property int overlayButton: 30
    readonly property int overlayIcon: 16
    readonly property int overlayCheckIcon: 21
    readonly property int cardRing: 2
    readonly property int carouselCard: 164
    readonly property int ratingStar: 14
    readonly property int ratingMaximum: 5
    // The quick view under a row (03-components.md AlbumQuickView).
    readonly property int quickArt: 184
    readonly property int quickPad: 22
    readonly property int quickPadBottom: 18
    readonly property int quickGap: 28
    readonly property int quickNotch: 12
    readonly property int selectionBarHeight: 44
    readonly property int selectionBottom: 88
    // Covers are decoded at sizes rounded up to this step, in device pixels.
    readonly property int artDecodeStep: 128

    // Headers of screens.
    readonly property int shelfHeader: 40
    // How many records a home shelf reads: more than one row can show at
    // any width, so a wider window never shows a gap.
    readonly property int shelfLimit: 12
    readonly property int mostPlayedLimit: 10
    readonly property int heroArt: 220
    readonly property int artworkModal: 680
    readonly property int artworkModalGutter: 64
    readonly property int pageHero: 168
    readonly property int playlistFilter: 26
    readonly property int playlistIndent: 22
    readonly property int heroGap: 28
    readonly property int barHeight: 44

    // Library (02-layout.md 9).
    readonly property int breadcrumb: 32
    readonly property int panesHeight: 212
    readonly property int panesMin: 102
    readonly property int panesMax: 420
    readonly property int paneHeader: 28
    readonly property int paneRow: 22
    readonly property int panesHandle: 12
    readonly property int toolbar: 28
    readonly property int crumbHeight: 22

    // The search flyout (design/02 8, design/03 SearchFlyout).
    readonly property int searchFlyout: 440
    // The player's options (the player bar's switches).
    readonly property int playbackOptionsWidth: 320
    readonly property int optionRow: 44
    readonly property real disabledOpacity: 0.45
    readonly property int searchTop: 14
    readonly property int searchHeader: 26
    readonly property int searchRow: 46
    readonly property int searchArt: 36
    readonly property int searchBest: 68
    readonly property int searchAvatar: 52
    // The filter field of a screen that searches itself (design/04 11).
    readonly property int screenSearch: 200
    readonly property int sourcesPopoverWidth: 340
    readonly property int sourceFilterRow: 44
    readonly property int playlistModalWidth: 420
    readonly property int playlistModalHeight: 520
    readonly property int playlistPickerRow: 58
    readonly property int playlistManagerSearch: 250
    readonly property int playlistManagerCardWidth: 176
    readonly property int playlistManagerCard: 210
    readonly property int playlistManagerListRow: 58
    readonly property int playlistManagerArt: 112
    readonly property int playlistManagerFolderIcon: 60
    readonly property int playlistManagerTextY: 134
    readonly property int playlistManagerBack: 110
    readonly property int playlistFolderDialog: 360
    readonly property int colorSwatch: 28
    readonly property int colorSwatchRadius: 14
    readonly property int playlistCollage: 20
    // Two, so the tiles are whole pixels.
    readonly property int playlistCollageSeam: 2
    readonly property int contentHorizontalMargins: 56
    readonly property int selectionHorizontalPadding: 32

    // The right panel (design/02 5).
    readonly property int rightPanel: 300
    readonly property int panelHeader: 40
    // A share kept as a whole number, in thousandths.
    readonly property int shareUnits: 1000
    readonly property int queueMin: 250
    readonly property int lyricsMin: 110
    readonly property int queueRow: 46
    readonly property int queueNowRow: 48
    readonly property int queueArt: 36
    readonly property int queueGrip: 12
    readonly property int queueMore: 20
    // "Now playing" and "Up next" over their rows.
    readonly property int queueSection: 26

    // Settings (design/03 SettingsRow, design/06 Settings).
    readonly property int settingsNav: 188
    readonly property int settingsRow: 54
    readonly property int settingsGroup: 44
    readonly property int settingsMaxWidth: 880
    readonly property int folderRow: 48
    readonly property int folderDateWidth: 130
    readonly property int folderStatusWidth: 70
    readonly property int folderEntry: 44
    readonly property int folderArt: 34
    readonly property int folderTreeWidth: 272
    readonly property int folderTreeMin: 136
    readonly property real folderTreeShare: 0.5
    readonly property int folderTreeRow: 26
    readonly property int folderTreeIndent: 16
    readonly property int folderTileMinWidth: 180
    readonly property int folderCardMin: 140
    readonly property int folderCardText: 42
    readonly property int folderListRow: 34
    readonly property int folderGlyph: 32

    // The strip of letters beside a grouped list.
    readonly property int alphaStrip: 18
    readonly property int alphaCell: 15
    readonly property int folderTileHeight: 72
    readonly property int folderAlbumRow: 54
    // How the four columns share the width: year, genre, artist, album.
    readonly property var paneShares: [0.7, 1.0, 1.2, 1.3]
    readonly property real paneShareTotal: 4.2
    // The three one-pixel separators between them and the edge on each side.
    readonly property int paneSeams: 5

    // The metadata editor: a spreadsheet of tracks and the record's column.
    readonly property int metadataCover: 96
    readonly property int metadataCandidate: 72
    readonly property int metadataRow: 32
    readonly property int metadataHit: 40
    readonly property int metadataDisc: 56
    readonly property int metadataTrack: 64
    readonly property int metadataTitleMin: 180
    readonly property int metadataCellMin: 150
    readonly property int metadataCellPad: 9
    readonly property int metadataDetailsBar: 38
    readonly property int metadataDetails: 218
    readonly property int metadataHitsShown: 5
    readonly property int metadataColumnMin: 340
    readonly property int confirmWidth: 440
}

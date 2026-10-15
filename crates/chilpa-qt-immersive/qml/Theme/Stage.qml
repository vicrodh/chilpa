pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Every measure of the immersive view, read off QBZ's immersive QML file
// by file (03-immersive.md keeps each number with its line). Font sizes
// are QBZ's pixels taken to points. Nothing here is a color: those are
// the theme's immersive.* tokens.
QtObject {
    readonly property int sceneMaxWide: 2560
    readonly property int sceneMaxTall: 1440
    readonly property int sceneFade: 240
    readonly property int scopeHeightLess: 196
    readonly property int scopeTracePad: 8
    readonly property real scopeLine: 2.2
    readonly property real scopeFill: 0.16
    readonly property int scopeCorrelationEdge: 12
    readonly property int scopeCorrelationWide: 132
    readonly property int scopeCorrelationTall: 18
    readonly property int scopeCorrelationTrack: 2
    readonly property int scopeCorrelationThumbWide: 4
    readonly property int scopeCorrelationThumbTall: 10
    readonly property real spectralLabel: 11 * Type.pointsPerPixel
    readonly property int spectralLabelWide: 40
    readonly property int spectralLabelLead: 46
    readonly property int spectralLabelApart: 6
    readonly property int spectralTimeStep: 30
    readonly property int spectralMaxLabels: 31
    readonly property real spectralTimeText: 10 * Type.pointsPerPixel
    readonly property int spectralHeaderPad: 28
    readonly property int spectralHeaderTall: 22
    readonly property int spectralHeaderRound: 5
    readonly property real spectralHeaderText: 12 * Type.pointsPerPixel
    // ---- the overlay and its chrome ----------------------------------
    readonly property int edge: 24
    readonly property int headerTop: 16
    readonly property int headerTall: 36
    readonly property int triggerWide: 122
    readonly property int triggerRound: 18
    readonly property int triggerArrow: 38
    readonly property int triggerMiddle: 46
    readonly property int triggerIcon: 16
    readonly property int menuWide: 660
    readonly property int menuRound: 16
    readonly property int menuPad: 12
    readonly property int menuColumnApart: 16
    readonly property int menuRowTall: 33
    readonly property int menuRowRound: 8
    readonly property int menuDot: 7
    readonly property int menuDotIn: 12
    readonly property real menuHeadText: 11 * Type.pointsPerPixel
    readonly property real menuHeadTracking: 1.5
    readonly property real menuRowText: 13 * Type.pointsPerPixel
    readonly property int menuRowPad: 12
    readonly property int searchWide: 420
    readonly property int searchTall: 36
    readonly property int searchRound: 18
    readonly property int searchPad: 14
    readonly property real searchText: 13 * Type.pointsPerPixel
    readonly property real hintText: 12 * Type.pointsPerPixel
    readonly property int capsuleIdle: 44
    readonly property int capsuleBase: 40
    readonly property int capsuleStep: 36
    readonly property int capsuleRound: 18
    readonly property int capsuleIcon: 16
    readonly property int capsuleSlide: 150
    readonly property int hideAfter: 6000
    readonly property int chromeFade: 300
    readonly property real hoverGuard: 0.5
    readonly property int focusDelay: 30

    // ---- blurs (the reach of MultiEffect) and hit slop ---------------------
    readonly property int blurReach: 64
    readonly property int blurReachArt: 32
    readonly property int blurReachCard: 36
    readonly property int blurReachPlay: 24
    readonly property int capsuleHitPad: 9
    readonly property real disabledOpacity: 0.4
    readonly property real idleGlyph: 0.65
    readonly property real atmosphereDim: 0.15
    readonly property real scopeArtOpacity: 0.78
    readonly property int glyphFade: 100
    readonly property int hoverFade: 150
    readonly property int chipRound: 4
    readonly property int chipPad: 8
    readonly property int chipTall: 22
    readonly property real chipText: 11 * Type.pointsPerPixel
    readonly property int menuHeadTall: 26
    readonly property int menuApart: 8
    readonly property int menuRowApart: 1
    readonly property int menuRowLead: 10
    readonly property int menuDotPad: 8
    readonly property int exitMenuWide: 200
    readonly property int hintApart: 10
    readonly property int searchFieldApart: 6

    // ---- the player bar ------------------------------------------------
    readonly property int barWide: 600
    readonly property int barLess: 32
    readonly property int barTall: 90
    readonly property int barRound: 16
    readonly property int barShadow: 32
    readonly property int barShadowDrop: 8
    readonly property int ghostSide: 28
    readonly property int ghostFirst: 24
    readonly property int ghostStep: 32
    readonly property int ghostTop: 22
    readonly property int ghostIcon: 14
    readonly property int navSide: 28
    readonly property int navBackAt: 64
    readonly property int navNextAt: 36
    readonly property int playDisc: 48
    readonly property int playGlyph: 20
    readonly property int playShadow: 12
    readonly property int playShadowDrop: 4
    readonly property real timeText: 11 * Type.pointsPerPixel
    readonly property int timeLeftAt: 108
    readonly property int timeWide: 42
    readonly property int timeRightAt: 76
    readonly property int volumeGhostFromRight: 156
    readonly property int volumeBarWide: 64
    readonly property int volumeBarFromRight: 112
    readonly property int volumeBarTop: 31
    readonly property int volumeTrack: 2
    readonly property int volumeThumb: 12
    readonly property int volumeThumbFade: 150
    readonly property int bubbleWide: 34
    readonly property int bubbleTall: 20
    readonly property int bubbleRound: 6
    readonly property int bubbleUp: 26
    readonly property real bubbleText: 11 * Type.pointsPerPixel
    readonly property int moreFromRight: 42
    readonly property int tinyTop: 71
    readonly property int tinyLess: 16
    readonly property int tinyTrack: 2
    readonly property int tinyRound: 1
    readonly property int tinyGrab: 14

    // ---- the song card and the metadata ---------------------------------
    readonly property int cardEdge: 24
    readonly property real cardTitleText: 15 * Type.pointsPerPixel
    readonly property int cardTitleCap: 420
    readonly property real cardSubText: 12 * Type.pointsPerPixel
    readonly property int cardCover: 72
    readonly property int cardCoverRound: 6
    readonly property int cardShadow: 16
    readonly property int cardShadowDrop: 4
    readonly property int cardApart: 16
    readonly property int metaApart: 6
    readonly property real metaTitleText: 28 * Type.pointsPerPixel
    readonly property real metaArtistText: 18 * Type.pointsPerPixel
    readonly property real metaAlbumText: 14 * Type.pointsPerPixel
    readonly property int metaBadgeTop: 12
    readonly property int metaNowBelow: 8
    readonly property real nowText: 12 * Type.pointsPerPixel
    readonly property real nowTracking: 0.5

    // ---- Album Reactive and Static --------------------------------------
    readonly property int reactivePadTop: 52
    readonly property int reactivePadBottom: 132
    readonly property int reactiveApart: 40
    readonly property int reactiveArtMin: 180
    readonly property int reactiveArtMax: 660
    readonly property real reactiveArtShare: 0.62
    readonly property real reactiveArtDivisor: 1.25
    readonly property int reactiveHeightLess: 384
    readonly property real reactiveScaleGain: 0.25
    readonly property int glowBase: 15
    readonly property int glowGain: 200
    readonly property real glowOpacityBase: 0.1
    readonly property real glowOpacityGain: 1.5
    readonly property int glowBlurDivisor: 64
    readonly property int artShadow: 32
    readonly property int artShadowDrop: 8
    readonly property int artRound: 8
    readonly property int staticHeightLess: 364
    readonly property int staticArtMax: 640
    readonly property int staticApart: 20
    readonly property real settleEase: 0.45

    // ---- Spectrum and Wave Bed -------------------------------------------
    readonly property int spectrumColumns: 28
    readonly property int spectrumBarLess: 4
    readonly property real spectrumBaseline: 0.15
    readonly property real spectrumMaxShare: 0.7
    readonly property int spectrumArtMin: 160
    readonly property int spectrumArtMax: 360
    readonly property real spectrumArtShareTall: 0.45
    readonly property real spectrumArtShareWide: 0.5
    readonly property int spectrumTop: 70
    readonly property int spectrumApart: 20
    readonly property int spectrumShadow: 36
    readonly property int spectrumShadowDrop: 8
    readonly property int waveColumns: 48
    readonly property int waveStep: 5
    readonly property real waveGain: 0.7
    readonly property int waveBarLess: 4
    readonly property int waveMinTall: 2
    readonly property int waveRound: 1

    // ---- Coverflow -------------------------------------------------------
    readonly property int flowArtMin: 160
    readonly property int flowArtMax: 560
    readonly property int flowHeightLess: 372
    readonly property real flowShare: 0.52
    readonly property real flowPitch: 0.42
    readonly property real flowScaleFall: 0.18
    readonly property real flowScaleFloor: 0.5
    readonly property real flowDimCap: 0.55
    readonly property real flowDimRate: 0.18
    readonly property real flowPaintReach: 3.2
    readonly property real flowLiveReach: 6.5
    readonly property int flowSlide: 320
    readonly property int flowLift: 12
    readonly property int flowRound: 8
    readonly property int flowShadow: 28
    readonly property int flowShadowDrop: 8
    readonly property int flowStageExtra: 56
    readonly property int flowPadTop: 52
    readonly property int flowPadBottom: 132
    readonly property int flowApart: 28

    // ---- Lyrics in focus and Queue ----------------------------------------
    readonly property int lineMin: 30
    readonly property int lineMax: 48
    readonly property real lineShare: 0.026
    readonly property int lineRise: 24
    readonly property int lineEnter: 500
    readonly property int lyricsPadTop: 80
    readonly property int lyricsPadSide: 60
    readonly property int lyricsPadBottom: 140
    readonly property real lyricsWideShare: 0.9
    readonly property real noteText: 48 * Type.pointsPerPixel
    readonly property real noteOpacity: 0.3
    readonly property real noLyricsText: 20 * Type.pointsPerPixel
    readonly property int queueWide: 720
    readonly property int queueSideLess: 96
    readonly property int queuePadTop: 56
    readonly property int queuePadBottom: 132
    readonly property int queueFocusLess: 8
    readonly property int queueApart: 16
    readonly property real queueTabText: 18 * Type.pointsPerPixel
    readonly property int queueTabApart: 24
    readonly property int queueTabTall: 34
    readonly property int queueUnderline: 2
    readonly property int queueClearTall: 32
    readonly property int queueClearRound: 8
    readonly property int queueClearIcon: 14
    readonly property int queueClearPad: 12
    readonly property real queueClearText: 15 * Type.pointsPerPixel
    readonly property real queueHeadText: 13 * Type.pointsPerPixel
    readonly property real queueHeadTracking: 1.2
    readonly property int queueRowTall: 56
    readonly property int queueRowRound: 10
    readonly property int queueRowPad: 8
    readonly property int queueCover: 40
    readonly property int queueCoverRound: 5
    readonly property real queueTitleText: 17 * Type.pointsPerPixel
    readonly property real queueArtistText: 15 * Type.pointsPerPixel
    readonly property real queueTimeText: 14 * Type.pointsPerPixel
    readonly property int queuePlay: 30
    readonly property real queueEmptyText: 19 * Type.pointsPerPixel
    readonly property real queueEmptySubText: 15 * Type.pointsPerPixel
    readonly property int queueHistoryApart: 4
    readonly property int queueCache: 200

    // ---- Reactive Rings and the scopes -------------------------------------
    readonly property int ringsPool: 12
    readonly property int ringsCooldown: 2
    readonly property real ringsLabelText: 10 * Type.pointsPerPixel
    readonly property real ringsLabelAt: 0.41
    readonly property int ringsLabelMinTall: 560
    readonly property int ringsLabelWide: 46
    readonly property real ringsLabelTracking: 0.8
    readonly property int ringsHalo: 7
    readonly property int ringsStroke: 2
    readonly property real ringsStrokeGain: 4.5
    readonly property int rampSteps: 8
    readonly property int ringsHaloGain: 9
    readonly property int ringsCore: 2
    readonly property int ringsCoreGain: 4
    readonly property int cineBackdropDecode: 1024
    readonly property int scopeInsetLess: 96
    readonly property int scopeBottomLess: 196
    readonly property int scopeBottom: 132
    readonly property int scopeSide: 48
    readonly property int scopeTop: 48
    readonly property int scopeRightTall: 96
    readonly property int meterWide: 132
    readonly property int meterTall: 18
    readonly property real traceStroke: 2.2
    readonly property real traceFill: 0.16
    readonly property int trailDepthDots: 5
    readonly property int trailDepthLines: 4

    // ---- Split and the cinematic card ---------------------------------------
    readonly property int splitPadTop: 56
    readonly property int splitPadBottom: 132
    readonly property int splitPadSide: 56
    readonly property int splitApart: 48
    readonly property int splitColumnsLess: 160
    readonly property int splitArtMin: 240
    readonly property int splitArtLess: 200
    readonly property int splitArtRound: 12
    readonly property int splitShadow: 40
    readonly property int splitShadowDrop: 12
    readonly property int splitMetaLess: 44
    readonly property int splitMetaApart: 28
    readonly property int splitPlatePad: 20
    readonly property int splitLyricSide: 8
    readonly property int splitGapMin: 16
    readonly property int splitGapMax: 26
    readonly property real splitGapShare: 0.014
    readonly property int splitLineMin: 24
    readonly property int splitLineMax: 34
    readonly property real splitLineShare: 0.018
    readonly property int splitActiveMin: 28
    readonly property int splitActiveMax: 42
    readonly property real splitActiveShare: 0.022
    readonly property int cineTopMin: 56
    readonly property int cineTopMax: 72
    readonly property real cineTopShare: 0.09
    readonly property int cinePlayerAbove: 160
    readonly property int cinePlayerLess: 114
    readonly property real cineCardShare: 0.85
    readonly property real cineCardTallShare: 0.70
    readonly property real cineCardRatio: 0.56
    readonly property int cineCardMinTall: 120
    readonly property int cineCardLess: 32
    readonly property real cineCardPlace: 0.55
    readonly property int cineCardRound: 16
    readonly property int cineBlurMin: 36
    readonly property real cineBlurShare: 0.12
    readonly property real cineBlur: 0.58
    readonly property real cineArtTallShare: 0.94
    readonly property real cineArtWideShare: 0.42
    readonly property int cineArtShadow: 32
    readonly property int cineArtShadowDrop: 8
    readonly property real cineArtBlur: 0.55
    readonly property int cineTextGapMin: 20
    readonly property real cineTextGapShare: 0.032
    readonly property real cineNowScale: 1.5
    readonly property real cineTitleShare: 0.105
    readonly property int cineTitleMin: 24
    readonly property int cineTitleMax: 48
    readonly property real cineSubShare: 0.052
    readonly property int cineSubMin: 13
    readonly property int cineSubMax: 21
    readonly property real cineTimeShare: 0.035
    readonly property int cineTimeMin: 10
    readonly property int cineTimeMax: 13
    readonly property int cineLineMin: 22
    readonly property int cineLineMax: 44
    readonly property int cineWaveColumns: 48
    readonly property real cineWaveBarShare: 0.76
    readonly property int cineWaveBarMin: 3
    readonly property real cineWaveEase: 0.62
    readonly property real cineWaveWide: 0.98
    readonly property real cineWaveTallShare: 0.80
    readonly property int cineWaveAbove: 8
    readonly property real cineBaselineAlpha: 0.50

    // ---- the search ---------------------------------------------------------
    readonly property int searchPanelWide: 380
    readonly property int searchPanelTop: 58
    readonly property int searchPanelRound: 16
    readonly property int searchBottomLess: 138
    readonly property int searchSkeletonRows: 4
    readonly property int searchPulse: 700
    readonly property int searchHeadTall: 24
    readonly property real searchHeadText: 11 * Type.pointsPerPixel
    readonly property int searchRowTall: 56
    readonly property int searchRowRound: 8
    readonly property int searchSectionPad: 4
    readonly property int searchActiveBar: 3
    readonly property int searchThumb: 40
    readonly property real searchTitleText: 14 * Type.pointsPerPixel
    readonly property real searchSubText: 12 * Type.pointsPerPixel
    readonly property int searchDebounce: 220
    readonly property int searchMinChars: 2
    readonly property int searchPanelShadow: 32
    readonly property int searchPanelShadowDrop: 8
}

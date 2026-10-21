pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The Refine panel's measures, as the design handoff gives them
// (docs/ui/handoff_refine, source/LibraryRefine.dc.html), in logical pixels.
// Colors come from the app's theme; type sizes from Type.
QtObject {
    // The panel and its parts.
    readonly property int panelRound: 12
    readonly property int headMinTall: 40
    readonly property int headTop: 6
    readonly property int headRight: 8
    readonly property int headBottom: 4
    readonly property int headLeft: 14
    readonly property int headApart: 8
    readonly property int rowsSide: 8
    readonly property int rowsBottom: 2
    readonly property int rowsApart: 1
    readonly property int footTop: 6
    readonly property int footSide: 12
    readonly property int footBottom: 10
    readonly property int footApart: 8

    // A condition row.
    readonly property int rowMinTall: 34
    readonly property int rowRound: 8
    readonly property int rowPadTop: 5
    readonly property int rowPadRight: 4
    readonly property int rowPadLeft: 6
    readonly property int rowApartX: 8
    readonly property int rowApartY: 4
    readonly property int labelWide: 104
    readonly property int labelWideLong: 124
    readonly property int labelIcon: 15
    readonly property int labelApart: 7
    readonly property int innerApart: 6

    // The operator pill and its open segments.
    readonly property int opTall: 24
    readonly property int opRound: 6
    readonly property int opPadLeft: 8
    readonly property int opPadRight: 6
    readonly property int opApart: 3
    readonly property int opChevron: 11
    readonly property int opMinus: 12
    readonly property int segTrackPad: 2
    readonly property int segTrackRound: 7
    readonly property int segTall: 20
    readonly property int segPad: 8
    readonly property int segRound: 5
    readonly property int matchSegTall: 22
    readonly property int matchSegPad: 10

    // The joined block of values.
    readonly property int valueTall: 24
    readonly property int valueRound: 7
    readonly property int valuePadLeft: 9
    readonly property int valuePadRight: 3
    readonly property int valueApart: 2
    readonly property int valueSubApart: 5
    readonly property int valueRemove: 18
    readonly property int valueRemoveRound: 5
    readonly property int valueRemoveIcon: 11
    readonly property int valueNoRemove: 6

    // Square buttons: in the head, and inside rows.
    readonly property int button: 28
    readonly property int buttonRound: 7
    readonly property int buttonIcon: 16
    readonly property int rowButton: 24
    readonly property int rowButtonIcon: 14
    readonly property int textButtonTall: 26
    readonly property int textButtonPad: 9
    readonly property int eye: 10

    // Field chips, and the segmented switch with its badge.
    readonly property int fieldChipTall: 26
    readonly property int fieldChipRound: 13
    readonly property int fieldChipPad: 9
    readonly property int fieldChipIcon: 13
    readonly property int fieldChipApart: 5
    readonly property int fieldChipsApart: 4
    readonly property int modeTall: 26
    readonly property int modePad: 10
    readonly property int modeRound: 6
    readonly property int modeTrackRound: 8
    readonly property int modeIcon: 15
    readonly property int badgeTall: 16
    readonly property int badgeRound: 8
    readonly property int badgePad: 4
    readonly property int modeApart: 6
    readonly property real badgeText: 10 * Type.pointsPerPixel

    // The summary chips on the crumb line.
    readonly property int chipTall: 24
    readonly property int chipRound: 12
    readonly property int chipPadLeft: 8
    readonly property int chipPadRight: 3
    readonly property int chipApart: 5
    readonly property int chipIcon: 12
    readonly property int chipsApart: 6
    readonly property int chipRemoveRound: 9
    readonly property int crumbSeparator: 16

    // The add field.
    readonly property int addTall: 30
    readonly property int addRound: 8
    readonly property int addPad: 10
    readonly property int addIcon: 15
    readonly property int addMinWide: 240
    readonly property int addClear: 20

    // A group box (F5) and the add field's ring while it is used.
    readonly property int groupRound: 10
    readonly property int groupAround: 3
    readonly property int groupPadTop: 4
    readonly property int groupPadRight: 4
    readonly property int groupPadBottom: 6
    readonly property int groupPadLeft: 8
    readonly property int groupHead: 30
    readonly property int groupHeadLeft: 2
    readonly property real targetRing: 1.5
    readonly property int addRing: 2
    readonly property int emptyGroupPad: 6

    // Pickers and suggestions.
    readonly property int pickRound: 10
    readonly property int pickGrow: 1000
    readonly property int pickPad: 8
    readonly property int pickApart: 10
    readonly property int pickFieldsWide: 158
    readonly property int pickFieldRow: 28
    readonly property int pickFieldRound: 6
    readonly property int pickFieldIcon: 14
    readonly property int pickTitleIcon: 16
    readonly property int pickSearchTall: 26
    readonly property int pickSearchRound: 6
    readonly property int pickListMax: 196
    readonly property int pickRow: 30
    readonly property int pickRowRound: 6
    readonly property int pickState: 18
    readonly property int pickStateIcon: 15
    readonly property int pickActions: 58
    readonly property int pickAction: 26
    readonly property int pickActionTall: 24
    readonly property int pillTall: 26
    readonly property int pillRound: 13
    readonly property int pillPad: 11
    readonly property int ratingColumnsApart: 12
    readonly property int yearField: 64
    readonly property int yearFieldTall: 28
    readonly property int wordsTall: 30
    readonly property int doneWide: 14
    readonly property int suggestMax: 300
    readonly property int suggestPad: 4
    readonly property int suggestHead: 24
    // How far past the panel the rows are built: a query people type.
    readonly property int rowsBuilt: 100000
    // Below this width the panel stacks its labels and folds its fields (F7).
    readonly property int narrowBelow: 640
    readonly property int narrowChipTall: 24
    readonly property int narrowChipRound: 12

    // Engine notes, console and the detached window.
    readonly property int notesPadY: 8
    readonly property int notesPadX: 14
    readonly property int noteTall: 24
    readonly property int noteRound: 12
    readonly property int consolePadTop: 7
    readonly property int consoleMaxTall: 54
    readonly property int consoleField: 28
    readonly property int windowWide: 520
    readonly property int windowBar: 34
    readonly property int windowButton: 26
    readonly property int windowButtonRound: 6
    readonly property int windowRoom: 140
    readonly property int dockedTall: 34
    readonly property int dockedRound: 10
    readonly property int gripTall: 12
    readonly property int gripWide: 30
    readonly property int gripThick: 3

    // Column exclusion controls (F8).
    readonly property int columnExclude: 20
    readonly property int columnExcludeCorner: 4
    readonly property int columnExcludeIcon: 12

    // Zero results.
    readonly property int zeroRow: 32
    readonly property int zeroRound: 8
    readonly property int zeroPad: 12
    readonly property int zeroTop: 28
    readonly property real zeroTitle: 16 * Type.pointsPerPixel
}

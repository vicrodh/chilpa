pragma Singleton
import QtQuick

// The HiFi Wizard's numbers, QBZ's DacWizardModal: a card as wide as 620,
// 40 from each edge, at least 500 tall and at most nine tenths of the
// window; a header of 68 and a footer of 70, the step rail 18 tall with
// dots of 10; the copy button's check held for two seconds; the document
// read again while the modal is open, and the card asked every second and
// a half while the test plays.
QtObject {
    readonly property int widest: 620
    readonly property int gutter: 80
    readonly property int shortest: 500
    readonly property real tallestShare: 0.9
    readonly property int chrome: 210
    readonly property int header: 68
    readonly property int footer: 70
    readonly property int rail: 18
    readonly property int dot: 10
    readonly property int trackRow: 26
    readonly property int trackGlyph: 14
    readonly property int field: 43
    readonly property int copyButton: 30
    readonly property int configHead: 40
    readonly property int busBox: 18
    readonly property int manualToggle: 22
    readonly property real toneFill: 0.10
    readonly property real toneEdge: 0.30
    readonly property int copiedMs: 2000
    readonly property int readMs: 150
    readonly property int pollMs: 1500
}

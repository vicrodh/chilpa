pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// QBZ's What's New numbers (WhatsNewModal.qml): a panel up to 1040 wide and
// 700 tall, at least 80 short of the window, 24 inside (the kit's 20 and 4);
// a header of 28 with a close of 28 and its glyph of 17; 16 between header,
// body and footer; a footer of 36 with dots of 7, 7 apart. A card's text
// sits 28 in over the left half, 10 apart: an eyebrow of 11 tracked 1.2, a
// title of 26, a body of 14 at 1.35 and a button of 34. The notes are at
// most 720 wide. The art darkens from the left (the `onboarding.art*`
// tokens), painted over the image and not in it, so one image reads in every
// theme; the gradient's middle stop sits at 48 %. It asks again every 1.2 s
// until the window is up and reads the notes while they load.
QtObject {
    readonly property int widest: 1040
    readonly property int tallest: 700
    readonly property int gutter: 80
    readonly property int inset: Metrics.gap4
    readonly property int header: 28
    readonly property int closeBox: 28
    readonly property int closeGlyph: 17
    readonly property int gap: Metrics.gap16
    readonly property int footer: 36
    readonly property int dot: 7
    readonly property int dotGap: dot // derived: the dots sit their own width apart
    readonly property int cardInset: Metrics.gap24 + Metrics.gap4 // derived: QBZ's 28
    readonly property int cardGap: Metrics.gap10
    readonly property int cardButton: 34
    readonly property int notesWidest: 720
    readonly property int tocChip: 26
    readonly property int tocPad: 12
    readonly property int tocGap: Metrics.gap6
    readonly property int sectionTop: 16
    readonly property int sectionBottom: 6
    readonly property int blockBottom: 7
    readonly property int bulletIndent: 16
    readonly property int startMs: 1200
    readonly property int readMs: 150

    readonly property real headingSize: Type.section
    readonly property real eyebrowSize: 11 * Type.pointsPerPixel // derived: QBZ's 11 px
    readonly property real eyebrowTracking: 1.2
    readonly property real titleSize: 26 * Type.pointsPerPixel // derived: QBZ's 26 px
    readonly property real bodySize: Type.slimTitle
    readonly property real bodyLine: 1.35
    readonly property real sectionSize: Type.slimTitle
    readonly property real paragraphSize: Type.body
    readonly property real bulletSize: 12 * Type.pointsPerPixel // derived: QBZ's 12 px
    readonly property real deepBulletSize: eyebrowSize

    readonly property real artMiddleAt: 0.48

    readonly property string art: "qrc:/qt/qml/app/chilpa/about/assets/"
}

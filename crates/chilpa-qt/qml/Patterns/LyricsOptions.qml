import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// How the lyrics are read, from the panel's header: whether the sung line
// is followed, the size, the language of the translation, the color of the
// sung line, capitals, a lighter fill and how much the other lines dim; and
// at the foot, copying the lyrics and going back to how it all began.
//
// It keeps each choice as the window keeps its own ("ui.lyrics."), and the
// panel reads them from here.
Popup {
    id: flyout
    objectName: "lyricsOptions"

    // The lyrics as text, for copying. Empty when there are none.
    property string text
    readonly property bool canCopy: text !== ""

    property bool follow: kept("follow", "true") === "true"
    property string size: kept("size", "m")
    property string translation: kept("translation", "auto")
    property string color: kept("color", "")
    property bool uppercase: kept("uppercase", "false") === "true"
    property bool lite: kept("lite", "false") === "true"
    property string dimming: kept("dimming", "strong")

    // The lyrics went to the clipboard.
    signal copied()

    readonly property var first: ({ follow: true, size: "m", translation: "auto", color: "", uppercase: false, lite: false, dimming: "strong" })
    readonly property var sizes: ["s", "m", "l", "xl"]
    readonly property var dimmingLevels: ["off", "soft", "strong"]
    readonly property var languages: ["auto", "en", "es", "fr", "de", "it", "pt", "nl", "ja", "zh", "ru"]

    function kept(name, fallback) {
        return SettingsModel.kept("ui.lyrics." + name) || fallback
    }

    function choose(name, value) {
        flyout[name] = value
        SettingsModel.keep("ui.lyrics." + name, String(value))
    }

    function reset() {
        for (const name in first)
            choose(name, first[name])
    }

    function copy() {
        if (!canCopy)
            return
        clipboard.write(text)
        copied()
    }

    // A click outside closes it and is nothing else.
    modal: true
    dim: false

    padding: Metrics.gap12
    width: Metrics.lyricsOptions
    margins: Metrics.gap10
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Item {
        Hold {}
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised

            Edge {}
        }
    }

    contentItem: Column {
        spacing: Metrics.gap4

        Clipboard { id: clipboard }

        OptionRow {
            width: parent.width
            title: I18n.tr("Auto-follow", I18n.rev)
            detail: I18n.tr("The line being sung stays in view.", I18n.rev)
            checked: flyout.follow
            onToggled: (on) => flyout.choose("follow", on)
        }
        ChoiceRow {
            width: parent.width
            title: I18n.tr("Size", I18n.rev)
            SegmentedControl {
                options: [{ text: "S" }, { text: "M" }, { text: "L" }, { text: "XL" }]
                current: flyout.sizes.indexOf(flyout.size)
                onActivated: (index) => flyout.choose("size", flyout.sizes[index])
            }
        }
        ChoiceRow {
            width: parent.width
            title: I18n.tr("Translation", I18n.rev)
            SelectField {
                width: Metrics.lyricsLanguage
                values: flyout.languages
                options: [I18n.tr("Auto (app language)", I18n.rev), I18n.tr("English", I18n.rev), I18n.tr("Spanish", I18n.rev),
                    I18n.tr("French", I18n.rev), I18n.tr("German", I18n.rev), I18n.tr("Italian", I18n.rev), I18n.tr("Portuguese", I18n.rev),
                    I18n.tr("Dutch", I18n.rev), I18n.tr("Japanese", I18n.rev), I18n.tr("Chinese", I18n.rev), I18n.tr("Russian", I18n.rev)]
                value: flyout.translation
                menuWidthLimit: Metrics.lyricsOptions
                onPicked: (value) => flyout.choose("translation", value)
            }
        }
        ChoiceRow {
            width: parent.width
            title: I18n.tr("Sung line", I18n.rev)
            Swatches {
                colors: [Theme.color.textPrimary, Theme.color.accent].concat(Theme.lyricColors)
                values: ["", "accent"].concat(Theme.lyricColors)
                value: flyout.color
                onPicked: (value) => flyout.choose("color", value)
            }
        }
        OptionRow {
            width: parent.width
            title: I18n.tr("Uppercase", I18n.rev)
            checked: flyout.uppercase
            onToggled: (on) => flyout.choose("uppercase", on)
        }
        OptionRow {
            width: parent.width
            title: I18n.tr("Lite fill", I18n.rev)
            detail: I18n.tr("The sung line lights up whole. Saves CPU.", I18n.rev)
            checked: flyout.lite
            onToggled: (on) => flyout.choose("lite", on)
        }
        ChoiceRow {
            width: parent.width
            title: I18n.tr("Dimming", I18n.rev)
            SegmentedControl {
                options: [{ text: I18n.tr("Off", I18n.rev) }, { text: I18n.tr("Soft", I18n.rev) }, { text: I18n.tr("Strong", I18n.rev) }]
                current: flyout.dimmingLevels.indexOf(flyout.dimming)
                onActivated: (index) => flyout.choose("dimming", flyout.dimmingLevels[index])
            }
        }
        Row {
            anchors.right: parent.right
            topPadding: Metrics.gap8
            spacing: Metrics.gap8

            PillButton {
                text: I18n.tr("Copy lyrics", I18n.rev)
                glyph: Icons.copy
                enabled: flyout.canCopy
                onClicked: flyout.copy()
            }
            PillButton {
                text: I18n.tr("Reset", I18n.rev)
                onClicked: flyout.reset()
            }
        }
    }
}

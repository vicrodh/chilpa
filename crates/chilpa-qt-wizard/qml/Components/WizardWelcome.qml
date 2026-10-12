import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Step 0: what the wizard does, and the terms that open the Start button.
Column {
    id: step
    property bool accepted: false
    signal toggled()
    spacing: Metrics.gap14

    Text {
        width: parent.width
        text: I18n.tr("Guided DAC Setup", I18n.rev)
        color: Theme.color.textPrimary
        font.pointSize: Type.section
        font.weight: Type.semibold
        wrapMode: Text.WordWrap
    }
    Text {
        width: parent.width
        text: I18n.tr("This wizard helps you configure your DAC for bit-perfect playback on Linux. Chilpa never changes your system automatically — it auto-detects your DACs, shows you the exact commands and config to copy, and lets you test playback right here.", I18n.rev)
        color: Theme.color.textSecondary
        font.pointSize: Type.body
        wrapMode: Text.WordWrap
    }
    Text {
        width: parent.width
        text: I18n.tr("You are responsible for your own backups. This is a helper, not a system recovery tool.", I18n.rev)
        color: Theme.color.textTertiary
        font.pointSize: Type.small
        wrapMode: Text.WordWrap
    }
    WizardBanner {
        variant: "info"
        title: I18n.tr("What to test with", I18n.rev)
        body: I18n.tr("The built-in test plays four tracks spanning different bit-depths and sample rates (16/44.1, 24/44.1, 24/96, 24/192) so you can see your DAC switch. Each is the most played track of your library in that format, or a generated test signal when you have none. If you'd rather use your own queue, add one track of each kind first so every rate is exercised.", I18n.rev)
    }
    WizardBanner {
        variant: "info"
        title: I18n.tr("DSD through PipeWire", I18n.rev)
        body: I18n.tr("This wizard tests PCM rates. Chilpa currently converts DSD to PCM on PipeWire; exclusive mode and rate switching alone cannot verify an untouched DoP stream. For a DAC that supports DSD, use ALSA direct (hw) and choose a supported DSD mode in Audio settings.", I18n.rev)
    }
    CircleCheck {
        objectName: "wizardAccept"
        width: parent.width
        checkable: false
        checked: step.accepted
        text: I18n.tr("I understand and accept these terms", I18n.rev)
        onClicked: step.toggled()
    }
}

import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

// Releases an exclusive output before Settings opens it for diagnosis.
Item {
    id: guard
    required property var settingsPage
    function request() { confirmation.open() }

    ConfirmDialog {
        id: confirmation
        title: I18n.tr("Stop playback to diagnose?", I18n.rev)
        message: I18n.tr("The output is playing in exclusive mode. Diagnosis needs to open it directly, so playback must stop first.", I18n.rev)
        confirmText: I18n.tr("Stop and diagnose", I18n.rev)
        onConfirmed: {
            guard.settingsPage.stop_for_diagnosis()
            waiting.attempts = 0
            waiting.start()
        }
    }

    Timer {
        id: waiting
        interval: 250
        repeat: true
        property int attempts: 0
        onTriggered: {
            attempts += 1
            if (!guard.settingsPage.diagnosis_requires_stop()) {
                stop()
                guard.settingsPage.press("audio.wizard")
            } else if (attempts >= 20) {
                stop()
                failed.open()
            }
        }
    }

    ConfirmDialog {
        id: failed
        title: I18n.tr("The output is still playing", I18n.rev)
        message: I18n.tr("Chilpa could not release the exclusive output. Stop playback and try the diagnosis again.", I18n.rev)
        confirmText: I18n.tr("Try again", I18n.rev)
        onConfirmed: guard.request()
    }
}

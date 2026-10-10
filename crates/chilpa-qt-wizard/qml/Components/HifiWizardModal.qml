import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// QBZ's HiFi Wizard (DacWizardModal.qml): six steps over a rail of dots,
// Welcome, Check, DACs, Configure, Test and Done. The facade owns what it
// computes (the verdict, the DACs and their boxes, the files, the
// read-back); the modal owns the step, the welcome's box, the review's
// three boxes and the manual entry, which start over on every open.
// Escape, the X, a click outside and Close only hide it: what the test
// plays keeps playing.
Modal {
    id: modal
    readonly property var doc: {
        try {
            return JSON.parse(HifiWizard.doc)
        } catch (e) {
            return ({})
        }
    }
    property int step: 0
    property bool welcomeConfirmed: false
    property bool backupDone: false
    property bool configDone: false
    property bool restartDone: false
    property bool showManual: false
    readonly property var stepLabels: [I18n.tr("Welcome", I18n.rev), I18n.tr("Check", I18n.rev), I18n.tr("DACs", I18n.rev),
                                       I18n.tr("Configure", I18n.rev), I18n.tr("Test", I18n.rev), I18n.tr("Done", I18n.rev)]
    // Start needs the terms; Next on the DACs a DAC or a valid node name;
    // Next on the review its three boxes.
    readonly property bool primaryEnabled: step === 0 ? welcomeConfirmed
                                         : step === 2 ? (doc.anyDacSelected === true || doc.manualValid === true)
                                         : step === 3 ? (backupDone && configDone && restartDone) : true
    width: Math.min((parent ? parent.width : WizardFrame.widest) - WizardFrame.gutter, WizardFrame.widest)
    height: Math.min(Math.max(WizardFrame.shortest, body.implicitHeight + WizardFrame.chrome), (parent ? parent.height : WizardFrame.shortest) * WizardFrame.tallestShare)

    readonly property int openSeq: doc.openSeq || 0
    onOpenSeqChanged: {
        step = 0
        welcomeConfirmed = backupDone = configDone = restartDone = showManual = false
        dacs.clear()
    }
    readonly property bool wanted: doc.open === true
    onWantedChanged: wanted ? open() : close()
    onClosed: if (wanted) HifiWizard.command("close", "")

    // A Settings button may have opened it; while it is open its document
    // is read again as the probes land.
    Connections { target: Prefs; function onRevChanged() { HifiWizard.refresh() } }
    Timer { interval: WizardFrame.readMs; repeat: true; running: modal.visible; onTriggered: HifiWizard.refresh() }

    Item {
        width: modal.width - Metrics.modalInsets
        height: modal.height - Metrics.modalInsets

        Item {
            id: header
            width: parent.width
            height: WizardFrame.header - Metrics.modalInset
            Icon {
                id: wand
                anchors.verticalCenter: parent.verticalCenter
                glyph: SourceGlyphs.gandalf
                iconFamily: SourceGlyphs.font
                size: Metrics.iconNav
                color: Theme.color.accent
            }
            Text {
                anchors.left: wand.right
                anchors.leftMargin: Metrics.gap10
                anchors.right: closeButton.left
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("HiFi Wizard — DAC Setup", I18n.rev)
                color: Theme.color.textPrimary
                font.pointSize: Type.titleSmall
                font.weight: Type.semibold
                elide: Text.ElideRight
            }
            IconButton {
                id: closeButton
                objectName: "wizardClose"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                glyph: Lucide.x
                iconFamily: Lucide.font
                tip: I18n.tr("Close", I18n.rev)
                onClicked: modal.close()
            }
        }
        Row {
            id: rail
            anchors.top: header.bottom
            anchors.topMargin: Metrics.gap14
            width: parent.width
            height: WizardFrame.rail
            spacing: Metrics.gap6
            Repeater {
                model: modal.stepLabels
                delegate: Row {
                    required property int index
                    required property string modelData
                    width: (rail.width - rail.spacing * (modal.stepLabels.length - 1)) / modal.stepLabels.length // derived: six equal parts
                    spacing: Metrics.gap6
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: WizardFrame.dot
                        height: WizardFrame.dot
                        radius: WizardFrame.dot / 2 // derived: a circle
                        color: index <= modal.step ? Theme.color.statusPositive : Theme.color.strokeFrame
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - WizardFrame.dot - Metrics.gap6 // derived: beside the dot
                        text: modelData
                        color: index === modal.step ? Theme.color.textPrimary : index < modal.step ? Theme.color.textSecondary : Theme.color.textTertiary
                        font.pointSize: Type.small
                        font.weight: index === modal.step ? Type.semibold : Type.regular
                        elide: Text.ElideRight
                    }
                }
            }
        }
        Flickable {
            id: flick
            anchors.top: rail.bottom
            anchors.topMargin: Metrics.gap14
            anchors.bottom: footer.top
            width: parent.width
            clip: true
            contentHeight: body.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar {}
            Column {
                id: body
                width: flick.width
                topPadding: Metrics.gap10
                bottomPadding: Metrics.gap10
                WizardWelcome { visible: modal.step === 0; width: parent.width; accepted: modal.welcomeConfirmed; onToggled: modal.welcomeConfirmed = !modal.welcomeConfirmed }
                WizardCheck { visible: modal.step === 1; width: parent.width; doc: modal.doc }
                WizardDacs { id: dacs; visible: modal.step === 2; width: parent.width; doc: modal.doc; manual: modal.showManual; onManualToggled: modal.showManual = !modal.showManual }
                WizardConfigure {
                    visible: modal.step === 3
                    width: parent.width
                    doc: modal.doc
                    backupDone: modal.backupDone
                    configDone: modal.configDone
                    restartDone: modal.restartDone
                    onBackupToggled: modal.backupDone = !modal.backupDone
                    onConfigToggled: modal.configDone = !modal.configDone
                    onRestartToggled: modal.restartDone = !modal.restartDone
                }
                WizardTest { visible: modal.step === 4; width: parent.width; doc: modal.doc }
                WizardDone { visible: modal.step === 5; width: parent.width; doc: modal.doc }
            }
        }
        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: WizardFrame.footer - Metrics.modalInset
            PillButton {
                objectName: "wizardBack"
                visible: modal.step > 0
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Back", I18n.rev)
                onClicked: modal.step -= 1
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap12
                PillButton { text: I18n.tr("Close", I18n.rev); onClicked: modal.close() }
                PillButton {
                    objectName: "wizardPrimary"
                    kind: "primary"
                    enabled: modal.primaryEnabled
                    text: modal.step === 0 ? I18n.tr("Start", I18n.rev) : modal.step === 5 ? I18n.tr("Finish", I18n.rev) : I18n.tr("Next", I18n.rev)
                    onClicked: {
                        if (modal.step === 5)
                            return modal.close()
                        // Entering the DACs finds them; entering the review writes the files.
                        if (modal.step === 1)
                            HifiWizard.command("runDetect", "")
                        if (modal.step === 2)
                            HifiWizard.command("genConfigs", "")
                        modal.step += 1
                    }
                }
            }
        }
    }
}

import QtQuick
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The HiFi Wizard: the modal behind its button, and each step with a
// sample document.
Column {
    id: gallery
    readonly property var sample: ({
        healthOk: false, healthSummary: "1 item needs attention before bit-perfect playback will work.",
        distroOptions: ["Gentoo / Funtoo", "Other"], distroIndex: 0, initOptions: ["OpenRC", "Other / unknown"], initIndex: 0,
        remediations: [{ caption: "Install the ALSA bridge so playback can reach PipeWire", command: "sudo emerge pipewire-alsa" }],
        candidates: [{ id: "alsa_output.usb-dac", description: "USB DAC Analog Stereo", bus: "usb", isDefault: true, checked: true, ratesLabel: "44.1 / 96 / 192 kHz" }],
        hasEnumeration: true, backupCmd: "mkdir -p \"$BACKUP\"", restartCmd: "systemctl --user restart pipewire",
        dacConfigs: [{ name: "USB DAC", pipewireConf: "context.properties = {}", pulseConf: "stream.rules = []", wireplumberConf: "monitor.alsa.rules = []", expanded: true }],
        testTracks: ["24-bit / 96 kHz — Generated test signal"], testPlaying: true, testRequestedLabel: "Chilpa requesting 96 kHz · 24-bit",
        testNegotiatedLabel: "DAC: 96 kHz · S32_LE · 2 ch", testRateMatched: true, createdPaths: ["~/.config/pipewire/pipewire.conf.d/99-chilpa-dac-usb-dac.conf"]
    })
    width: Metrics.playbackOptionsWidth * 2 // derived: two option panels wide
    spacing: Metrics.gap16

    PillButton { text: "HiFi Wizard"; onClicked: HifiWizard.command("open", "") }
    HifiWizardModal {}
    WizardBanner { variant: "success"; title: "Audio stack ready"; body: "Your audio stack is ready for bit-perfect playback." }
    CommandBlock { caption: "A command"; command: "wpctl status" }
    WizardWelcome { width: parent.width }
    WizardCheck { width: parent.width; doc: gallery.sample }
    WizardDacs { width: parent.width; doc: gallery.sample }
    WizardConfigure { width: parent.width; doc: gallery.sample }
    WizardTest { width: parent.width; doc: gallery.sample }
    WizardDone { width: parent.width; doc: gallery.sample }
}

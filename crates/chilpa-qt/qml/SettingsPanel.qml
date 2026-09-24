// The preferences, gathered in one place.
//
// Three groups, in the order someone actually cares about them: what comes
// out of the cable, where the music is, and how the window looks. Every
// control writes straight to the bridge, which remembers it; there is no
// "apply" button because there is nothing to apply.
//
// Nothing here is new behaviour. What changes is that the top bar stops
// being the only place these live.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.blitzfc.chilpa

ScrollView {
    id: root
    clip: true

    Component.onCompleted: {
        Chilpa.refreshOutputs()
        Chilpa.refreshVolume()
        Chilpa.refreshLibraryFolders()
    }

    // A heading with its rule, and a labelled row. Both are here so the
    // three groups line up without three copies of the same numbers.
    component Group: ColumnLayout {
        id: group
        property string heading: ""
        Layout.fillWidth: true
        Layout.topMargin: 14
        spacing: 6
        Label {
            text: group.heading
            font.pixelSize: 13
            font.weight: Font.DemiBold
            color: "#411d1f"
        }
        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#1a411d1f"
        }
    }

    component Setting: RowLayout {
        id: setting
        property string label: ""
        Layout.fillWidth: true
        spacing: 10
        Label {
            Layout.preferredWidth: 150
            Layout.alignment: Qt.AlignTop
            text: setting.label
            font.pixelSize: 12
            color: "#7a411d1f"
        }
    }

    ColumnLayout {
        width: root.width - 24
        spacing: 2

        Group { heading: Chilpa.translate("Playback", Chilpa.trRev) }

        Setting {
            label: Chilpa.translate("Output", Chilpa.trRev)
            ComboBox {
                Layout.fillWidth: true
                model: Chilpa.outputs
                currentIndex: {
                    const i = Chilpa.outputIds.indexOf(Chilpa.device)
                    return i >= 0 ? i : 0
                }
                onActivated: Chilpa.choose(
                    index >= 0 && index < Chilpa.outputIds.length
                        ? Chilpa.outputIds[index] : "",
                    Chilpa.wantBitPerfect)
            }
            Button {
                text: Chilpa.translate("Rescan", Chilpa.trRev)
                onClicked: Chilpa.refreshOutputs()
            }
        }

        Setting {
            label: Chilpa.translate("Bit-perfect", Chilpa.trRev)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                CheckBox {
                    text: Chilpa.translate("Deliver the samples untouched", Chilpa.trRev)
                    checked: Chilpa.wantBitPerfect
                    onToggled: Chilpa.choose(Chilpa.device, checked)
                }
                // Said here and not only in the wizard: this is the switch
                // that decides whether a track plays at all when the device
                // cannot take its format.
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.translate(
                        "With this on, a track the device cannot take in its own format does not play, and says so. With it off, Chilpa converts and says that too.",
                        Chilpa.trRev)
                    wrapMode: Text.Wrap
                    font.pixelSize: 10
                    color: "#7a411d1f"
                }
            }
        }

        Setting {
            label: Chilpa.translate("Volume", Chilpa.trRev)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    visible: Chilpa.hasVolume
                    Slider {
                        Layout.fillWidth: true
                        from: 0
                        to: 100
                        stepSize: 1
                        value: Chilpa.volume
                        onMoved: Chilpa.chooseVolume(Math.round(value))
                    }
                    Label {
                        Layout.preferredWidth: 40
                        text: Chilpa.volume + "%"
                        font.pixelSize: 12
                        color: "#411d1f"
                    }
                    Button {
                        visible: Chilpa.canMute
                        text: Chilpa.muted
                            ? Chilpa.translate("Unmute", Chilpa.trRev)
                            : Chilpa.translate("Mute", Chilpa.trRev)
                        onClicked: Chilpa.chooseMuted(!Chilpa.muted)
                    }
                }
                // Said and not hidden: an output with no control of its own
                // is the common case on a DAC, and someone looking for a
                // slider deserves to know why there is none.
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.hasVolume
                        ? Chilpa.translateArgs(
                            "This is the device's own control ({}). Chilpa never scales the samples: that is what bit-perfect means.",
                            [Chilpa.volumeControl], Chilpa.trRev)
                        : Chilpa.translate(
                            "This output has no volume control of its own. Chilpa will not make one up by scaling the samples, because that is the end of bit-perfect: use the knob on the device.",
                            Chilpa.trRev)
                    wrapMode: Text.Wrap
                    font.pixelSize: 10
                    color: "#7a411d1f"
                }
            }
        }

        Group { heading: Chilpa.translate("Library", Chilpa.trRev) }

        Setting {
            label: Chilpa.translate("Folders", Chilpa.trRev)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Repeater {
                    model: Chilpa.libraryFolders
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Label {
                            Layout.fillWidth: true
                            text: modelData
                            elide: Text.ElideMiddle
                            font.pixelSize: 11
                            color: "#411d1f"
                        }
                        // Forgetting deletes nothing: adding the folder back
                        // finds its tracks again, with the listening history
                        // they already had. The button says forget and not
                        // remove for exactly that reason.
                        Button {
                            text: Chilpa.translate("Forget", Chilpa.trRev)
                            onClicked: Chilpa.forgetLibraryFolder(modelData)
                        }
                    }
                }
                Label {
                    visible: Chilpa.libraryFolders.length === 0
                    text: Chilpa.translate("No folder scanned yet", Chilpa.trRev)
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
                RowLayout {
                    spacing: 8
                    Button {
                        text: Chilpa.translate("Add a folder", Chilpa.trRev)
                        onClicked: scanDialog.open()
                    }
                    Button {
                        text: Chilpa.translate("Rescan", Chilpa.trRev)
                        enabled: !Chilpa.scanning && Chilpa.libraryFolders.length > 0
                        onClicked: Chilpa.rescan()
                    }
                }
            }
        }

        Setting {
            label: Chilpa.translate("Analysis", Chilpa.trRev)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                RowLayout {
                    spacing: 8
                    Button {
                        text: Chilpa.analyzing
                            ? Chilpa.translate("Stop the analysis", Chilpa.trRev)
                            : Chilpa.translate("Analyse the library", Chilpa.trRev)
                        onClicked: Chilpa.analyzing
                            ? Chilpa.cancelAnalysis()
                            : Chilpa.analyzeLibrary()
                    }
                    Label {
                        text: Chilpa.analysisStatus
                        font.pixelSize: 11
                        color: "#7a411d1f"
                    }
                }
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.translate(
                        "Reads a minute of each track and works out its tempo, its key and its loudness. It is what fills the tempo lists and AutoMix.",
                        Chilpa.trRev)
                    wrapMode: Text.Wrap
                    font.pixelSize: 10
                    color: "#7a411d1f"
                }
            }
        }

        Setting {
            label: Chilpa.translate("Health", Chilpa.trRev)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                RowLayout {
                    spacing: 8
                    Button {
                        text: Chilpa.translate("Check", Chilpa.trRev)
                        onClicked: Chilpa.checkLibrary()
                    }
                    Button {
                        text: Chilpa.translate("Rebuild the counts", Chilpa.trRev)
                        onClicked: Chilpa.repairLibrary()
                    }
                }
                // Said plainly because someone reaching for a Repair button
                // wants to know what it is about to do to their library.
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.translate(
                        "Checking reads the whole file and reports what it finds. Rebuilding remakes the play counts and the search index from the tracks and the play log, which it does not touch: nothing is lost by running it.",
                        Chilpa.trRev)
                    wrapMode: Text.Wrap
                    font.pixelSize: 10
                    color: "#7a411d1f"
                }
            }
        }

        Group { heading: Chilpa.translate("Appearance", Chilpa.trRev) }

        Setting {
            label: Chilpa.translate("Language", Chilpa.trRev)
            LanguageSelector { Layout.preferredWidth: 160 }
            Item { Layout.fillWidth: true }
        }

        Setting {
            label: Chilpa.translate("Layout", Chilpa.trRev)
            LayoutSelector { Layout.preferredWidth: 160 }
            Item { Layout.fillWidth: true }
        }

        Setting {
            label: Chilpa.translate("Visualizer", Chilpa.trRev)
            ComboBox {
                Layout.preferredWidth: 160
                // The id is column 0 and the name column 1: what is kept and
                // compared is the id, so it does not move with the language.
                model: Chilpa.visualizers.map(v => v.split("\t")[1])
                currentIndex: Math.max(0, Chilpa.visualizers.findIndex(
                    v => v.split("\t")[0] === Chilpa.visualizer))
                onActivated: Chilpa.chooseVisualizer(
                    Chilpa.visualizers[index].split("\t")[0])
            }
            Item { Layout.fillWidth: true }
        }

        Item { Layout.preferredHeight: 10 }
    }

    FolderDialog {
        id: scanDialog
        currentFolder: Chilpa.lastFolder === "" ? "" : "file://" + Chilpa.lastFolder
        title: Chilpa.translate("Scan a folder into the library", Chilpa.trRev)
        onAccepted: {
            Chilpa.scanFolder(selectedFolder)
            Chilpa.refreshLibraryFolders()
        }
    }
}

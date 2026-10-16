import QtQuick
import app.chilpa
import app.chilpa.metadata
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The head of the metadata editor: which record, what happened last, where
// a save writes -- a sidecar beside the music, or the files themselves --
// and revert, close and save. `doc` is the editor's view.
Item {
    id: root

    property var doc: ({})

    signal saveAsked()
    signal closeAsked()

    readonly property var save: doc.save || ({})
    readonly property var layers: doc.layers || ({})
    readonly property bool files: save.mode === "files"
    readonly property bool xml: save.mode === "xml"
    readonly property string album: (doc.album || []).length > 0 ? doc.album[0].value : ""
    readonly property string layerLine: (layers.present || []).map(l => l.name + " (" + l.files + ")").join(", ")

    implicitHeight: head.height + Metrics.gap12 + options.height

    Column {
        id: head
        width: parent.width - buttons.width - Metrics.gap16
        spacing: Metrics.gap4

        Text {
            text: I18n.tr("Edit metadata", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: root.album + (root.doc.tracks ? " · " + I18n.trCount("{} track", "{} tracks", root.doc.tracks.length, I18n.rev) : "")
            elide: Text.ElideRight
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: root.doc.state === "saving" && root.save.total > 0
                  ? I18n.trArgs("Saving… {} of {} files", [root.save.done, root.save.total], I18n.rev)
                  : root.doc.state === "saving" ? I18n.tr("Saving…", I18n.rev)
                  : root.doc.error || root.doc.notice || ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: root.doc.error ? Theme.color.statusNegative : Theme.color.statusPositive
        }
    }

    Row {
        id: buttons
        anchors.right: parent.right
        spacing: Metrics.gap8
        PillButton {
            kind: "ghost"
            text: I18n.tr("Revert", I18n.rev)
            enabled: root.doc.dirty === true && root.doc.state === "ready"
            onClicked: {
                // The field being typed in lets go, and shows what the form holds.
                root.forceActiveFocus()
                MetadataEditor.revert()
            }
        }
        PillButton { text: I18n.tr("Close", I18n.rev); enabled: root.doc.state !== "saving"; onClicked: root.closeAsked() }
        PillButton {
            kind: "primary"
            text: I18n.tr("Save", I18n.rev)
            enabled: root.doc.dirty === true && root.doc.state === "ready"
            onClicked: root.saveAsked()
        }
    }

    Flow {
        id: options
        y: head.height + Metrics.gap12
        width: parent.width
        spacing: Metrics.gap12

        Text {
            height: Metrics.segmentHeight
            verticalAlignment: Text.AlignVCenter
            text: I18n.tr("Save to", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        SegmentedControl {
            options: root.save.xml_allowed
                     ? [{ text: I18n.tr("Sidecar", I18n.rev) }, { text: I18n.tr("Files", I18n.rev) }, { text: I18n.tr("SACD XML", I18n.rev) }]
                     : [{ text: I18n.tr("Sidecar", I18n.rev) }, { text: I18n.tr("Files", I18n.rev) }]
            current: root.xml ? 2 : root.files ? 1 : 0
            onActivated: (index) => MetadataEditor.setMode(index === 2 ? "xml" : index === 1 ? "files" : "sidecar")
        }
        Text {
            visible: root.xml
            width: Math.min(implicitWidth, options.width)
            height: Math.max(Metrics.segmentHeight, implicitHeight)
            verticalAlignment: Text.AlignVCenter
            text: I18n.tr("The ISO stays untouched; fields without an XML place and cover art use a sidecar.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Text {
            visible: !root.save.files_allowed && root.save.files_refused !== ""
            height: Metrics.segmentHeight
            verticalAlignment: Text.AlignVCenter
            text: root.save.files_refused || ""
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
        SegmentedControl {
            visible: root.files
            options: [{ text: "ID3v2.4" }, { text: "ID3v2.3" }]
            current: root.save.id3 === "2.3" ? 1 : 0
            onActivated: (index) => MetadataEditor.setId3(index === 1 ? "2.3" : "2.4")
        }
        Row {
            visible: root.files
            spacing: Metrics.gap6
            Toggle {
                anchors.verticalCenter: parent.verticalCenter
                checked: root.save.secondary_layers === true
                onToggled: (on) => MetadataEditor.setSecondaryLayers(on)
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Also write the other tag layers", I18n.rev)
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }
        Text {
            visible: root.layerLine !== ""
            height: Metrics.segmentHeight
            verticalAlignment: Text.AlignVCenter
            text: I18n.trArgs("Tags: {}", [root.layerLine], I18n.rev)
                  + (root.layers.conflicting > 0 ? " · " + I18n.trCount("{} file whose tags disagree", "{} files whose tags disagree", root.layers.conflicting, I18n.rev) : "")
            font.pointSize: Type.meta
            color: root.layers.conflicting > 0 ? Theme.color.statusWarning : Theme.color.textTertiary
        }
    }
}

import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"

// The metadata editor on one record (the route's id), laid out as QBZ's:
// the tracks as a spreadsheet and the record's column beside them, under a
// header that stays. The form lives in the facade; this screen shows what
// `MetadataEditor.document` says and hands every change back. Writing into
// the files is asked first; closing with changes, too. A save that lands
// takes the person to the record.
Item {
    id: page

    readonly property var doc: JSON.parse(MetadataEditor.document(MetadataEditor.rev))
    readonly property real contentWidth: width - Metrics.contentX * 2 // derived: padding on both sides
    property bool saving: false

    function ensure() {
        if (Navigation.screen !== "metadata" || Navigation.routeId <= 0)
            return
        if (doc.state !== "closed" && doc.album_id === Navigation.routeId)
            return
        // One record is open at a time: another with changes asks first.
        if (doc.state === "ready" && doc.dirty)
            switchRecord.open()
        else
            MetadataEditor.openAlbum(Navigation.routeId, 0)
    }
    Component.onCompleted: ensure()
    Connections {
        target: Navigation
        function onRouteIdChanged() { page.ensure() }
    }

    onDocChanged: {
        if (doc.state === "saving")
            saving = true
        else if (saving) {
            saving = false
            // Saved: the record, as the library now has it.
            if (doc.state === "closed" && doc.error === "")
                Navigation.go("album", doc.album_id)
        }
    }

    function askSave() {
        page.forceActiveFocus()
        if (doc.save && doc.save.mode === "files")
            writeFiles.open()
        else
            MetadataEditor.save()
    }
    function askClose() {
        page.forceActiveFocus()
        if (doc.dirty)
            discard.open()
        else
            leave()
    }
    function leave() {
        MetadataEditor.close()
        if (Navigation.canBack)
            Navigation.back()
        else
            Navigation.go("album", Navigation.routeId)
    }

    MetadataHeader {
        id: header
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.contentWidth
        doc: page.doc
        onSaveAsked: page.askSave()
        onCloseAsked: page.askClose()
    }

    Row {
        id: reopen
        x: Metrics.contentX
        anchors.top: header.bottom
        anchors.topMargin: Metrics.gap12
        visible: page.doc.state === "closed" || page.doc.state === "opening"
        height: visible ? Metrics.pillHeight : 0
        spacing: Metrics.gap8
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: page.doc.state === "opening" ? I18n.tr("Reading the files…", I18n.rev) : I18n.tr("The record is not open in the editor.", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        PillButton {
            visible: page.doc.state === "closed"
            text: I18n.tr("Open it again", I18n.rev)
            onClicked: MetadataEditor.openAlbum(page.doc.album_id > 0 ? page.doc.album_id : Navigation.routeId, 0)
        }
    }

    // Two panes side by side, each scrolling on its own; the page does not.
    // They end where the player bar begins.
    Item {
        id: body
        x: Metrics.contentX
        anchors.top: reopen.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.playerHeight + Metrics.playerBottom * 2 // derived: clear of the player bar
        width: page.contentWidth
        visible: page.doc.state === "ready" || page.doc.state === "saving"
        enabled: page.doc.state === "ready"

        MetadataTracks {
            id: tracks
            width: Math.min(Math.floor((body.width - Metrics.gap12) * 8 / 12), body.width - Metrics.gap12 - Metrics.metadataColumnMin) // derived: QBZ's 8 of 12, leaving the record's column its least
            height: body.height
            doc: page.doc
        }
        MetadataAlbum {
            x: tracks.width + Metrics.gap12
            width: body.width - x
            height: body.height
            doc: page.doc
        }
    }

    ConfirmDialog {
        id: writeFiles
        title: I18n.tr("Write into the audio files?", I18n.rev)
        message: I18n.tr("Each file is copied, the copy is written and read back, and only then does it replace the original. It needs as much free space as the record takes. What is written cannot be undone.", I18n.rev)
        confirmText: I18n.tr("Write into the files", I18n.rev)
        onConfirmed: MetadataEditor.save()
    }
    ConfirmDialog {
        id: switchRecord
        title: I18n.tr("Discard the changes to the other record?", I18n.rev)
        message: I18n.trArgs("“{}” has changes that were not saved. Opening this record loses them.", [(page.doc.album || []).length > 0 ? page.doc.album[0].value : ""], I18n.rev)
        confirmText: I18n.tr("Discard", I18n.rev)
        destructive: true
        onConfirmed: MetadataEditor.openAlbum(Navigation.routeId, 0)
        onDismissed: Navigation.go("metadata", page.doc.album_id)
    }
    ConfirmDialog {
        id: discard
        title: I18n.tr("Discard the changes?", I18n.rev)
        message: I18n.tr("Nothing has been saved. Closing the editor loses what was changed.", I18n.rev)
        confirmText: I18n.tr("Discard", I18n.rev)
        destructive: true
        onConfirmed: page.leave()
    }
}

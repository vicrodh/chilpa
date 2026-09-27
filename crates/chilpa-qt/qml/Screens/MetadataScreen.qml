import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"

// The metadata editor on one record (the route's id). The form lives in
// the facade; this screen shows what `MetadataEditor.document` says and
// hands every change back. Writing into the files is asked first; closing
// with changes, too. A save that lands takes the person to the record.
Item {
    id: page

    readonly property var doc: JSON.parse(MetadataEditor.document(MetadataEditor.rev))
    readonly property real contentWidth: width - Metrics.contentX * 2 // derived: padding on both sides
    property bool saving: false

    function ensure() {
        if (Navigation.screen !== "metadata" || Navigation.routeId <= 0)
            return
        if (doc.state === "closed" || doc.album_id !== Navigation.routeId)
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
        if (doc.save && doc.save.mode === "files")
            writeFiles.open()
        else
            MetadataEditor.save()
    }
    function askClose() {
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

    Flickable {
        id: body
        x: Metrics.contentX
        anchors.top: reopen.bottom
        anchors.topMargin: Metrics.section
        anchors.bottom: parent.bottom
        width: page.contentWidth
        visible: page.doc.state === "ready" || page.doc.state === "saving"
        enabled: page.doc.state === "ready"
        clip: true
        contentHeight: Math.max(fields.height, side.height) + Metrics.contentBottom
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        MetadataFields {
            id: fields
            width: body.width - Metrics.metadataSide - Metrics.gridGap
            doc: page.doc
        }
        Column {
            id: side
            x: fields.width + Metrics.gridGap
            width: Metrics.metadataSide
            spacing: Metrics.section
            MetadataArtwork { width: parent.width; doc: page.doc }
            MetadataLookup { width: parent.width; doc: page.doc }
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
        id: discard
        title: I18n.tr("Discard the changes?", I18n.rev)
        message: I18n.tr("Nothing has been saved. Closing the editor loses what was changed.", I18n.rev)
        confirmText: I18n.tr("Discard", I18n.rev)
        destructive: true
        onConfirmed: page.leave()
    }
}

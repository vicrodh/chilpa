import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/wizard/qml/Components"

// Removing a server, built as the app's other confirmations are
// (ConfirmDialog): one panel, a title, what happens, and the way out. It has
// three states in that same panel. First the question, with what leaves the
// library. Then, once confirmed, what is happening, with a spinner and a
// bar the render thread moves, and nothing to press: it cannot be closed
// until the removal ends. A failure says what went wrong, what to do and
// the library's own words to copy, with Try again; success closes it, and
// the window says so in a notice. Each state fades in; none is drawn over
// another.
Modal {
    id: dialog
    // The facts `Servers.removal` gives, as JSON: the server's name and
    // what leaves with it. Numbers are written in the person's locale.
    property string message
    property int serverId: -1
    property var preview: null
    property bool sent: false
    readonly property var facts: JSON.parse(message || "{}")
    readonly property var setup: preview || JSON.parse(Servers.setup || "{}")
    readonly property var removal: setup.removing && setup.removing.id === serverId ? setup.removing : ({})
    readonly property string phase: removal.state === "failed" ? "failed" : sent || removal.state === "removing" ? "removing" : "confirm"
    readonly property string name: facts.name || removal.name || ""

    width: Metrics.confirmWidth
    closePolicy: phase === "removing" ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

    onAboutToShow: sent = false
    onClosed: if (!preview) Servers.cancel()
    // Closed after the change is taken in: closing says so to the facade,
    // which changes the state being read.
    onRemovalChanged: if (sent && removal.state === "done") Qt.callLater(close)
    onPhaseChanged: if (phase === "failed") sent = false

    function remove() {
        sent = true
        if (preview) return
        Servers.remove(serverId)
        Servers.refresh()
    }
    // A count's words, with its number written in the person's locale.
    function counted(words, n) {
        return words.replace("{}", Number(n || 0).toLocaleString(Qt.locale(), 'f', 0))
    }
    readonly property string said: {
        const f = facts
        const parts = [I18n.trArgs("{} and {} leave your library.", [counted(I18n.trN("{} track", "{} tracks", f.tracks || 0, I18n.rev), f.tracks), counted(I18n.trN("{} album", "{} albums", f.albums || 0, I18n.rev), f.albums)], I18n.rev)]
        const kept = []
        if ((f.ratings || 0) > 0) kept.push(counted(I18n.trN("{} rating", "{} ratings", f.ratings || 0, I18n.rev), f.ratings))
        if ((f.plays || 0) > 0) kept.push(counted(I18n.trN("{} play", "{} plays", f.plays || 0, I18n.rev), f.plays))
        if (kept.length === 2) parts.push(I18n.trArgs("Your {} and {} of them are removed too.", kept, I18n.rev))
        if (kept.length === 1) parts.push(I18n.trArgs("Your {} of them are removed too.", kept, I18n.rev))
        if ((f.playlists || 0) > 0)
            parts.push(I18n.trArgs("Its {} leave too.", [counted(I18n.trN("{} playlist", "{} playlists", f.playlists || 0, I18n.rev), f.playlists)], I18n.rev))
        if ((f.entries || 0) > 0)
            parts.push(I18n.trArgs("{} of its tracks leave {} of your playlists.", [Number(f.entries).toLocaleString(Qt.locale(), 'f', 0), Number(f.touched || 0).toLocaleString(Qt.locale(), 'f', 0)], I18n.rev))
        parts.push(I18n.tr("Your music files on the server are not touched.", I18n.rev))
        return parts.join(" ")
    }

    // How the removal goes, read while it runs.
    Timer {
        interval: Motion.poll
        repeat: true
        running: dialog.visible && !dialog.preview && dialog.phase === "removing"
        onTriggered: Servers.refresh()
    }

    Column {
        width: parent.width
        spacing: Metrics.gap12

        Text {
            objectName: "serverRemoveTitle"
            width: parent.width
            text: dialog.phase === "removing" ? I18n.trArgs("Removing {}", [dialog.name], I18n.rev)
                : dialog.phase === "failed" ? (dialog.removal.title || "")
                : I18n.trArgs("Remove {}?", [dialog.name], I18n.rev)
            textFormat: Text.PlainText
            // A long name takes another line, even one without spaces.
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }

        // The question: what leaves the library, and what stays.
        Text {
            objectName: "serverRemoveFacts"
            visible: dialog.phase === "confirm"
            opacity: visible ? 1 : 0
            Behavior on opacity { OpacityAnimator { duration: Motion.fast } }
            width: parent.width
            text: dialog.said
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }

        // While it runs: what is happening, and that it is alive.
        Column {
            objectName: "serverRemoving"
            visible: dialog.phase === "removing"
            opacity: visible ? 1 : 0
            Behavior on opacity { OpacityAnimator { duration: Motion.fast } }
            width: parent.width
            spacing: Metrics.gap12
            Row {
                width: parent.width
                spacing: Metrics.gap12
                // The app's busy mark as a glyph (BusyMark), larger and in
                // the accent, turned by the render thread: it moves even
                // while the window is busy a moment.
                Icon {
                    id: spinner
                    objectName: "serverRemovingSpinner"
                    size: Metrics.button32
                    glyph: Icons.loader2
                    color: Theme.color.accent
                    anchors.verticalCenter: parent.verticalCenter
                    RotationAnimator on rotation {
                        from: 0
                        to: 360
                        duration: Motion.busyLoop
                        loops: Animation.Infinite
                        running: dialog.visible && dialog.phase === "removing" && !Motion.reduced
                    }
                }
                Text {
                    objectName: "serverRemovingText"
                    width: parent.width - spinner.width - parent.spacing
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Its songs, albums and playlists are leaving your library. This can take a moment; you can keep using Chilpa meanwhile.", I18n.rev)
                    wrapMode: Text.WordWrap
                    font.pointSize: Type.body
                    color: Theme.color.textSecondary
                }
            }
            ProgressBar {
                width: parent.width
                indeterminate: true
                palette.dark: Theme.color.accent
                palette.midlight: Theme.color.fillField
            }
        }

        // It failed: what to do, and the library's words to copy.
        Column {
            objectName: "serverRemoveFailure"
            visible: dialog.phase === "failed"
            opacity: visible ? 1 : 0
            Behavior on opacity { OpacityAnimator { duration: Motion.fast } }
            width: parent.width
            spacing: Metrics.gap8
            WizardBanner {
                objectName: "serverRemoveFailed"
                variant: "error"
                body: dialog.removal.advice || ""
            }
            TextEdit {
                objectName: "serverRemoveDetail"
                visible: text !== ""
                width: parent.width
                text: dialog.removal.detail || ""
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }

        Row {
            visible: dialog.phase !== "removing"
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton {
                objectName: "serverRemoveCancel"
                text: dialog.phase === "failed" ? I18n.tr("Close", I18n.rev) : I18n.tr("Cancel", I18n.rev)
                onClicked: dialog.close()
            }
            PillButton {
                objectName: "serverRemoveConfirm"
                kind: "secondary"
                text: dialog.phase === "failed" ? I18n.tr("Try again", I18n.rev) : I18n.tr("Remove", I18n.rev)
                onClicked: dialog.remove()
            }
        }
    }
}

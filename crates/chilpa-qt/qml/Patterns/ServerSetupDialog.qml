import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Adding a server of mine (ADR-017), one step at a time. Every step is the
// facade's (`Servers.setup`): which kind, its address, signing in or the
// code to type elsewhere, which server of a Plex account, which libraries.
// Nothing here goes to the network; the dialog shows where the setup is.
Modal {
    id: dialog

    readonly property var setup: JSON.parse(Servers.setup || "{}")
    readonly property string step: setup.step || "idle"
    readonly property bool busy: setup.busy === true
    // The libraries chosen, all of them until one is turned off.
    property var chosen: []
    property bool useKey: false

    width: Metrics.confirmWidth
    onOpened: {
        chosen = []
        useKey = false
        Servers.cancel()
    }
    onClosed: if (step !== "done") Servers.cancel()

    // The setup moves on its own threads: the dialog reads it while open.
    Timer {
        interval: Motion.poll
        repeat: true
        running: dialog.visible
        onTriggered: Servers.refresh()
    }

    Column {
        width: parent.width
        spacing: Metrics.gap12

        Text {
            width: parent.width
            text: dialog.step === "done" ? I18n.tr("Server added", I18n.rev) : I18n.tr("Add a server", I18n.rev)
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: dialog.step === "idle" ? I18n.tr("Its music joins your library, listed with your own files.", I18n.rev)
                : dialog.step === "code" && (dialog.setup.link || "") !== "" ? I18n.tr("Type this code at plex.tv/link, signed in to your Plex account.", I18n.rev)
                : dialog.step === "code" ? I18n.tr("Type this code in Quick Connect, in a Jellyfin you are signed in to.", I18n.rev)
                : dialog.step === "servers" ? I18n.tr("Your account reaches more than one server. Which one?", I18n.rev)
                : dialog.step === "libraries" ? I18n.tr("Which of its libraries come in?", I18n.rev)
                : dialog.step === "done" ? I18n.tr("Its music is being read; it appears in the library as it arrives.", I18n.rev)
                : dialog.setup.product || ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }

        // Which kind.
        Row {
            visible: dialog.step === "idle"
            spacing: Metrics.gap8
            Repeater {
                model: [["subsonic", "OpenSubsonic"], ["jellyfin", "Jellyfin"], ["plex", "Plex"]]
                PillButton {
                    required property var modelData
                    text: modelData[1]
                    onClicked: Servers.begin(modelData[0])
                }
            }
        }

        // Its address.
        TextField {
            id: address
            visible: dialog.step === "address"
            width: parent.width
            placeholder: dialog.setup.kind === "jellyfin" ? "http://nas.local:8096" : "http://music.local:4533"
            onAccepted: Servers.address(text)
        }

        // Signing in.
        Row {
            visible: dialog.step === "signin" && dialog.setup.apiKey === true
            spacing: Metrics.gap8
            Toggle {
                anchors.verticalCenter: parent.verticalCenter
                checked: dialog.useKey
                onToggled: (on) => dialog.useKey = on
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Sign in with an API key", I18n.rev)
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
        }
        TextField {
            id: user
            visible: dialog.step === "signin" && !dialog.useKey
            width: parent.width
            placeholder: I18n.tr("User", I18n.rev)
        }
        TextField {
            id: secret
            visible: dialog.step === "signin"
            width: parent.width
            secret: true
            placeholder: dialog.useKey ? I18n.tr("API key", I18n.rev) : I18n.tr("Password", I18n.rev)
            onAccepted: Servers.signIn(user.text, text, dialog.useKey)
        }

        // The code to type elsewhere.
        Text {
            visible: dialog.step === "code" && (dialog.setup.code || "") !== ""
            text: dialog.setup.code || ""
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }

        // A server of the account, or a library of the server.
        Column {
            visible: dialog.step === "servers" || dialog.step === "libraries"
            width: parent.width
            spacing: Metrics.gap4
            Repeater {
                model: dialog.setup.choices || []
                Row {
                    required property var modelData
                    spacing: Metrics.gap8
                    Toggle {
                        visible: dialog.step === "libraries"
                        anchors.verticalCenter: parent.verticalCenter
                        checked: dialog.chosen.indexOf(modelData.id) < 0
                        onToggled: (on) => {
                            const off = dialog.chosen.filter((id) => id !== modelData.id)
                            dialog.chosen = on ? off : off.concat([modelData.id])
                        }
                    }
                    PillButton {
                        visible: dialog.step === "servers"
                        text: modelData.name
                        onClicked: Servers.pick(modelData.id)
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: dialog.step === "libraries" ? modelData.name : modelData.detail
                        font.pointSize: Type.body
                        color: dialog.step === "libraries" ? Theme.color.textPrimary : Theme.color.textSecondary
                    }
                }
            }
        }

        Text {
            width: parent.width
            visible: dialog.busy || (dialog.setup.error || "") !== ""
            text: dialog.busy ? I18n.tr("Working…", I18n.rev) : dialog.setup.error || ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: dialog.busy ? Theme.color.textTertiary : Theme.color.statusNegative
        }

        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton {
                visible: dialog.step === "signin" && dialog.setup.quickConnect === true
                text: I18n.tr("Use Quick Connect", I18n.rev)
                enabled: !dialog.busy
                onClicked: Servers.quickConnect()
            }
            PillButton {
                visible: dialog.step === "code" && (dialog.setup.link || "") !== ""
                text: I18n.tr("Open plex.tv/link", I18n.rev)
                glyph: Icons.arrowUpRight
                onClicked: Qt.openUrlExternally(dialog.setup.link)
            }
            PillButton {
                text: dialog.step === "done" ? I18n.tr("Close", I18n.rev) : I18n.tr("Cancel", I18n.rev)
                onClicked: dialog.close()
            }
            PillButton {
                visible: ["address", "signin", "libraries"].indexOf(dialog.step) >= 0
                kind: "primary"
                // Every library switched off is not a choice.
                enabled: !dialog.busy && !(dialog.step === "libraries" && dialog.chosen.length >= (dialog.setup.choices || []).length)
                text: dialog.step === "libraries" ? I18n.tr("Add server", I18n.rev)
                    : dialog.step === "signin" ? I18n.tr("Sign in", I18n.rev) : I18n.tr("Continue", I18n.rev)
                onClicked: {
                    if (dialog.step === "address")
                        Servers.address(address.text)
                    else if (dialog.step === "signin")
                        Servers.signIn(user.text, secret.text, dialog.useKey)
                    else {
                        const all = (dialog.setup.choices || []).map((c) => c.id)
                        Servers.finish(JSON.stringify(all.filter((id) => dialog.chosen.indexOf(id) < 0)))
                    }
                }
            }
        }
    }
}

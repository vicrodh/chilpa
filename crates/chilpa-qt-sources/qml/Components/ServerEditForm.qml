import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: form
    signal closed()
    property var preview: null
    property bool editing: true
    readonly property var setup: preview || JSON.parse(Servers.setup || "{}")
    readonly property string step: setup.step || "idle"
    readonly property bool busy: setup.busy === true
    property var excluded: []
    property bool useKey: false
    implicitHeight: fields.implicitHeight

    Component.onCompleted: {
        label.text = setup.label || ""
        address.text = setup.address || ""
    }
    onStepChanged: {
        if (step === "libraries") {
            const selected = setup.selectedLibraries || []
            if (selected.length > 0)
                excluded = (setup.choices || []).map((c) => c.id).filter((id) => selected.indexOf(id) < 0)
        }
        if (step === "done" && editing) closed()
    }
    Timer {
        interval: Motion.poll
        repeat: true
        running: form.visible && !form.preview && (form.busy || form.step === "code")
        onTriggered: Servers.refresh()
    }
    Column {
        id: fields
        width: parent.width
        spacing: Metrics.gap8
        Flow {
            visible: form.step === "idle" && !form.editing
            width: parent.width
            spacing: Metrics.gap8
            Repeater {
                model: [["subsonic", "OpenSubsonic"], ["jellyfin", "Jellyfin"], ["plex", "Plex"]]
                PillButton {
                    required property var modelData
                    text: modelData[1]
                    enabled: !form.preview
                    onClicked: Servers.begin(modelData[0])
                }
            }
        }
        TextField {
            id: label
            objectName: "serverLabel"
            visible: form.editing || form.step === "libraries"
            width: parent.width
            placeholder: I18n.tr("Server name", I18n.rev)
        }
        TextField {
            id: address
            objectName: "serverAddress"
            visible: form.step === "address" && form.setup.kind !== "plex"
            width: parent.width
            placeholder: I18n.tr("Server address", I18n.rev)
            onAccepted: if (!form.busy && !form.preview) Servers.address(text)
        }
        Row {
            visible: form.step === "signin" && form.setup.apiKey === true
            spacing: Metrics.gap8
            Toggle { checked: form.useKey; onToggled: (on) => form.useKey = on }
            Text { text: I18n.tr("Sign in with an API key", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
        }
        TextField {
            id: user
            objectName: "serverUser"
            visible: form.step === "signin" && !form.useKey
            width: parent.width
            placeholder: I18n.tr("User", I18n.rev)
        }
        TextField {
            id: secret
            objectName: "serverSecret"
            visible: form.step === "signin"
            width: parent.width
            secret: true
            placeholder: form.useKey ? I18n.tr("API key", I18n.rev) : I18n.tr("Password", I18n.rev)
            onAccepted: if (!form.busy && !form.preview) Servers.signIn(user.text, text, form.useKey)
        }
        Text {
            visible: form.step === "code" && (form.setup.code || "") !== ""
            text: form.setup.code || ""
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Column {
            visible: form.step === "servers" || form.step === "libraries"
            width: parent.width
            spacing: Metrics.gap4
            Repeater {
                model: form.setup.choices || []
                Row {
                    required property var modelData
                    width: parent.width
                    spacing: Metrics.gap8
                    Toggle {
                        id: libraryToggle
                        visible: form.step === "libraries"
                        checked: form.excluded.indexOf(modelData.id) < 0
                        onToggled: (on) => {
                            const off = form.excluded.filter((id) => id !== modelData.id)
                            form.excluded = on ? off : off.concat([modelData.id])
                        }
                    }
                    PillButton {
                        id: serverChoice
                        visible: form.step === "servers"
                        text: modelData.name
                        onClicked: Servers.pick(modelData.id)
                    }
                    Text {
                        width: parent.width - (libraryToggle.visible ? libraryToggle.width : serverChoice.width) - Metrics.gap8
                        elide: Text.ElideRight
                        text: form.step === "libraries" ? modelData.name : modelData.detail
                        font.pointSize: Type.body
                        color: Theme.color.textPrimary
                    }
                }
            }
        }
        Text {
            visible: form.busy || (form.setup.error || "") !== ""
            width: parent.width
            wrapMode: Text.WordWrap
            text: form.busy ? I18n.tr("Working…", I18n.rev) : form.setup.error || ""
            font.pointSize: Type.meta
            color: form.busy ? Theme.color.textTertiary : Theme.color.statusNegative
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton {
                visible: form.editing && form.step !== "signin" && form.step !== "code" && form.step !== "idle"
                text: I18n.tr("Change sign-in", I18n.rev)
                enabled: !form.busy && !form.preview
                onClicked: { Servers.begin("reauth"); if (form.setup.kind !== "plex") Servers.address(address.text) }
            }
            PillButton {
                visible: form.step === "signin" && form.setup.quickConnect === true
                text: I18n.tr("Use Quick Connect", I18n.rev)
                enabled: !form.preview && !form.busy
                onClicked: Servers.quickConnect()
            }
            PillButton {
                visible: form.step === "code" && (form.setup.link || "") !== ""
                text: I18n.tr("Open plex.tv/link", I18n.rev)
                enabled: !form.preview
                onClicked: Qt.openUrlExternally(form.setup.link)
            }
            PillButton { text: form.step === "done" ? I18n.tr("Close", I18n.rev) : I18n.tr("Cancel", I18n.rev); onClicked: form.closed() }
            PillButton {
                visible: form.step === "address" || form.step === "signin" || form.step === "libraries"
                objectName: "serverContinue"
                kind: "primary"
                enabled: !form.preview && !form.busy && (form.step !== "libraries" || form.excluded.length < (form.setup.choices || []).length)
                text: form.step === "libraries" ? (form.editing ? I18n.tr("Save changes", I18n.rev) : I18n.tr("Add server", I18n.rev))
                    : form.step === "signin" ? I18n.tr("Sign in", I18n.rev) : I18n.tr("Continue", I18n.rev)
                onClicked: {
                    if (form.step === "address") Servers.address(address.text)
                    else if (form.step === "signin") Servers.signIn(user.text, secret.text, form.useKey)
                    else {
                        const chosen = (form.setup.choices || []).map((c) => c.id).filter((id) => form.excluded.indexOf(id) < 0)
                        Servers.finish(JSON.stringify({label: label.text, libraries: chosen}))
                    }
                }
            }
        }
    }
}

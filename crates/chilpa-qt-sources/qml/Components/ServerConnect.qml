import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// How a server is reached, before it is tested: its address and, for the
// kinds that sign in, who signs in. A media server on this network is
// chosen from what a search found, or typed. Plex has nothing to type: its
// code comes on the next step. Editing keeps the saved sign-in unless it is
// changed.
Column {
    id: form
    property string kind: ""
    property var setup: ({})
    property bool editing: false
    property bool preview: false
    readonly property alias address: addressField.text
    readonly property alias user: userField.text
    readonly property alias secret: secretField.text
    property bool useKey: false
    property bool quick: false
    property bool changeSignIn: false
    readonly property bool signsIn: kind === "subsonic" || kind === "jellyfin"
    readonly property bool asksSignIn: signsIn && (!editing || changeSignIn)
    readonly property bool ready: kind === "plex" && !editing ? true
        : address.trim() !== "" && (!asksSignIn || quick || (useKey ? secret !== "" : user.trim() !== ""))
    readonly property var found: kind === "dlna" && setup.step === "address" ? setup.choices || [] : []
    signal submitted()
    spacing: Metrics.gap8

    Component.onCompleted: addressField.text = editing ? setup.address || "" : ""

    Text {
        visible: form.kind === "plex" && !form.editing
        width: parent.width
        text: I18n.tr("Nothing to type here. You need a browser where you can sign in to your Plex account.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }

    // A media server found on this network, chosen by a click.
    Column {
        visible: form.kind === "dlna"
        width: parent.width
        spacing: Metrics.gap4
        Item {
            width: parent.width
            height: again.height
            Text {
                anchors.left: parent.left
                anchors.right: again.left
                anchors.verticalCenter: parent.verticalCenter
                // An edit searches only when asked: until then nothing is said found.
                text: form.setup.searching === true ? I18n.tr("Searching this network…", I18n.rev)
                    : form.found.length > 0 || !form.editing ? I18n.tr("Found on this network", I18n.rev) : ""
                elide: Text.ElideRight
                font.pointSize: Type.small
                font.weight: Type.semibold
                color: Theme.color.textSecondary
            }
            PillButton {
                id: again
                objectName: "serverSearchAgain"
                anchors.right: parent.right
                kind: "ghost"
                glyph: Icons.refresh
                text: form.editing && form.found.length === 0 && form.setup.searching !== true
                    ? I18n.tr("Search this network", I18n.rev) : I18n.tr("Search again", I18n.rev)
                enabled: !form.preview && form.setup.busy !== true && form.setup.step === "address"
                onClicked: Servers.begin("search")
            }
        }
        Repeater {
            model: form.found
            Rectangle {
                id: one
                required property var modelData
                required property int index
                objectName: "serverFound_" + index
                readonly property bool chosen: addressField.text === modelData.id
                width: form.width
                height: Math.max(Metrics.menuItemRich, lines.implicitHeight + Metrics.gap8 * 2) // derived: padding above and below
                radius: Metrics.radiusControl
                color: chosen ? Theme.color.rowSelected : pointer.containsMouse ? Theme.color.stateHover : Theme.color.fillField
                SourceIcon {
                    id: icon
                    x: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    width: Metrics.iconNav
                    height: width
                    sourceInfo: ({icon: "dlna"})
                }
                Column {
                    id: lines
                    anchors.left: icon.right
                    anchors.leftMargin: Metrics.gap10
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    Text { width: parent.width; text: one.modelData.name; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.pointSize: Type.body; color: Theme.color.textPrimary }
                    Text { width: parent.width; text: one.modelData.detail; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.pointSize: Type.small; color: Theme.color.textSecondary }
                }
                MouseArea {
                    id: pointer
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: addressField.text = one.modelData.id
                }
            }
        }
        Text {
            visible: form.setup.searching !== true && form.found.length === 0 && !form.editing
            width: parent.width
            text: I18n.tr("No media server answered the search. Some do not answer searches: type its address below, such as 192.168.0.20:8200.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }

    Text {
        visible: addressField.visible
        width: parent.width
        topPadding: form.kind === "dlna" ? Metrics.gap8 : 0
        text: form.kind === "dlna" ? I18n.tr("Address of the media server, or of its description", I18n.rev) : I18n.tr("Server address", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    TextField {
        id: addressField
        objectName: "serverAddress"
        visible: form.kind !== "plex" || form.editing
        width: parent.width
        placeholder: form.kind === "dlna" ? "192.168.0.20:8200" : form.kind === "jellyfin" ? "http://music.lan:8096" : "http://music.lan:4533"
        onAccepted: form.submitted()
    }
    CircleCheck {
        objectName: "serverChangeSignIn"
        visible: form.editing && form.kind !== "dlna"
        width: parent.width
        checkable: false
        checked: form.changeSignIn
        text: form.kind === "plex" ? I18n.tr("Sign in to Plex again", I18n.rev) : I18n.tr("Change the sign-in", I18n.rev)
        onClicked: form.changeSignIn = !form.changeSignIn
    }
    CircleCheck {
        objectName: "serverUseKey"
        visible: form.asksSignIn && form.kind === "subsonic"
        width: parent.width
        checkable: false
        checked: form.useKey
        text: I18n.tr("Sign in with an API key instead (OpenSubsonic servers that offer one)", I18n.rev)
        onClicked: form.useKey = !form.useKey
    }
    CircleCheck {
        objectName: "serverQuick"
        visible: form.asksSignIn && form.kind === "jellyfin"
        width: parent.width
        checkable: false
        checked: form.quick
        text: I18n.tr("Sign in with Quick Connect: approve a code in a Jellyfin app where you are signed in", I18n.rev)
        onClicked: form.quick = !form.quick
    }
    TextField {
        id: userField
        objectName: "serverUser"
        visible: form.asksSignIn && !form.useKey && !form.quick
        width: parent.width
        placeholder: I18n.tr("User", I18n.rev)
        onAccepted: form.submitted()
    }
    TextField {
        id: secretField
        objectName: "serverSecret"
        visible: form.asksSignIn && !form.quick
        width: parent.width
        secret: true
        placeholder: form.useKey ? I18n.tr("API key", I18n.rev) : I18n.tr("Password", I18n.rev)
        onAccepted: form.submitted()
    }
}

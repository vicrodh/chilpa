import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The test of a server's connection: what is being done, a code to type
// elsewhere, the server to choose when an account reaches several, and the
// result in plain words. A failure says what the server answered and what
// to do about it.
Column {
    id: check
    property string kind: ""
    property var setup: ({})
    property bool connected: false
    property bool failed: false
    property string error: ""
    readonly property string step: setup.step || ""
    readonly property string said: error === "" ? "" : error.charAt(0).toUpperCase() + error.slice(1)
    spacing: Metrics.gap12

    // While the server is asked.
    Row {
        visible: !check.connected && !check.failed && check.step !== "code" && check.step !== "servers"
        width: parent.width
        spacing: Metrics.gap8
        Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.refresh; size: Metrics.iconMd; color: Theme.color.textTertiary }
        Text {
            objectName: "serverProgress"
            width: parent.width - Metrics.iconMd - parent.spacing
            anchors.verticalCenter: parent.verticalCenter
            text: check.kind === "dlna" ? I18n.tr("Reading the media server's description and its folders…", I18n.rev)
                : check.step === "signin" ? I18n.tr("Signing in and reading its libraries…", I18n.rev)
                : check.kind === "plex" ? I18n.tr("Asking Plex for a code…", I18n.rev)
                : I18n.tr("Reaching the server…", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
    }

    // A code to type elsewhere: Plex's PIN, Jellyfin's Quick Connect.
    Column {
        visible: check.step === "code" && !check.failed
        width: parent.width
        spacing: Metrics.gap8
        Text {
            width: parent.width
            text: (check.setup.link || "") !== "" ? I18n.tr("Open plex.tv/link in a browser, signed in to your Plex account, and type this code. Chilpa notices by itself.", I18n.rev)
                : I18n.tr("In a Jellyfin app where you are signed in, open Quick Connect and type this code. Chilpa notices by itself.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Text {
            objectName: "serverCode"
            text: check.setup.code || ""
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.labelTracking
            color: Theme.color.textPrimary
        }
        PillButton {
            visible: (check.setup.link || "") !== ""
            text: I18n.tr("Open plex.tv/link", I18n.rev)
            glyph: Icons.externalLink
            onClicked: Qt.openUrlExternally(check.setup.link)
        }
    }

    // An account that reaches several servers chooses one.
    Repeater {
        model: check.step === "servers" && !check.failed ? check.setup.choices || [] : []
        PillButton {
            required property var modelData
            required property int index
            objectName: "serverChoice_" + index
            text: modelData.detail ? modelData.name + " · " + modelData.detail : modelData.name
            enabled: check.setup.busy !== true
            onClicked: Servers.pick(modelData.id)
        }
    }

    // The result.
    Row {
        visible: check.connected || check.failed
        width: parent.width
        spacing: Metrics.gap12
        Icon {
            id: verdict
            glyph: check.connected ? Icons.circleCheckFilled : Icons.alertCircle
            filled: check.connected
            size: Metrics.iconPlay
            color: check.connected ? Theme.color.statusPositive : Theme.color.statusNegative
        }
        Column {
            width: parent.width - verdict.width - parent.spacing
            spacing: Metrics.gap4
            Text {
                objectName: "serverResult"
                width: parent.width
                text: check.connected ? I18n.trArgs("Connected to {}", [check.setup.label || check.setup.product || check.setup.address || ""], I18n.rev)
                    : check.said
                wrapMode: Text.WordWrap
                font.pointSize: Type.bodyLarge
                font.weight: Type.semibold
                color: Theme.color.textPrimary
            }
            Text {
                visible: check.connected && text !== ""
                width: parent.width
                text: [check.setup.product || "", check.setup.address || ""].filter(Boolean).join(" · ")
                wrapMode: Text.WrapAnywhere
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            Text {
                objectName: "serverAdvice"
                width: parent.width
                text: check.connected ? (check.kind === "dlna"
                        ? I18n.trCount("It shows {} folder at its top. Next, open the one that holds your music.", "It shows {} folders at its top. Next, open the one that holds your music.", (check.setup.choices || []).length, I18n.rev)
                        : I18n.trCount("It offers {} music library.", "It offers {} music libraries.", (check.setup.choices || []).length, I18n.rev))
                    : check.step === "signin" ? I18n.tr("Check the user and the password or the API key, then go back and try again.", I18n.rev)
                    : check.step === "code" ? I18n.tr("Try again to get a new code.", I18n.rev)
                    : check.kind === "dlna" ? I18n.tr("Check the address, that DLNA is on in the server's settings and that it lets this computer in, then try again.", I18n.rev)
                    : I18n.tr("Check the address and its port, and that the server is on and reachable from this computer, then try again.", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
        }
    }
}

import QtQuick
import QtQuick.Controls.Basic as Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: dialog
    objectName: "collectionSizeDialog"
    property string action
    property string title
    property bool shuffled
    property bool working: false
    closePolicy: working ? Basic.Popup.NoAutoClose : Basic.Popup.CloseOnEscape | Basic.Popup.CloseOnReleaseOutside
    signal chosen(int limit)

    Loader {
        id: body
        width: parent.width
        active: dialog.visible
        sourceComponent: Column {
            width: body.width
            spacing: Metrics.gap12

            Text {
                width: parent.width
                text: dialog.title
                textFormat: Text.PlainText
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Text {
                width: parent.width
                text: dialog.shuffled
                    ? I18n.tr("How many tracks should be shuffled from the whole collection?", I18n.rev)
                    : I18n.tr("How many tracks should be added to the queue?", I18n.rev)
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
            SelectField {
                id: size
                objectName: "collectionSizeSelect"
                width: parent.width
                value: "50"
                values: ["50", "100", "200", "500", "1000", "0", "custom"]
                options: values.map(value => {
                    if (value === "0") return I18n.tr("All tracks", I18n.rev)
                    if (value === "custom") return I18n.tr("Custom", I18n.rev)
                    return I18n.trCount("{} track", "{} tracks", Number(value), I18n.rev)
                })
                rich: true
                enabled: !dialog.working
                onPicked: (choice) => {
                    value = choice
                    if (choice === "custom") Qt.callLater(custom.focusInput)
                }
            }
            Item {
                width: parent.width
                height: custom.implicitHeight
                TextField {
                    id: custom
                    objectName: "collectionCustomSize"
                    anchors.fill: parent
                    visible: size.value === "custom"
                    enabled: visible && !dialog.working
                    placeholder: I18n.tr("Number of tracks", I18n.rev)
                    inputMethodHints: Qt.ImhDigitsOnly
                    validator: RegularExpressionValidator { regularExpression: /[0-9]{0,10}/ }
                    onAccepted: if (accept.enabled) accept.clicked()
                }
            }
            Basic.ProgressBar {
                objectName: "collectionQueueProgress"
                width: parent.width
                opacity: dialog.working ? 1 : 0
                indeterminate: dialog.working
                Accessible.name: I18n.tr("Preparing queue", I18n.rev)
                Accessible.ignored: !dialog.working
                palette.dark: Theme.color.accent
                palette.midlight: Theme.color.fillField
            }
            Flow {
                id: buttons
                width: parent.width
                spacing: Metrics.gap8
                layoutDirection: Qt.RightToLeft

                PillButton {
                    id: accept
                    objectName: "collectionSizeConfirm"
                    readonly property real limit: Number(size.value === "custom" ? custom.text : size.value)
                    maximumWidth: buttons.width
                    wrapText: true
                    kind: "primary"
                    text: dialog.title
                    enabled: !dialog.working && (size.value !== "custom" || (Number.isInteger(limit) && limit >= 1 && limit <= 2147483647))
                    onClicked: dialog.chosen(limit)
                }
                PillButton {
                    text: I18n.tr("Cancel", I18n.rev)
                    enabled: !dialog.working
                    onClicked: dialog.close()
                }
            }
        }
    }
}

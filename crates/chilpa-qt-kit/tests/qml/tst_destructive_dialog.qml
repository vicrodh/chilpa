import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "DestructiveDialog"
    when: windowShown
    visible: true
    width: 720; height: 700
    property real savedWindowHeight: 0
    property real savedWindowWidth: 0
    property int rows: 1
    property int footerClicks: 0

    ConfirmationDialog {
        id: question
        title: I18n.tr("Delete playlist?",I18n.rev)
        message: I18n.tr("This permanently removes the playlist and its saved order.",I18n.rev)
        confirmText: I18n.tr("Delete",I18n.rev)
        destructive: true
        doubleConfirmation: true
    }
    SignalSpy { id: accepted; target: question; signalName: "confirmed" }
    Modal {
        id: owner
        Column {
            width: parent.width
            ConfirmationDialog { id: nested; title: "Fixture"; message: "Fixture"; confirmText: "Continue"; destructive: true }
        }
    }
    Modal {
        id: natural
        preferredWidth: 480
        Column {
            width: parent.width
            Repeater {
                model: test.rows
                Rectangle { width: parent.width; height: 32; color: "transparent" }
            }
            PillButton { objectName: "naturalFooter"; text: "Last action"; onClicked: test.footerClicks++ }
        }
    }
    function press(popup, name) {
        const item = findChild(popup.contentItem,name)
        const viewport = popup.contentItem
        waitForRendering(viewport,100)
        const point = item.mapToItem(viewport.contentItem,0,0)
        viewport.contentY = Math.max(0,Math.min(viewport.contentHeight-viewport.height,point.y+item.height-viewport.height))
        waitForRendering(viewport,100)
        mouseClick(item)
    }
    function initTestCase() {
        savedWindowHeight = test.Window.window.height
        savedWindowWidth = test.Window.window.width
    }
    function cleanup() {
        question.close(); natural.close(); owner.close(); accepted.clear()
        question.targetValid = true
        question.doubleConfirmation = true
        test.Window.window.width = savedWindowWidth
        test.Window.window.height = savedWindowHeight
    }
    function test_two_confirmations_and_acknowledgement_precede_the_action() {
        question.open()
        tryCompare(question,"opened",true)
        const button = findChild(question.contentItem,"confirmationAccept")
        press(question,"confirmationAccept")
        compare(accepted.count,0)
        verify(question.opened)
        const acknowledgement = findChild(question.contentItem,"destructiveAcknowledgement")
        tryCompare(acknowledgement,"visible",true)
        tryCompare(button,"enabled",false)
        press(question,"confirmationAccept")
        compare(accepted.count,0)
        press(question,"destructiveAcknowledgement")
        tryCompare(button,"enabled",true)
        waitForRendering(question.contentItem)
        press(question,"confirmationAccept")
        compare(accepted.count,1)
        tryCompare(question,"visible",false)
    }
    function test_a_destructive_style_alone_requires_only_one_confirmation() {
        question.doubleConfirmation = false
        question.open()
        tryCompare(question,"opened",true)
        press(question,"confirmationAccept")
        compare(accepted.count,1)
        tryCompare(question,"visible",false)
    }
    function test_cancelling_the_final_step_and_reopening_requires_both_steps_again() {
        question.open()
        tryCompare(question,"opened",true)
        press(question,"confirmationAccept")
        press(question,"confirmationCancel")
        tryCompare(question,"visible",false)
        compare(accepted.count,0)
        question.open()
        tryCompare(question,"opened",true)
        verify(!findChild(question.contentItem,"destructiveAcknowledgement").visible)
        press(question,"confirmationAccept")
        tryCompare(findChild(question.contentItem,"confirmationAccept"),"enabled",false)
        compare(accepted.count,0)
    }
    function test_a_changed_target_cannot_be_confirmed() {
        question.open()
        tryCompare(question,"opened",true)
        press(question,"confirmationAccept")
        press(question,"destructiveAcknowledgement")
        question.targetValid = false
        const button = findChild(question.contentItem,"confirmationAccept")
        tryCompare(button,"enabled",false)
        button.clicked()
        compare(accepted.count,0)
        verify(question.opened)
    }
    function test_closing_an_owner_also_closes_its_overlay_confirmation() {
        owner.open()
        tryCompare(owner,"opened",true)
        nested.open()
        tryCompare(nested,"opened",true)
        owner.close()
        tryCompare(owner,"visible",false)
        tryCompare(nested,"visible",false)
    }
    function test_height_tracks_content_and_caps_at_eighty_percent_while_footer_stays_reachable() {
        test.Window.window.width = 360
        test.Window.window.height = 320
        rows = 1; footerClicks = 0
        natural.open()
        tryCompare(natural,"opened",true)
        const small = natural.height
        rows = 40
        tryVerify(() => natural.height > small)
        tryVerify(() => natural.height <= test.Window.window.height * 0.8 + 0.5)
        verify(natural.width <= test.Window.window.width)
        const viewport = natural.contentItem
        tryVerify(() => viewport.contentHeight > viewport.height)
        viewport.contentY = viewport.contentHeight - viewport.height
        waitForRendering(viewport,100)
        mouseClick(findChild(natural.contentItem,"naturalFooter"))
        compare(footerClicks,1)
        test.Window.window.height = 600
        tryVerify(() => natural.height > small && natural.height <= test.Window.window.height * 0.8 + 0.5)
        rows = 1
        tryVerify(() => Math.abs(natural.height - small) < 0.5)
    }
}

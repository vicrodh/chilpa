import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Components"
import "../../qml/Patterns"

TestCase {
    id: test
    name: "Controls"
    when: windowShown
    visible: true
    width: 1100
    height: 700

    WindowControls { id: controls; window: test.Window.window }
    SleepTimer { id: timer }
    LoopRow { id: loop; width: 320 }
    TextInput { id: field; y: 60; width: 100; height: 20 }
    Item { id: plain; y: 90; width: 100; height: 20 }
    MouseArea { id: covered; x: 200; y: 100; width: 120; height: 120; acceptedButtons: Qt.AllButtons; onClicked: test.coveredClicks++ }
    Flickable { id: wheelArea; x: 350; y: 100; width: 120; height: 120; contentWidth: width; contentHeight: 600 }
    property int coveredClicks: 0

    readonly property string sheet: JSON.stringify({
        col1: [{ label: "Playback", rows: [{ id: "playback.toggle", label: "Play / Pause", shortcut: "Space", modified: false },
            { id: "playback.mute", label: "Mute / Unmute", shortcut: "X", modified: true }] }],
        col2: [{ label: "Navigation", rows: [{ id: "nav.back", label: "Go Back", shortcut: "Alt + ◀", modified: false }] }],
        col3: [], keymap: "letters", modified: 1, recording: "", tried: "", takenBy: "" })

    function init() {
        Navigation.backCount = 0
        Navigation.forwardCount = 0
        coveredClicks = 0
        Hotkeys.snapshot = sheet
        Hotkeys.answer = ""
        Hotkeys.commands = []
        Hotkeys.liked = 0
        Controls.sleepLeft = ""
        Controls.loopState = 0
        Controls.asked = -1
        Player.lastAction = ""
        plain.forceActiveFocus()
    }

    function test_side_buttons_work_over_an_active_mouse_target() {
        mouseClick(covered, 10, 10, Qt.BackButton)
        compare(Navigation.backCount, 1)
        compare(coveredClicks, 0)
        mouseClick(covered, 10, 10, Qt.ForwardButton)
        compare(Navigation.forwardCount, 1)
        mouseClick(covered, 10, 10, Qt.LeftButton)
        compare(coveredClicks, 1)
    }
    function test_side_button_layer_leaves_wheel_scrolling_to_the_page() {
        wheelArea.contentY = 0
        mouseWheel(wheelArea, 20, 20, 0, -120)
        tryVerify(() => wheelArea.contentY > 0)
    }

    // Open and drawn: a popup laid out again is clicked where it now is.
    function shown(popup) {
        tryVerify(() => popup.opened)
        waitForRendering(popup.contentItem)
    }

    function cleanup() {
        for (const name of ["shortcutsSheet", "shortcutsEditor"])
            findChild(controls, name).close()
        timer.close()
        tryVerify(() => !timer.opened && !findChild(controls, "shortcutsSheet").opened && !findChild(controls, "shortcutsEditor").opened)
    }

    function test_a_key_that_is_an_action_is_taken_and_run() {
        verify(controls.listening, "the window's keys are heard")
        SettingsModel.sidebarMode = "open"
        Hotkeys.answer = "ui.sidebar"
        keyClick(Qt.Key_S, Qt.ShiftModifier)
        compare(SettingsModel.sidebarMode, "collapsed", "its action ran")
        keyClick(Qt.Key_S, Qt.ShiftModifier)
        compare(SettingsModel.sidebarMode, "closed")
        Hotkeys.answer = "-"
        keyClick(Qt.Key_S, Qt.ShiftModifier)
        compare(SettingsModel.sidebarMode, "closed", "taken with nothing to run")
    }

    function test_a_key_that_is_nobodys_goes_on() {
        field.forceActiveFocus()
        field.text = ""
        Hotkeys.answer = ""
        keyClick(Qt.Key_Q)
        compare(field.text, "q", "typed where the keyboard was")
        verify(controls.typing())
        plain.forceActiveFocus()
        verify(!controls.typing())
    }

    function test_tab_keys_and_queue_keys_switch_the_requested_workspace() {
        Navigation.stepCount = 0
        Hotkeys.answer = "nav.nextTab"
        keyClick(Qt.Key_Tab, Qt.ControlModifier)
        compare(Navigation.stepCount, 1)
        Hotkeys.answer = "nav.previousTab"
        keyClick(Qt.Key_Tab, Qt.ControlModifier | Qt.ShiftModifier)
        compare(Navigation.stepCount, 0)
        Hotkeys.answer = "queue.next"
        keyClick(Qt.Key_Right, Qt.ControlModifier | Qt.AltModifier)
        compare(ListenList.lastAction, "next")
        compare(Navigation.page, "listenlist")
        Hotkeys.answer = "queue.previous"
        keyClick(Qt.Key_Left, Qt.ControlModifier | Qt.AltModifier)
        compare(ListenList.lastAction, "previous")
    }

    function test_modals_keep_tab_and_queue_shortcuts_away_from_the_background() {
        timer.open(); shown(timer)
        Navigation.stepCount = 0; ListenList.lastAction = ""
        plain.forceActiveFocus()
        Hotkeys.answer = "nav.nextTab"
        keyClick(Qt.Key_Tab, Qt.ControlModifier)
        compare(Navigation.stepCount, 0)
        Hotkeys.answer = "queue.next"
        keyClick(Qt.Key_Right, Qt.ControlModifier | Qt.AltModifier)
        compare(ListenList.lastAction, "")
        verify(timer.opened)
    }

    function test_modals_block_side_buttons_above_and_outside_the_panel() {
        timer.open(); shown(timer)
        mouseClick(timer.contentItem, 10, 10, Qt.BackButton)
        mouseClick(covered, 10, 10, Qt.ForwardButton)
        compare(Navigation.backCount, 0)
        compare(Navigation.forwardCount, 0)
        compare(coveredClicks, 0)
    }

    function test_the_search_box_is_asked_through_the_shortcuts() {
        let asked = 0
        const count = () => ++asked
        Hotkeys.searchAsked.connect(count)
        controls.run("nav.search")
        Hotkeys.searchAsked.disconnect(count)
        compare(asked, 1)
    }

    function test_liking_what_plays() {
        controls.run("playback.favorite")
        compare(Hotkeys.liked, 1)
    }

    // The app menu's rows that are keyboard actions run them without the key.
    function test_an_action_asked_without_its_key_runs() {
        Hotkeys.ask("ui.showShortcuts")
        shown(findChild(controls, "shortcutsSheet"))
    }

    function test_the_sheet_opens_and_leads_to_the_editor() {
        controls.run("ui.showShortcuts")
        const sheet = findChild(controls, "shortcutsSheet")
        shown(sheet)
        const caps = []
        const collect = (item) => { for (const child of item.children) { if (child.objectName === "keycap") caps.push(child.text); collect(child) } }
        collect(sheet.contentItem)
        compare(caps.sort(), ["Alt + ◀", "Space", "X"], "every action with its key")
        mouseClick(findChild(sheet.contentItem, "customize"))
        const editor = findChild(controls, "shortcutsEditor")
        shown(editor)
        verify(!sheet.opened)
        compare(findChild(editor.contentItem, "modified").text, "1 modified")
        compare(findChild(editor.contentItem, "keymap").current, 1, "the keymap chosen is the one lit")
        editor.close()
        tryVerify(() => !editor.opened)
        verify(Hotkeys.commands.indexOf("cancel:") >= 0, "closing it lets a recording go")
    }

    function test_the_editor_records_and_says_who_has_a_key() {
        const editor = findChild(controls, "shortcutsEditor")
        editor.open()
        shown(editor)
        const cap = findChild(editor.contentItem, "keycap")
        mouseClick(cap)
        compare(Hotkeys.commands[Hotkeys.commands.length - 1], "record:playback.toggle")
        const doc = JSON.parse(sheet)
        doc.recording = "playback.toggle"
        Hotkeys.snapshot = JSON.stringify(doc)
        tryCompare(findChild(editor.contentItem, "keycap"), "text", "Press keys…")
        verify(editor.recording)
        doc.tried = "Q"
        doc.takenBy = "Queue"
        Hotkeys.snapshot = JSON.stringify(doc)
        tryCompare(findChild(editor.contentItem, "keycap"), "text", "Q")
        const conflict = findChild(editor.contentItem, "conflict")
        verify(conflict.visible)
        compare(conflict.text, "Already used by Queue")
        mouseClick(findChild(editor.contentItem, "resetAll"))
        compare(Hotkeys.commands[Hotkeys.commands.length - 1], "reset-all:")
        editor.close()
        tryVerify(() => !editor.opened)
    }

    function test_the_timer_is_set_from_what_is_offered_or_typed() {
        timer.open()
        shown(timer)
        timer.chosen = 2
        mouseClick(findChild(timer.contentItem, "sleepSet"))
        compare(Controls.asked, 120)
        tryVerify(() => !timer.opened)
        Controls.sleepLeft = ""
        timer.open()
        shown(timer)
        timer.chosen = timer.minutes.length
        findChild(timer.contentItem, "sleepMinutes").text = "45"
        timer.set()
        compare(Controls.asked, 45)
        timer.chosen = timer.minutes.length
        findChild(timer.contentItem, "sleepMinutes").text = "soon"
        Controls.asked = -1
        timer.set()
        compare(Controls.asked, -1, "what is not a number sets nothing")
    }

    function test_a_running_timer_says_what_is_left_and_is_called_off() {
        Controls.sleepLeft = "29m"
        timer.open()
        shown(timer)
        compare(findChild(timer.contentItem, "sleepLeft").text, "29m")
        verify(!findChild(timer.contentItem, "sleepSet").visible)
        mouseClick(findChild(timer.contentItem, "sleepCancel"))
        compare(Controls.asked, 0)
        tryVerify(() => !timer.opened)
    }

    function test_the_loop_is_marked_twice_and_let_go() {
        const mark = findChild(loop, "loopMark")
        const clear = findChild(loop, "loopClear")
        compare(mark.text, "Set loop start (A)")
        verify(!clear.visible)
        mouseClick(mark)
        compare(mark.text, "Set loop end (B)")
        verify(clear.visible)
        mouseClick(mark)
        verify(!mark.visible, "looping, there is only letting it go")
        waitForRendering(loop)
        mouseClick(clear)
        compare(Controls.loopState, 0)
    }
}

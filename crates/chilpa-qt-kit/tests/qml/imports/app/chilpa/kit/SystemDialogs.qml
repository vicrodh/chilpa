pragma Singleton
import QtQuick

// The system's file dialogs, answered at once with what a test sets, and
// what each was asked.
QtObject {
    property string answer: "/music/Chosen"
    property var asked: []
    signal chosen(string asker, string path)
    function ask(kind, asker, title, extra) { asked = asked.concat([{kind: kind, asker: asker, title: title, extra: extra}]); if (answer !== "") chosen(asker, answer) }
    function pickFile(asker, title, filters) { ask("file", asker, title, filters) }
    function pickFolder(asker, title, from) { ask("folder", asker, title, from) }
    function saveFile(asker, title, suggested, filters) { ask("save", asker, title, suggested) }
}

pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    function tr(value, revision) { return value }
    function trArgs(value, args, revision) { return args.reduce((text, arg) => text.replace("{}", arg), value) }
    function trCount(one, many, count, revision) { return (count === 1 ? one : many).replace("{}", count) }
}

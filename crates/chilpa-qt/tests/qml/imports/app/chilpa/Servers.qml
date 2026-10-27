pragma Singleton
import QtQuick
QtObject {
    property string setup: '{"step":"idle"}'
    property var calls: []
    property int refreshes: 0
    function show(doc) { setup = JSON.stringify(doc) }
    function begin(kind) {
        calls.push(["begin", kind])
        show({step:"address",kind:kind.startsWith("edit/")?"jellyfin":kind,label:"Saved server",address:"http://music.local",editing:kind.startsWith("edit/")})
    }
    function cancel() { calls.push(["cancel"]); show({step:"idle"}) }
    function address(value) { calls.push(["address",value]) }
    function signIn(user, secret, key) { calls.push(["signIn",user,secret,key]) }
    function finish(value) { calls.push(["finish",JSON.parse(value)]); return true }
    function refresh() { refreshes++ }
    function quickConnect() { calls.push(["quickConnect"]) }
    function pick(id) { calls.push(["pick",id]) }
}

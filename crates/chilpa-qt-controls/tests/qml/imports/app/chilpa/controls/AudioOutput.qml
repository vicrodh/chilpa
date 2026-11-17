pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property string tab: "equipment"
    property bool shown: false
    property int reads: 0
    property var commands: []
    property string channelDocument: "{}"
    property bool channelValid: true
    property var channelAnswers: []
    signal channelOfferChanged()
    function channelOffer() { return channelDocument }
    function channelCurrent(token) { return channelValid && JSON.parse(channelDocument).token === token }
    function channelAnswer(token,accept,remember) { channelAnswers=channelAnswers.concat([[token,accept,remember]]);channelDocument="{}";channelOfferChanged();return true }
    signal outputSelected()
    signal outputsListed()
    property bool watching: false
    function watchOutputs(shown) { watching = shown }
    property string pathAnswer: "{}"
    property int pathReads: 0
    function signalPath() { pathReads++; return pathAnswer }
    function show(page) { tab = page; shown = true }
    function close() { shown = false; snapshot = "{}" }
    function command(action, argument) { commands = commands.concat([{action:action, argument:argument}]) }
    function refresh() { reads++ }
    function takeRequest() {}
}

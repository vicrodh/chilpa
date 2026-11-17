import QtQuick
import QtTest
import app.chilpa.controls
import app.chilpa.kit
import "../../qml/Patterns"
TestCase {
    id:test
    name:"ChannelOffer"
    when:windowShown
    width:1000;height:800
    visible:true
    Component { id:factory;ChannelOffer {} }
    function offer(token) { AudioOutput.channelDocument=JSON.stringify({token:token,message:"This track has six channels."});AudioOutput.channelOfferChanged() }
    function init() { I18n.choose("en");failOnWarning(/Binding loop detected|Cannot assign|is not a function|ReferenceError|TypeError/);AudioOutput.channelDocument="{}";AudioOutput.channelValid=true;AudioOutput.channelAnswers=[] }
    function cleanup() { I18n.choose("en");AudioOutput.channelDocument="{}";AudioOutput.channelOfferChanged();wait(100) }
    function question(host) { return findChild(host,"channelConversionOffer") }
    function accept(host) {
        let button=null
        tryVerify(()=>{button=findChild(question(host),"confirmationAccept");return button && button.visible && button.enabled && button.Window.window?.visible === true})
        mouseClick(button,button.width/2,button.height/2)
    }
    function test_acceptance_carries_the_optional_output_memory() {
        const host=createTemporaryObject(factory,test);offer("1");tryCompare(question(host),"opened",true)
        const option=findChild(question(host),"confirmOption");mouseClick(option,option.width/2,option.height/2)
        verify(question(host).optionChecked);accept(host)
        tryCompare(AudioOutput,"channelAnswers",[["1",true,true]])
        tryCompare(question(host),"visible",false)
        offer("2");tryCompare(question(host),"opened",true);compare(question(host).optionChecked,false)
        keyClick(Qt.Key_Escape);tryCompare(AudioOutput,"channelAnswers",[["1",true,true],["2",false,false]])
    }
    function test_replacement_releases_the_old_press_before_presenting_a_new_choice() {
        const host=createTemporaryObject(factory,test);offer("1");tryCompare(question(host),"opened",true)
        const old=findChild(question(host),"confirmationAccept"),point=old.mapToItem(test,old.width/2,old.height/2)
        mousePress(test,point.x,point.y)
        offer("2");tryCompare(host,"document",{token:"2",message:"This track has six channels."});tryCompare(question(host),"opened",true)
        mouseRelease(test,point.x,point.y);compare(AudioOutput.channelAnswers,[])
        accept(host);tryCompare(AudioOutput,"channelAnswers",[["2",true,false]])
    }
    function test_inactive_host_keeps_the_choice_without_a_hidden_dialog() {
        const host=createTemporaryObject(factory,test);offer("1");tryCompare(question(host),"opened",true)
        host.active=false;tryCompare(question(host),"visible",false);compare(AudioOutput.channelAnswers,[])
        host.active=true;tryCompare(question(host),"opened",true);accept(host)
        tryCompare(AudioOutput,"channelAnswers",[["1",true,false]])
    }
    function test_changed_target_cannot_confirm() {
        const host=createTemporaryObject(factory,test);offer("1");tryCompare(question(host),"opened",true)
        AudioOutput.channelValid=false;accept(host);tryCompare(question(host),"invalidated",true);compare(AudioOutput.channelAnswers,[])
    }
    function test_translation_changes_preserve_the_current_choice() {
        const host=createTemporaryObject(factory,test);offer("1");tryCompare(question(host),"opened",true)
        question(host).optionChecked=true
        I18n.choose("es")
        tryVerify(()=>question(host).title !== "Play in stereo?")
        compare(question(host).confirmText,I18n.tr("Play in stereo",I18n.rev))
        verify(question(host).optionChecked);compare(host.document.token,"1")
        accept(host);tryCompare(AudioOutput,"channelAnswers",[["1",true,true]])
    }

}

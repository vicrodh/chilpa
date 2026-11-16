import QtQuick
import QtTest
import "../../qml/Primitives"

TestCase {
    name:"MultilineFieldInput"; when:windowShown; visible:true; width:400; height:200
    MultilineField { id:field; width:300; height:120; maximumLength:40 }
    function test_multiple_values_stay_plain_and_bounded() {
        field.text="Iron Maiden\nJudas Priest"
        compare(field.text,"Iron Maiden\nJudas Priest")
        field.text="<b>Rock</b>\nHeavy Metal"
        compare(field.text,"<b>Rock</b>\nHeavy Metal")
        field.text="x".repeat(100)
        compare(field.text.length,40)
    }
}

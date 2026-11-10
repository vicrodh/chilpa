import QtQuick
import QtTest
import app.chilpa.about
import "../../qml/Components"

TestCase {
    id: test
    name: "AboutCredits"
    when: windowShown
    visible: true
    width: 600
    height: 300
    AboutCredits { id: credits; width: 240; doc: JSON.parse(About.doc) }
    SignalSpy { id: requests; target: credits; signalName: "profileRequested" }
    function cleanup() { requests.clear() }
    function test_profiles_wrap_and_support_pointer_and_keyboard() {
        const author = findChild(credits, "aboutAuthor")
        compare(author.handle, "vicrodh")
        mouseClick(author)
        compare(requests.count, 1)
        compare(requests.signalArguments[0][0], "https://github.com/vicrodh")
        const flow = findChild(credits, "aboutContributorFlow")
        const people = flow.children.filter(item => item.objectName === "aboutContributor")
        compare(people.map(item => item.handle).sort(), ["TheMelmacian", "afonsojorgeramos", "jakumule"].sort())
        verify(people[people.length - 1].y > people[0].y, "narrow layouts wrap the profiles")
        for (const person of people) {
            verify(person.x + person.width <= flow.width)
            const avatar = findChild(person, "creditAvatar")
            tryCompare(avatar, "status", Image.Ready)
            compare(avatar.sourceSize.width, 40)
            person.forceActiveFocus()
            keyClick(Qt.Key_Return)
            compare(requests.signalArguments[requests.count - 1][0], person.url)
        }
        compare(requests.count, 4)
    }
}

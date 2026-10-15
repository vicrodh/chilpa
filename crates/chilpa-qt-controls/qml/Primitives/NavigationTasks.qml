import QtQuick

// I run navigation work only after its destination acknowledgement frame.
Item {
    id: tasks
    property var router: null
    property var queued: null
    signal prepared()

    function deferNavigation(action) {
        if (!router) { action(); return }
        router.request(true)
        queued = {generation: router.generation, destination: router.route, action: action}
    }
    Connections {
        target: tasks.router
        function onGenerationChanged() { tasks.queued = null }
        function onPreparingRoute() {
            const pending = tasks.queued
            tasks.queued = null
            try {
                if (pending && pending.generation === tasks.router.generation && pending.destination === tasks.router.route)
                    pending.action()
            } finally {
                tasks.prepared()
            }
        }
    }
}

// Once the event loop has returned and the engine is gone, the deletes the
// engine left are still queued. Qt delivers them only when asked for them by
// type; left to exit(), they ran after the Quick theme and its palettes were
// destroyed, and a control's destructor read a freed palette.

#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>

extern "C" void chilpa_delete_what_is_left()
{
    // A delete can queue another, and a pass leaves what is posted during it
    // for the next one. Whatever a few passes do not reach, the application
    // object's destructor drops without delivering.
    for (int pass = 0; pass < 8; ++pass)
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
}

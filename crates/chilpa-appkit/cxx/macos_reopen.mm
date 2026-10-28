#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>

// Only the Dock's and the Finder's reopen event is handled here. An ordinary
// activation, such as a click on the menu-bar item, must not undo a hide.
// Qt's own application delegate stays: it owns quitting, the menus and the
// opening of files.
@interface ChilpaReopenHandler : NSObject
@property(nonatomic, assign) void (*presentWindow)(void);
- (void)reopen:(NSAppleEventDescriptor *)event reply:(NSAppleEventDescriptor *)reply;
- (void)installAfterLaunch:(NSNotification *)notification;
@end

@implementation ChilpaReopenHandler
- (void)installAfterLaunch:(NSNotification *)notification
{
    (void)notification;
    // AppKit installs its defaults while it launches; registering after that
    // keeps the first launch from replacing this handler.
    [[NSAppleEventManager sharedAppleEventManager]
        setEventHandler:self andSelector:@selector(reopen:reply:)
        forEventClass:kCoreEventClass andEventID:kAEReopenApplication];
    [[NSNotificationCenter defaultCenter] removeObserver:self
        name:NSApplicationDidFinishLaunchingNotification object:nil];
}

- (void)reopen:(NSAppleEventDescriptor *)event reply:(NSAppleEventDescriptor *)reply
{
    (void)event;
    (void)reply;
    if (self.presentWindow)
        self.presentWindow();
}
@end

extern "C" void chilpa_install_macos_reopen_handler(void (*presentWindow)(void))
{
    // One owner for the whole life of the application. The interface can be
    // built before exec(), when AppKit has not finished launching.
    static ChilpaReopenHandler *handler = nil;
    if (handler)
        return;
    handler = [[ChilpaReopenHandler alloc] init];
    handler.presentWindow = presentWindow;
    if (NSApp.isRunning) {
        [handler installAfterLaunch:nil];
    } else {
        [[NSNotificationCenter defaultCenter] addObserver:handler
            selector:@selector(installAfterLaunch:)
            name:NSApplicationDidFinishLaunchingNotification object:nil];
    }
}

// A window-manager close request on the disposable nested X server.
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    Display *display = XOpenDisplay(NULL);
    if (!display) {
        fprintf(stderr, "Could not open display.\n");
        return 2;
    }
    Window root, parent, *children = NULL;
    unsigned int count = 0;
    if (!XQueryTree(display, DefaultRootWindow(display), &root, &parent, &children, &count)) return 1;
    int result = 1;
    for (unsigned i = 0; i < count; ++i) {
        XClassHint hint;
        if (!XGetClassHint(display, children[i], &hint)) continue;
        int found = hint.res_class && strcasecmp(hint.res_class, "LearnHarness") == 0;
        if (hint.res_name) XFree(hint.res_name);
        if (hint.res_class) XFree(hint.res_class);
        XWindowAttributes attributes;
        if (!found || !XGetWindowAttributes(display, children[i], &attributes)
                || attributes.map_state != IsViewable) continue;
        if (argc == 2 && strcmp(argv[1], "--ready") == 0) { result = 0; break; }
        XEvent event = {0};
        event.xclient.type = ClientMessage;
        event.xclient.window = children[i];
        event.xclient.message_type = XInternAtom(display, "WM_PROTOCOLS", False);
        event.xclient.format = 32;
        event.xclient.data.l[0] = XInternAtom(display, "WM_DELETE_WINDOW", False);
        event.xclient.data.l[1] = CurrentTime;
        if (XSendEvent(display, children[i], False, NoEventMask, &event)) {
            XSync(display, False);
            puts("Closed a real LearnHarness window");
            result = 0;
        }
        break;
    }
    if (children) XFree(children);
    if (result != 0) fprintf(stderr, "LearnHarness window was not visible among %u root children.\n", count);
    XCloseDisplay(display);
    return result;
}

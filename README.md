Help needed: how do I make it refresh faster?

I think it's a macOS bottleneck but not sure.

Fetching the bounds of all the applications seem to take a while, but even if I cache it it's still too slow. But caching it also defeats the whole purpose of this app.

Things I've tried:
- Polling with NSTimer at 120Hz, it was firing at ~8.3ms but the frame updates did NOT look like 120fps. Still laggy. The verbose outputs showed that it was consistently 8-13ms per call though, still not sure why it looked laggy.
- Listening to NSWindowDidMoveNotification, still extremely laggy (<10 fps). I don't think it fired frequent enough to keep up with the drag based on the # of println's I saw.
- NSWindowWillMoveNotification, same issue/result as DidMove.

These are the macOS APIs I'm using:
- CGWindowListCopyWindowInfo to get all windows and their bounds. This is the main bottleneck I think, but idk how to optimize it. I believe this takes ~10-30ms per call, but even then it should be able to do ~30fps and it did not look like that.
- AppKit for the window and views. Using NSView + CALayer for rendering the masks (shouldn't be the bottleneck right?)

Current way:
- NSWindowDidMoveNotification cancels existing timer and schedules new 300ms timer to update masks. It looks like the masks only update when the user stops dragging the window, but that doesn't solve the issue of slowness, it just avoids it by not updating during drag.

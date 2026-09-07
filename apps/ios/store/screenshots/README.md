# App Store screenshots

Empty on purpose. Nothing here is generated, and a screenshot committed
before the app's first release would be of a build nobody shipped.

Capture them with `../../scripts/screenshots.sh`, which pins the device,
the pixel size and the status bar so the only thing left to get right is
what is on the screen:

```bash
../../scripts/build-sim.sh          # once, to have an app to install
../../scripts/screenshots.sh setup
# tap the app into the state you want
../../scripts/screenshots.sh shot 01-editing
```

## What App Store Connect requires

The app targets iPhone and iPad (`TARGETED_DEVICE_FAMILY = "1,2"`), so
both sets are mandatory.

| Set | Device the script uses | Pixels |
| --- | --- | --- |
| iPhone 6.9" | iPhone 17 Pro Max | 1320 × 2868 |
| iPad 13" | iPad Pro 13-inch (M5) | 2064 × 2752 |

The simulators render at exactly those sizes, so the raw capture uploads
as-is. Do not scale one to fit the other — a rescaled screenshot is
rejected, and the error does not say that is why.

Between one and ten per set. The first is the only one most people see.

## What to show

Not the empty state. On a 6.9-inch phone it is an icon and a button in a
field of grey, and it says nothing about what the app does. Lead with a
real document being edited.

- `01-editing` — a document open, annotation visible
- `02-pages` — the page manager, thumbnails showing
- `03-forms` — a form being filled, or a signature placed
- `04-tools` — the tool palette open
- `iap-review` — the account panel with both credit packs

That last one is not a listing screenshot. App Store Connect asks for a
review screenshot *per in-app purchase*, showing where it is bought; one
image of the credits panel serves for both packs.

## Two traps

**A back-chip in the status bar.** Launching the app from another app —
including anything `simctl openurl` does — leaves a "◀ Preview" chip
next to the clock, and it is small enough to miss. Relaunch from the
home screen, or reboot the simulator, before capturing.

**Documents opened from Files go to Preview.** OpenPdfEdit registers as
a PDF editor but is not the system default, so `simctl openurl file://…`
opens Apple's Preview instead. Use the app's own Open button. This is
also worth knowing before reading the review notes: the share-sheet
route a reviewer is asked to try is a *share sheet*, not a double-tap in
Files.

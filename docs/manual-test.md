# Manual hardware test checklist

CI runners have no real mouse, so these checks run by hand on a Windows PC with a release-candidate build. Fill in one results table per build.

**Setup:** download the release zip, check its SHA256 against `SHA256SUMS.txt`, unzip to a new folder and run `pollscope.exe`. Leave other programs idle.

| # | Check | Spec scenario | Expected |
|---|---|---|---|
| 1 | Launch on a clean Windows 11 install (no Visual C++ redistributable) | Self-contained executable | Window opens, no missing-DLL error |
| 2 | Start with exactly one mouse connected | One mouse | It is already selected |
| 3 | Connect a second mouse, click **Detect**, move the second mouse | Pick by moving | The second mouse becomes selected |
| 4 | Move the selected mouse | Moving the mouse | Rate, interval plot and histogram update smoothly |
| 5 | Stop moving | Mouse at rest | Rate shows "—" within about ¼ s |
| 6 | Unplug a mouse that isn't selected | Mouse unplugged while listed | It disappears from the list within 2 s |
| 7 | 10 s run, moving the whole time | Complete run | Result appears after 10 s with figures, scope statement visible |
| 8 | 10 s run, click another window midway, keep moving | Window loses focus | Result has no gap at the focus change; trustworthy |
| 9 | 30 s run, unplug the measured mouse at about 10 s | Mouse unplugged during a run | Run ends early, result says "not trustworthy … disconnected" |
| 10 | Start a run, click **Cancel** | Cancelled run | Back to idle, no result |
| 11 | 5 s run, barely moving | Too little movement | "Not enough data", no rate or deviation figures |
| 12 | 1000 Hz mouse, 10 s run | — | Median ≈ 1000 µs, rate ≈ 1000 Hz |
| 13 | Mouse at 4 kHz or 8 kHz if available, 10 s run | — | Median ≈ 250 / 125 µs, dropped reports = 0 |
| 14 | **Export JSON** and **Export CSV** of a finished run | Export JSON; No personal identifiers | Files open; JSON has every timestamp; neither contains your user name, PC name or `\\?\HID#` path |
| 15 | Watch network activity (Resource Monitor → Network) during runs and export | No network access | pollscope.exe has no network activity |

## Results

| Build | Date | Windows version | Mice tested | Passed | Notes |
|---|---|---|---|---|---|
| | | | | | |

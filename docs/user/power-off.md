# When the computer shuts down

[Português (Brasil)](pt-BR/power-off.md)

Many computers keep their USB ports powered after they shut down. A screen that
gets nothing at that moment stays frozen on the theme's last picture all night.
For each Turing rev C screen (the serial generation: 8.8", 5", 2.1" round and
others) you choose what happens instead:

| Choice | When the computer shuts down or restarts | Stored on the screen (the plan B) |
|---|---|---|
| **Leave as it is** (the default) | nothing is sent, as before | nothing; choosing it undoes what the others stored |
| **Turn the screen off** | the screen goes fully dark, backlight included | its sleep timer: 1 to 10 minutes (5 suggested) |
| **Play a video stored on the screen** | the video you chose plays in a loop | start with a video: the first one of `sd/video` |
| **Photo album from the card** | the screen restarts and, about 15 s later, shows the album | start with the album of `sd/image` |

Other screens do not offer the choice, and `bezel standby` says it is not
supported for them.

## Who carries it out

- **Bezel Studio**, while it runs (in the tray is enough). On Linux it asks the
  system (systemd-logind) to wait for it at shutdown, within the few seconds the
  system grants (5 s by default); on Windows it acts when the session ends
  (shutting down, restarting or signing out). Quitting Bezel from the tray or
  the window applies nothing.
- The command line only records the choice and stores the plan B. Without
  Bezel Studio running (only `bezel run` or its service, or the studio
  closed), nothing is sent at shutdown, and only the plan B is left: with
  **Turn the screen off**, the screen's sleep timer turns it off a few minutes
  later; a video or the album starts only when the screen itself starts again
  (the computer cuts the USB power, a power cut, a restart). While the computer
  keeps the USB powered after it shuts down, the screen stays frozen on its
  last picture until then.
- If the choice cannot be carried out (the video was deleted, the card was
  removed), the screen is turned off rather than left frozen. A screen that was
  already asleep stays as it is.
- When the computer starts again, Bezel wakes the screen and shows your live
  theme again.
- Suspending the computer (sleep) is not covered, nor signing out on Linux
  (only shutting down and restarting).

## The plan B

Choosing also stores a setting on the screen itself, for when the studio cannot
act (it was not running, the computer lost power). The screen applies it when it
starts on its own (power-up, a restart); only the sleep timer of **Turn the
screen off** also acts while the screen stays powered.

- **Turn the screen off** stores the screen's own sleep timer: it turns itself
  off after that many minutes **without anything from the computer**. While a
  theme is live, Bezel sends the screen a one-pixel update after 30 seconds
  without anything else, so that the timer does not run out. The timer also
  stops a video or picture
  the screen plays on its own (**Play on the screen** with Live off), which is
  why only this choice uses it.
- **Photo album from the card** makes the screen start with the album: the
  photos of `sd/image`, one after another, every 3 to 5 seconds. The screen sets
  the pace; there is no setting for it.
- **Play a video stored on the screen** makes the screen start with a video, but
  the screen cannot be told which file: after a restart or a power cut it plays
  the **first video of `sd/video`** on the card, not necessarily the one you
  chose.
- **Leave as it is** undoes it: the start setting goes back to what
  **Show at start…** chose in the Storage tab (or the screen's clock), and the
  timer is off.

**Show at start…** and this choice change the same start setting: the last one
you made wins, and setting the start file keeps the sleep timer. With
**Photo album from the card**, Bezel Studio stores the album's start setting
again at each shutdown, since that is how the screen restarts into the album.
See [Screen storage and video](storage-and-video.md).

Two screens of the same model share one choice, as they share Bezel's catalog.

## In the app

**Screen → Settings → When the computer shuts down**: pick one of the four;
each **?** explains it. A choice that does not apply is greyed out with the
reason: the screen is not connected, it is not a Turing rev C screen, there is
no memory card, or no video is stored. The screen must be connected to change
the choice.

Choosing opens a confirmation that says what will be stored on the screen;
nothing is sent before you confirm. **Turn the screen off** asks the minutes,
**Play a video stored on the screen** lists the videos of the internal memory
and the card, and **Photo album from the card** opens the album.

## From the command line

```bash
bezel standby show                                   # each connected screen's choice and plan B
bezel standby set off --sleep 5 --yes                # turn off at shutdown; sleep timer 5 min
bezel standby set video --file sd/video/clip.mp4 --yes
bezel standby set album --brightness 40 --yes        # the album starts at 40% brightness
bezel standby set keep --yes                         # back to the default; undoes the plan B
```

Without `--yes`, `set` only prints what it would store: nothing is sent to the
screen and nothing is recorded. `--brightness N` (0 to 100) chooses the
backlight level the screen starts with, stored with the plan B, as
`bezel storage boot --brightness` does; without it, the screen's default, about
67%. Bezel records that level too: when Bezel Studio restarts the screen into
the album at shutdown, the album starts at it. A video played at shutdown keeps
the level the screen has at that moment; the stored level applies when the
screen starts with a video on its own. `bezel standby show` says the plan B
last stored on the screen, by this choice or by `bezel storage boot`, whichever
came last, or by Bezel Studio restarting the screen into the album at shutdown
(which stores the album's start setting again, even after `bezel storage boot`).
The choice lives in Bezel's catalog (`<data>/bezel/storage`), which the app
reads at shutdown, so a choice made here while the app is open counts.

## The photo album

The album is the card's `sd/image` folder, so it needs an SD card (see
[Preparing an SD card](sd-card.md)). It lists every picture there, with a
thumbnail for those Bezel sent and by name for the others.

**Adding a photo**: JPEG, PNG, BMP or the first frame of a GIF. Bezel stands it
upright by its EXIF orientation (a phone photo arrives upright) and frames it
for the way the screen stands, horizontal or vertical: in the app, the
orientation you use for that screen; on the command line, `--orientation`
(default: the model's). **Fill** (`--fit cover`, the default) covers the screen
and crops; **Fit** (`--fit contain`) shows the whole photo with black around it.
The photo is stored turned for the panel, as a PNG of its native size (480×1920
on the 8.8"). The app shows a preview in the screen's shape before sending; a
name already in the album asks before replacing it (`--yes` on the command
line).

**Removing a photo** asks first, naming the file. Bezel never deletes anything
on its own.

```bash
bezel standby album add beach.jpg --orientation horizontal       # Fill
bezel standby album add portrait.jpg --orientation vertical --fit contain
bezel storage ls sd/image                                        # the album
bezel storage rm sd/image/beach.png --yes                        # remove a photo
```

`--name` chooses the stored name (by default, one made from the photo's).

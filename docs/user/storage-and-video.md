# Screen storage and video

[Português (Brasil)](pt-BR/storage-and-video.md)

Screens with storage (Turing rev C, and the Turing USB generation) keep pictures
and videos in their internal memory and, when they have a slot, on an SD card.
They play them on their own and can show one at power-up, without Bezel running.

The screen has four folders: `internal/image`, `internal/video`, `sd/image` and
`sd/video`. `sd` is the memory card, reached only through the screen. Bezel
never formats it; see [Preparing an SD card](sd-card.md).

## In the app

**Screen → Storage** fills the window: the internal memory and the SD card side
by side, each with how full it is and its files. A file shows a thumbnail (or an
icon for its kind), its size, and when Bezel sent it, or that Bezel did not send
it. Above the lists, **Name contains**, **Kind**, **Origin** and **Sort by**
filter and sort both sides.

- **Selecting**: click a file; Ctrl+click and Shift+click add to the selection.
  In a list, the arrows move, Space selects or unselects, Shift+arrows extend
  the selection and Ctrl+A selects everything; Delete deletes and F2 renames.
- **Send**: drag a file from the computer onto a side, or **Send a file…**.
  Bezel shows what it will do (conversion, destination, a file it replaces) and
  asks you to confirm. The progress bar goes through *Converting*, *Sending* and
  *Checking*.
- **Play on the screen** and **Stop playback**: the screen plays a stored video
  in a loop, or shows a stored picture. With **Live** on, the theme covers what
  the screen plays, so these wait until you turn Live off.
- **Delete…**: asks for confirmation, naming the file (or listing the files).
- **At start**: **Show at start…** (or a file dragged onto **At start**) picks
  the file the screen shows when it powers up; **Back to the default clock**
  undoes it. The screen remembers the choice.
- **Move to the SD card**, **Copy to the SD card** (or to the internal memory),
  **Rename…**, **Restore…**, **Cleanup assistant…**, **Associate original…** and
  **Local copies…**: see [Managing the files](#managing-the-files).

## From the command line

```bash
bezel storage info                          # used and free space
bezel storage ls                            # every stored file; or one folder: bezel storage ls sd/video
bezel storage put clip.mp4                  # convert when needed and send, with progress
bezel storage put logo.png sd/image/logo.png
bezel storage play internal/video/clip.mp4  # loop it on the screen (--once: play it once)
bezel storage stop
bezel storage rm internal/video/clip.mp4 --yes
bezel storage boot internal/video/clip.mp4 --brightness 60 --yes   # shown at power-up
bezel storage boot default --yes            # back to the built-in start screen
```

Whatever deletes, replaces or changes the power-up choice (`rm`, `put` over an
existing file, `boot`) first prints what it will do and needs `--yes`; without
it nothing reaches the screen.

`bezel storage ls` also shows, for each file Bezel sent, its state (*stored*,
or *pending*: an upload that did not finish) and whether Bezel keeps a local
copy of it. Moving, renaming, restoring and cleaning up are in
[Managing the files](#managing-the-files).

## What can be sent

- Pictures: JPEG, PNG, BMP, GIF, sent as they are.
- Videos: converted with ffmpeg to the screen's format (on the 8.8": 480×1920
  H.264 MP4 without sound), turned for the orientation you choose
  (`--orientation`, default: the video's own shape) and cropped to the screen's
  shape, never stretched. `--fps 24` lowers the frame rate. A video already in
  the right format goes as it is. ffmpeg is not included:
  [Installing ffmpeg](ffmpeg.md).
- File names: lower-case letters `a-z`, digits, `_`, `.` and `-`.
- Size: up to **25 MiB per file on Turing rev C screens** (the serial generation:
  8.8", 5", 2.1" round and others), up to 120 MB on the Turing USB
  generation. See [How large a file can be](#how-large-a-file-can-be).
- When a file does not fit, Bezel says how much is free and lists the stored
  files, largest first. It never deletes anything for you.

## How large a file can be

A Turing rev C screen keeps the whole upload in its memory before it stores it.
On the 8.8" the firmware stops reading at about 28 MiB and freezes until it is
restarted, whatever the speed, so Bezel takes at most **25 MiB per file** on
these screens and refuses a larger one before sending anything. The vendor app's
largest files are about 24.6 MiB too.

- A video Bezel converts is made to fit: from the video's length it caps the
  bitrate, so a long clip loses some quality instead of going over the limit.
- If a converted video is still too large (or ffmpeg cannot tell its length),
  nothing is sent and Bezel says so: send a shorter clip, or lower the frame
  rate with `--fps` (for example `bezel storage put clip.mp4 --fps 24`).
- A video already in the screen's format is sent as it is, so it must be under
  the limit itself; `--fps` converts it, and the conversion fits it.

The limit appears in MiB (1 MiB = 1,048,576 bytes): the app's message, for
example, reads *"The file is 30 MiB; this screen takes files of up to 25 MiB."*

## Cancelling an upload

You can cancel an upload (**Cancel upload** in the app, Ctrl+C in the terminal;
a second Ctrl+C quits at once). Part of the file may stay on the screen:

1. **Delete the incomplete file.** The app offers **Delete the incomplete file**;
   the terminal prints the command, for example
   `bezel storage rm internal/video/clip.mp4 --yes`. If the app says the screen
   stopped answering, it comes back on the next action: press **Refresh**, then
   delete the incomplete file if it is listed.
2. **Send it again.** If the next upload ends with *"the stored size differs;
   delete it and send it again"*, bytes from the cancelled upload reached it:
   delete that file and send it once more.

If the screen stops answering altogether (the upload stalls, or every command
times out), it froze: the next command restarts a Turing rev C screen on its
own, or use `bezel restart` or **Restart screen…** in the app; no replug is
needed. See [The screen froze](troubleshooting.md#the-screen-froze--stopped-responding).

## Managing the files

A screen can only list, store, delete and play its files: it cannot send one
back to the computer, rename it or move it. So Bezel keeps a copy of what it
sends, and moving, renaming and restoring send that copy again.

### Bezel's local copies

Every file Bezel sends (from **Send a file…**, `bezel storage put`, a theme's
video, a move or a restore) is also kept on the computer: the exact bytes the
screen got (a converted video's output), with a thumbnail (for a video, it
needs ffmpeg) and a record of where it went (Bezel's *catalog*). They live in
your data folder:

- Linux: `~/.local/share/bezel/storage` (or `$XDG_DATA_HOME/bezel/storage`);
- Windows: `%APPDATA%\bezel\storage`.

The same bytes on the internal memory and on the card are kept once.

**The limit.** Only copies of files you deleted through Bezel count toward it,
2 GiB by default (about 80 files of 25 MiB); above it, the oldest go first.
Copies of files still on a screen, or missing from it (a card formatted or
replaced), never go on their own, so a restore always has them.

**Clear cache.** In the app, **Local copies…** shows how many copies there are
and their size, and sets the limit; **Clear cache…** removes the copies of files
deleted through Bezel (every copy with **Also clear the copies of files still
on a screen**), after a confirmation that says how many and how much. The files
on the screens stay, and so do their entries and thumbnails, marked "no local
copy": those files cannot be moved, renamed or restored until you
[associate their original](#files-bezel-did-not-send) again. On the command
line:

```bash
bezel storage cache info                # how many copies, their size and the limit
bezel storage cache --limit 1GiB        # the limit for the copies of deleted files
bezel storage cache clear --yes         # remove the copies of deleted files
bezel storage cache clear --all --yes   # every copy, also of files still on a screen
```

`bezel storage catalog` lists what Bezel sent to the screen: each file's state
(*stored*, *pending*, *missing*, *deleted* or *on another card*), whether it has
a local copy, when it was sent and from where.

### Moving, renaming and copying

Bezel moves (internal memory to card, or back) and renames one file at a time:

1. it checks the target first: the name, the kind, the
   [25 MiB per file](#how-large-a-file-can-be) of Turing rev C screens and the
   free space;
2. it sends the file again from its local copy;
3. it checks that the size the screen stored is the copy's size;
4. only then does it delete the source.

The source is never deleted first, not even when space is short. If a file
fails, or you **Cancel** (Ctrl+C in the terminal), the source stays where it
was and the next files do not start; the report says what was moved, what
failed and why, and what did not start, and an incomplete file the cancelled
upload left is named with the command that deletes it.

The new name follows the sending rule (lower-case letters, digits, `_`, `.` and
`-`, the same extension): moving `NVI.mp4` gives `nvi.mp4`. A file of that name
already there is left out unless you choose to replace it. Moving the file the
screen shows at start, or renaming a video a theme plays, adds a warning to the
confirmation: the screen starts with the last file it played, and a theme finds
its video by its name.

In the app, select files in one list and press **Move to the SD card** (or
**Move to the internal memory**), or drag them onto the other list. **Copy to
the SD card** sends them the same way and keeps the originals. **Rename…** (F2)
asks the new name and shows it as the screen will store it. One confirmation
lists every file as source → target with its size, what is left out and why
(with **Replace the “…” there** for a file of the same name) and the free space;
nothing starts before you press **Move**. While it runs, the bar shows *Moving
“…” (1 of 3)* and **Cancel**.

On the command line, `mv`, `rename` and `restore` first print the exact list;
without `--yes` they only ask the screen and change nothing:

```bash
bezel storage mv internal/video/clip.mp4 --to sd          # prints the list; --yes moves
bezel storage mv sd/video/a.mp4 sd/video/b.mp4 --to internal --yes
bezel storage rename sd/video/clip.mp4 intro.mp4 --yes
```

```text
$ bezel storage mv internal/video/bezel_demo.mp4 --to sd
Move 1 file to the memory card of Turing Smart Screen 8.8", each sent from Bezel's local copy:
  internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4     2.3 MiB
1 file, 2.3 MiB to send; each source is deleted only after its copy is verified.
Nothing on the screen was changed. Add --yes to move it.
```

`--overwrite` replaces the files of the same name already there. The command
line has no copy command: `bezel storage restore internal intro.mp4 --yes` sends
a file that is on the card to the internal memory too, and keeps the card's.

### Restoring

Restore sends files Bezel sent before back to a medium from their local copies:
after you formatted the card on the computer, or onto a new card. It never
deletes anything.

Before the first byte, Bezel checks that all the files fit in the free space and
that each one can go there, including the 25 MiB per file of Turing rev C
screens; if not, nothing is sent and Bezel says by how much they do not fit. A
file already there with the same name and size is skipped; the same name with
another size is left out unless you choose to replace it. The files go one at a
time, the oldest sent first, each one checked; **Cancel** and failures behave as
in a move.

In the app, **Restore…** (with how many files) appears above a list when files
Bezel sent there are missing or are on another card; choose the files,
**Continue…**, then **Restore**. On the command line:

```bash
bezel storage restore sd                     # the files Bezel sent to the card that are missing
bezel storage restore sd --yes
bezel storage restore internal intro.mp4 --yes   # one file by name, also one deleted through Bezel
```

```text
$ bezel storage restore sd
Restore 1 file to the memory card of Turing Smart Screen 8.8" from Bezel's local copies:
  sd/video/bezel_intro.mp4 -> sd/video/bezel_intro.mp4     1.2 MiB  (missing)
1 file, 1.2 MiB to send; 7.9 GiB free there; nothing is deleted.
Nothing on the screen was changed. Add --yes to restore it.
```

### The cleanup assistant

For the files Bezel did not send (the vendor app's, for example) and the
uploads that did not finish, the cleanup assistant points out likely leftovers:

- **Duplicate**: the copies the vendor app makes when it converts a file again,
  named like `x.mp4.mp4` or `x.mp4<digits>.mp4` (`NVI.mp427034822.mp4`), with
  the same size as the file that stays. Pre-checked.
- **Vendor copy**: the same names with another size. Only listed.
- **Interrupted upload**: a file of exactly 29,577,216 bytes, what an upload
  that hung a Turing rev C screen leaves. Pre-checked.
- **Unfinished upload**: a file Bezel started to send and never verified (its
  catalog state is *pending*). Pre-checked.
- **Same size** and **Size changed**: other files of exactly the same size and
  kind, and a file whose size is not the one Bezel sent. Only listed.
- **Not used**: a file Bezel did not send that no theme plays. Only listed.

Only exact signals are pre-checked; what is merely likely is shown, not chosen.
The assistant never suggests the file Bezel set to show at start, nor a video a
theme plays (a theme's video background, for example). It never runs on its
own, and nothing is deleted until you confirm the exact list.

In the app, **Cleanup assistant…** (above the lists) shows the suggestions by
group; check or uncheck them, then **Delete the checked…** lists the exact files
and the space freed, and **Delete these files** deletes them one at a time.
**Origin → Cleanup suggestions** shows them in the lists too.

On the command line, `bezel storage cleanup --dry-run` only lists them; with
`--yes` it deletes exactly the pre-checked files it printed, and nothing else
(without either, it lists them and says what `--yes` would delete):

```text
$ bezel storage cleanup --dry-run
Cleanup suggestions for Turing Smart Screen 8.8" (never the boot media Bezel set nor a video your themes play):
Pre-checked, deleted by `bezel storage cleanup --yes`:
  internal/video/bezel_cut.mp4      320.0 KiB  pending: an upload by Bezel that did not finish or failed its size check
Only listed (`bezel storage rm PATH --yes` deletes one you no longer need):
  sd/video/NVI.mp4                    5.4 MiB  unused: no theme plays it
  sd/video/NVI.mp427034822.mp4        5.1 MiB  variant: a vendor copy of sd/video/NVI.mp4 with another size
  …
1 file pre-checked (320.0 KiB to free), 12 files only listed.
Dry run: nothing was deleted.
```

### Files Bezel did not send

A file Bezel did not send has no local copy: it shows an icon instead of a
thumbnail (**Play on the screen** shows it), and it cannot be moved or renamed.
If you have its original on the computer, associate them: Bezel copies the
original into its local copies, which gives the file a thumbnail and makes it
movable.

- In the app: select the file, **Associate original…**, then **Choose files…**
  or **Choose a folder…**. Only files of exactly the same size in bytes and of
  the same kind are offered, likeliest first (by name, and by duration and
  resolution for a video); confirm the pair with **Associate**.
- On the command line:
  `bezel storage catalog associate sd/video/NVI.mp4 ~/Videos/NVI.mp4 --yes`
  (or a folder, to look in it). `bezel storage catalog forget PATH --yes` drops
  an entry from the catalog, and its local copy unless another entry has the
  same bytes; the screen is not changed.

### Turing USB screens

Bezel cannot delete files on the Turing USB generation, so nothing that ends in
a delete runs there: **Move to the SD card** (or to the internal memory),
**Rename…**, **Delete…** and the **Cleanup assistant…** are disabled, with the
reason in a note above the lists and on each button; the terminal prints it.
Copying to the other side, restoring and playing work. These screens do not
always report a file's size: Bezel shows the size it sent, or "size unknown"
(`?` in `bezel storage ls`).

### Two cards of the same size

The screen tells nothing about its card except its capacity, so Bezel recognizes
a card by its capacity: files Bezel sent to a card of another capacity show as
*on another card*, ready to restore onto this one. Two cards of the same size
look alike: Bezel takes one for the other, and the files it sent to the first
show as *missing* on the second. Screens of the same model cannot be told apart
either, so they share one catalog.

## What the screen shows at power-up

On rev C screens the power-up choice also stores the brightness the screen
starts with: in the app, the brightness set under **Settings**; on the command
line, `--brightness`; otherwise the vendor's default, about 67%. On the
Turing USB generation, Bezel can send and play files but cannot yet delete them,
play a video once, or change the power-up choice.

## Themes with a video background

A theme can use a video as its background: the screen loops the video and Bezel
draws the theme over it.

- If the video is not on the screen yet, the screen shows the theme's still
  picture and the app offers **Send to the screen**; `bezel run` prints the exact
  `bezel storage put` command.
- Screens that cannot play videos get them decoded on the computer, which needs
  ffmpeg (`bezel run --ffmpeg PATH` if it is not on the `PATH`).
- To give a theme a video background in the app (a video or an animated GIF),
  see [A video in the background](first-theme.md#a-video-in-the-background).

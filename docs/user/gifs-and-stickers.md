# GIFs and stickers

[Português (Brasil)](pt-BR/gifs-and-stickers.md)

Bezel Studio can search GIFs and stickers on KLIPY, a free GIF library, keep
the ones you like in your **collection** on this computer and use them in any
theme, like the pictures you add yourself. A sticker has a transparent
background: the theme shows through it.

Bezel has no KLIPY key of its own: you get a free one in a few minutes and
paste it once. Your searches use your key and count toward its limit. Without a
key, Bezel never connects to KLIPY.

## 1. Get your own KLIPY key

1. Open KLIPY's Partner Panel, [partner.klipy.com](https://partner.klipy.com),
   and create a free account.
2. Create an app there and copy its API key.
3. Paste the key in Bezel and save it ([next step](#2-save-the-key-in-bezel)).
4. A new key is a **test key**: it allows **100 requests per hour**, enough for
   searching now and then. For more, ask KLIPY for production
   ([Limits](#limits)).

The **?** button beside the key field in Bezel shows these same steps, with
**Open the Partner Panel** and **Open the guide** (this page). Esc, or a click
outside, closes it.

## 2. Save the key in Bezel

1. Open the **Media** tab, then **Collection** (next to **This theme**).
2. Click **Search GIFs and stickers…**. The dialog opens with the key field at
   the top.
3. Paste the key in **KLIPY API key** and click **Save**.

Saving sends nothing to KLIPY: a key that KLIPY refuses shows up at your first
search, which opens the help. Once saved, Bezel shows only the key's last 4
characters (**Saved key ending in** followed by them), never the whole key
again. **Remove** deletes it from this computer.

## 3. Search

In **Search GIFs and stickers**:

- Choose **GIFs** or **Stickers**.
- Type in the search field (its placeholder reads **Search KLIPY**). Bezel
  searches once you pause typing, with 2 characters or more; Enter searches at
  once. **Trending** shows what is popular now, without typing.
- The results come 24 at a time; **Load more** brings the next 24. **Powered
  by KLIPY** under the results credits where they come from.
- Nothing is searched when the dialog opens. A page of results already shown
  in this session is not asked again, so going back costs nothing.
- When the computer is set to reduce motion, the previews are still pictures.

With the keyboard: the arrow keys move through the results, Home and End go to
the first and the last, Enter adds the selected one to the collection. Screen
readers hear how many results came, the errors and what was added.

### Explicit results

**Show explicit results** is off every time Bezel starts. Off, Bezel asks
KLIPY for results rated G and PG (KLIPY's `medium` content filter): safe on a
screen on your desk, and everyday reaction GIFs stay. On, Bezel asks for
unfiltered results (KLIPY's `off` filter), which may include adult content. The
choice lasts until Bezel closes, and changing it searches again from the first
page. Content settings you make for your key in the Partner Panel may narrow
the results further.

## 4. Add to the collection

Click **Add to collection** on a result (or press Enter on it). Bezel downloads
the largest GIF version of it that is at most **25 MiB**, the most a Turing
rev C screen takes per file, so the GIF can also go to the screen as it is
([How large a file can be](storage-and-video.md#how-large-a-file-can-be)).

- Adding the same GIF twice keeps one copy: Bezel recognises it by its
  content.
- If what arrives is not a GIF, nothing is kept and Bezel says so. An item
  with no GIF version under 25 MiB cannot be added.
- The item gets the result's title as its name, which you can change.

## 5. Use and manage the collection

**Media → Collection** lists what you added, with a filter (**All**, **GIFs**,
**Stickers**), how many items there are and their total size. For each item:

- **Add as image**: an image element in the middle of the canvas. An animated
  GIF moves at its own pace ([Animated GIFs](first-theme.md#animated-gifs)).
- **Use as background**: the GIF becomes the theme's background, like an
  animated GIF added with **Add video…**: it gets a poster (with ffmpeg), and
  the screen plays it as a video
  ([A video in the background](first-theme.md#a-video-in-the-background)).
- **Drag** it onto the canvas: an image element where you drop it.
- **Rename**: edit the name in place; it cannot be empty.
- **Delete…**: asks first, naming your themes (and the theme open now) that
  use the same GIF. They keep their own copy and go on working. Deleting
  cannot be undone.

Using an item copies the GIF into the theme, under the item's name, so the
theme (and its `.bezeltheme` file) carries it and the item appears in **This
theme**. A sticker's transparent parts show what is under it, the background
or other elements, in vertical and horizontal themes alike. When motion is
reduced, the collection shows still pictures.

## Limits

- **100 requests per hour** with a test key. Each page of results (a search,
  **Trending**, **Load more**) is one request to KLIPY's API; a page already
  shown in the session is not asked again, and Bezel waits for a pause in your
  typing instead of searching at every letter.
- **Production**: when the test key is not enough, in the Partner Panel find
  your key, open the three-dot menu on it and choose **Request Production**,
  then fill in the short form. KLIPY decides on the request; the same key keeps
  working in Bezel, nothing to change.
- **25 MiB** per GIF added to the collection (see
  [Add to the collection](#4-add-to-the-collection)).
- GIF only: KLIPY's WebP and MP4 versions are not used. The command line does
  not search KLIPY.

## When something goes wrong

### The key is refused

KLIPY answered that the key is not valid: Bezel opens the key help. Check that
you copied the whole key and nothing else, and that it still exists in the
Partner Panel; paste it again and **Save**. Bezel takes only letters, digits,
`_` and `-` (up to 128 characters) and says so when the field has anything
else.

### "The key reached its limit"

The key made its 100 requests of this hour (on a test key). Wait for the hour
to pass, or ask for production ([Limits](#limits)): the message has a button
that opens the Partner Panel. The results already shown stay, and the
collection keeps working.

### KLIPY cannot be reached

Without a connection, or when KLIPY does not answer within 10 seconds, the
search says KLIPY is unavailable. Check the connection; a firewall or proxy
must let Bezel reach `api.klipy.com` and `static.klipy.com` over HTTPS. The
collection is on this computer and keeps working offline, and so do the
themes that use its items.

### The search field is disabled

There is no key yet: save one first ([Save the key in Bezel](#2-save-the-key-in-bezel)).

## Your data

### Privacy

**When.** Nothing reaches KLIPY when Bezel starts, nor ever without a key.
Saving the key sends nothing. Bezel connects only when you act: a search (or
**Trending**), **Load more**, the previews of the results on screen, and
**Add to collection**.

**What, and to whom.** Everything goes to KLIPY, over HTTPS, and nowhere else:

- To `api.klipy.com`, for each page of results: your key (inside the
  request's address), the text you typed (nothing for **Trending**), GIFs or
  stickers, the page number and 24 results per page, the content filter
  chosen by **Show explicit results**, the formats Bezel wants (GIF, and JPEG
  stills), `BR` as the region when Bezel is in Portuguese, and a customer id: a
  random number Bezel made when you saved the key, which says nothing about
  you. These requests never follow a redirect, so the key goes to no other
  server.
- To `static.klipy.com`, without your key: the previews of the results shown,
  and the GIF you add to the collection.
- Like any web server, KLIPY sees your IP address and when each request comes;
  its privacy policy says what it does with them.

Nothing else is sent: no ads, no usage statistics, and Bezel does not use
KLIPY's share or report features.

**Where your key is.** Only on this computer, in `klipy.json` in Bezel's
configuration folder, next to its settings:

- Linux: `~/.config/io.github.slipalison.bezel/klipy.json` (or under
  `$XDG_CONFIG_HOME`);
- Windows: `%APPDATA%\io.github.slipalison.bezel\klipy.json`.

Only your user can read the file (on Linux its permissions are `0600`). The
key is kept as plain text there, not in the system's keyring. Bezel's window
never gets it back (only its last 4 characters), and it never appears in
Bezel's messages or logs.

**Where your collection is.** In your data folder, beside Bezel's
[local copies](storage-and-video.md#bezels-local-copies):

- Linux: `~/.local/share/bezel/collection` (or `$XDG_DATA_HOME/bezel/collection`);
- Windows: `%APPDATA%\bezel\collection`.

`collection.json` lists each item's name, kind, size, when you added it and
where it came from (KLIPY's id and page address); `files/` holds the GIFs and
`previews/` their previews. A theme that uses an item has its own copy inside
the theme.

**Keeping downloads.** Bezel keeps what you add until you delete it. KLIPY's
API terms say whether, and for how long, downloaded GIFs may be kept: check
them for your key, and delete the items they do not let you keep.

### Removing the key and the collection

- **Remove**, beside the key field, deletes `klipy.json`; searching is
  disabled until you save a key again, which gets a new customer id.
- To revoke the key itself, delete it in the Partner Panel.
- **Delete…** removes one item from the collection. To remove the whole
  collection, close Bezel and delete the `collection` folder above. Themes
  keep their own copies either way.

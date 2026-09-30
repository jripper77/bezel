# Preparing an SD card

[Português (Brasil)](pt-BR/sd-card.md)

Screens with a card slot read **FAT32** cards with an **MBR** partition table.
Bezel never formats the card: prepare it on the computer, then put it in the
screen.

New cards of 32 GB or less usually come ready (MBR and FAT32). Larger cards come
with exFAT, which the screen does not read: format them as below. Bezel was
tested with a 32 GB card.

**Formatting erases everything on the card.** Check twice that you picked the
card and not another disk.

## Linux, with GNOME Disks

1. Open **Disks** and select the card in the list on the left (check its size).
2. In the menu at the top right, choose **Format Disk…**, pick **Compatible with
   all systems and devices (MBR / DOS)** and confirm.
3. Click **+** under the empty space to create a partition of the whole card,
   type **For use with all systems and devices (FAT)**, and confirm.

## Linux, in a terminal

Find the card by its size and name (for example `sdb`, or `mmcblk0` for a
built-in reader):

```bash
lsblk -o NAME,SIZE,MODEL,TRAN
```

Then, replacing `sdX` with the card (unmount it first if the desktop mounted it):

```bash
sudo parted /dev/sdX --script mklabel msdos mkpart primary fat32 1MiB 100%
sudo mkfs.vfat -F 32 /dev/sdX1
```

For an `mmcblk0` card the partition is `/dev/mmcblk0p1`.

## Windows

In File Explorer, right-click the card, choose **Format…**, pick **FAT32** and
click **Start**. Windows offers FAT32 only for cards up to 32 GB; for a larger
card, format it on Linux or with a FAT32 formatting tool.

## In the screen

1. With the screen unplugged, insert the card.
2. Plug the screen back in.
3. Check that Bezel sees it: **Screen → Storage** shows **SD card** with its
   size, or run `bezel storage info`.

"No SD card in the screen" means the screen found no card it can read: check
that the card is FAT32 on an MBR partition table.

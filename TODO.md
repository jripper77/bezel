# Bezel Evo — TODO

Checked items are implemented; unchecked items are design notes for future work.

## Quick states from the Light tray

- [ ] Let users define named states such as Default, Work, Gaming and Music.
- [ ] Select a state directly from the tray icon menu without opening Studio.
- [ ] Associate each state with a saved theme/template, optionally a different
      theme for each screen. Mark the active state in the menu.
- [ ] Animate state/theme switches with a configurable transition (fade, slide
      or flip), duration and direction. Apply transitions to both manual tray
      selections and automatic triggers; preview them in Studio. Handle rapid
      selections, unavailable displays and unsupported effects gracefully.
- [ ] Define whether the selected state is temporary or restored at next startup.
- [ ] Provide a return to Default/Automatic and define how a manual selection
      interacts with automatic triggers, including priority and override duration.
- [ ] Configure states in Studio and switch them safely in Light, preserving
      display ownership and reporting missing themes or unavailable screens.

## Card widget with a base and multiple faces

Face transitions are implemented; face selection remains manual. Cards move and resize their
objects together, including inactive faces. Objects retain their own appearance
settings and are not clipped to the card bounds. Timers and entry/exit animations below are still planned.

- [x] Create a dedicated Card container with a shared base and one initial face.
- [x] Add, duplicate, rename and reorder faces; edit the base or a selected face.
- [x] Group text, indicators, icons, images and shapes within each face.
- [x] Create a card from selected objects; preserve layers, Undo and clipboard.
- [x] Configure face-transition animations: none, fade, slide and flip, with
      duration, direction and smooth acceleration/deceleration.
- [ ] Configure entry and exit animations independently.
- [x] Choose whether the shared base participates in the transition.
- [x] Add editor controls to animate the next face. Respect reduced motion in the preview.
- [ ] Support timed face changes and event-driven face selection, with priority,
      minimum display time, return behavior and sensor threshold hysteresis.
- [x] Measure host frame-delivery rate, rendering and serial costs on the real
      8.8-inch screen; keep panel refresh distinct from host completion.
- [x] Deliver independent per-display Studio workers; keep face caching optional after measurement.
- [x] Validate the 0.1.6 face cache on the user's device-video theme; compare cache
      hits, face drawing/composition times, delivery cadence and long intervals.
      Default restored to direct face drawing in 0.1.8: caching did not improve
      overall delivery cadence and increased rendering variability.

## Software and media triggers

- [ ] Trigger a face or state when a specified application starts or stops.
- [ ] Distinguish an application being open from media actively playing.
- [ ] Investigate Windows media-session access for Spotify: playback state,
      title, artist and cover art, including whether additional authorization is
      needed and which fields are reliably available.
- [ ] Show the Music/player face when Spotify plays; after a configurable pause
      or stop delay, return to the previous face or state.
- [ ] Resolve simultaneous triggers and manual tray overrides predictably.

The existing themes remain usable without cards, states or automatic triggers.


## Existing features: corrections and extensions

- [x] Layers: show each card face as a container, with its objects indented below
      it. Show the shared base separately; keep selection, visibility, ordering
      and card membership understandable when switching faces.
- [x] Card timers: configure automatic face rotation (5?3600 seconds), using
      the existing transition. Pause while editing a card/member in Studio;
      keep runtime playback outside saved themes and Undo.
- [ ] Coordinate timers with event priorities, explicit overrides and return
      behavior when software/media triggers are introduced.
- [ ] Software events: assess process start/stop, foreground application and media
      session events before choosing supported triggers. Implement only sources
      that are available reliably and without per-frame polling.
- [ ] Media-player widgets: show title, artist, cover and playback state; evaluate
      progress and playback controls using Windows media sessions. Support Spotify
      where its session exposes data; define behavior for multiple sessions and
      absent/stopped media.
- [ ] Images: add linear and radial opacity gradients, matching shape controls;
      apply opacity to the image contents and retain image framing/fit behavior.
- [ ] Shapes: add radial opacity gradients alongside existing linear opacity,
      with editable center, extent and start/end opacity.
- [ ] Screen sleep timer: configure minutes per display, default 5 minutes.
      Expose device capabilities and supported limits, a disabled state and
      persistence. Define any host fallback explicitly; do not assume all panels
      have the same firmware sleep command.
- [ ] Weather: add colored and 3D-style icon collections selectable per widget.
      Choose redistributable free assets, bundle licenses/credits and preserve
      existing monochrome styles and condition mappings.

## UX: first interface cleanup

- [ ] Review the editor's main navigation and remove duplicated or misplaced
      controls while preserving existing workflows.
- [ ] Group object properties by purpose with a consistent order: content/data,
      appearance, position/size, card membership and animation where applicable.
      Make common controls easy to find and show advanced controls in context.
- [ ] Align labels, units, spacing and disabled states across widget types;
      validate the result in both small and large editor windows.

## New ideas

- [ ] Calculated variables: named expressions combining available sensor values
      and other variables, selectable as widget data sources. Define units,
      missing values, division by zero, dependency cycles and evaluation cadence.
      Preview the expression result and report errors in the editor; keep the
      same evaluation rules in Studio and Light.
- [ ] PC-idle screensaver: after configurable keyboard/mouse inactivity, switch
      displays to a selected screensaver theme or media, then restore their
      previous state on activity. Keep this policy distinct from the screen's
      sleep timer and from Windows standby; handle multiple displays and Light.

Complete and measure the current animation release before implementing these
items. Document trigger feasibility first; do not imply that every application
exposes playback metadata or controls.

# Changelog

All notable changes to `atelier`. The format follows [Keep a Changelog](https://keepachangelog.com/1.1.0/).

## [Unreleased]

### Added

- Up on the first line of the chat input and Down on the last recall the previous and next
  prompt, as in a shell; Ctrl+Up and Ctrl+Down recall from anywhere in the text.

### Fixed

- An audio output that fails is closed and the device opened again, instead of logging a
  line on every period and never playing again.

### Changed

- One visual grammar, shared with the phonix plugins: a palette of two skins built on three
  surface depths (a raised plate, a flat panel, a sunk well) with the light coming from above
  in both; one accent; a type scale of five sizes; section panels with a striped title;
  chrome rows of pinned height; labels in capitals, values in monospace cells of fixed width;
  lamps for state, with the word beside them. Nothing moves under the pointer.
- The chat is rows in a well, the Studio is section panels in two columns with every
  description on hover, the model list and the log are lines with a stripe, the terminal is
  a screen. The colour relations of both skins are pinned by tests.
- Documentation screenshots for the model list and the server log.

## [0.1.0]

### Added

- Chat with streaming, image attachments for vision models, and speech input.
- Media Studio for image, image editing, audio, speech, video and transcription. Each kind
  owns its parameters, so switching kinds keeps the tweaks made to the others.
- A viewer with fullscreen and zoom, and a strip of earlier generations that survives a new
  run.
- Hardware and server views: per-device topology, what is loaded where, and the server log.
- In-process audio output (`native-audio`, on by default), so the transport behaves the same
  on every platform.
- Video that decodes as it plays, keeping the packets and producing pictures around the
  playhead.
- A cost estimate shown next to the button that starts a render.
- Light and dark themes, with muted text pinned against the WCAG AA contrast floor on both
  background and surface, in both, by test.

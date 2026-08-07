# Changelog

All notable changes to `atelier`. The format follows [Keep a Changelog](https://keepachangelog.com/1.1.0/).

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

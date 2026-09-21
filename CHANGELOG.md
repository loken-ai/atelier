# Changelog

All notable changes to atelier. The format follows [Keep a Changelog](https://keepachangelog.com/1.1.0/).

## [Unreleased]

### Added

- Chat: Up and Down at the input edges recall the previous and next prompt; Ctrl+Up and Ctrl+Down from anywhere.
- Models: the picker opens in a popup of its own.
- Studio: the status names which node renders.
- Docs: the chat's Auto mode is documented.
- CI: a workflow with a dependency audit.

### Fixed

- Audio: a failed output is closed and reopened, instead of logging every period and never playing again.
- Build: clean build and lint without audio output.
- Audio: PCM samples are read as fixed-size chunks.

### Changed

- Theme: one visual grammar, two skins on three surface depths, one accent, five type sizes.
- Theme: chat as rows in a well, Studio two columns, terminal a screen; skin colours pinned by tests.
- Docs: screenshots for the model list and the server log.
- Docs: examples use documentation addresses and the current project name.
- Docs: each screenshot sits with the section that explains it.

## [0.1.0]

### Added

- Chat: streaming, image attachments for vision models, and speech input.
- Studio: image, editing, audio, speech, video and transcription, each kind keeping its own parameters.
- Viewer: fullscreen and zoom, and a strip of earlier generations that survives a new run.
- Views: hardware and server, per-device topology, what is loaded where, and the server log.
- Audio: in-process output (native-audio, on by default), so the transport is the same on every platform.
- Video: decodes as it plays, keeping packets and producing pictures around the playhead.
- Studio: a cost estimate shown next to the button that starts a render.
- Theme: light and dark, muted text pinned against the WCAG AA contrast floor on both, by test.

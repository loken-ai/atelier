# atelier

The desktop client for [LOKEN](https://github.com/loken-ai/loken): a chat window, and a
studio for generating images, audio and video against models running on your own machine.

```sh
cargo build --release      # target/release/atelier
```

It talks to the server over HTTP and holds no model itself, so it runs on a laptop while the
work happens on the box with the cards.

## Chat

A conversation with the model the header names. The reasoning a model produces before its
answer sits folded above the answer, each reply carries its rate, token count and time, and
a reply still arriving can be stopped from the composer. The clip beside the composer
attaches an image for a vision model or speech for a transcription model, and Up on the
first line or Down on the last recalls earlier prompts, as in a shell.

![Chat: a conversation mid-answer, the first reply with its timing, the second still streaming](docs/img/atelier-chat.png)

## Media Studio

One panel per kind of render: image, image editing, music, sound effects, MIDI, video,
speech, transcription and source separation. Each kind keeps its own parameters, so moving
from image to video and back loses nothing. Picking a model applies the steps, guidance and
size it was validated at; the shapes offered are the buckets the models were trained on; a
negative prompt, regions that keep two subjects apart and a seed are there when a render
needs them. Video shows what a clip will cost before it renders, because the answer can be
an hour.

![Media Studio: the image panel with a prompt, the model and its recommended settings, shape, sampling, guidance, regions and seed](docs/img/atelier-media.png)

What comes back opens in a viewer with fullscreen and zoom, above a strip of earlier
generations that a new run does not wipe.

## Models

What the server can serve, filtered by kind and sorted by name, size or date, with the
loaded ones marked. A model is loaded, unloaded or deleted from its row, and a new one is
pulled from Ollama or Hugging Face by tag, with a few quick installs a click away.

![Models: three models with their kinds and sizes, one loaded, and the import bar](docs/img/atelier-models.png)

## Terminal and server logs

The server's own log as it works, filtered by level and searched by text, each line tagged
with the part of the server that wrote it. A terminal beside it takes the server's own
commands, to list, load, unload and pull models and to ask what the server is doing.

![Server logs: the server's lines by level, a warning and an error picked out](docs/img/atelier-logs.png)

What each machine in a cluster is doing is not here: it lives in
[atlas](https://github.com/loken-ai/atlas). One machine's view of its own cards could never
show the others.

The pictures are rendered by the test suite from state written in `src/screenshots.rs`, so
they show the current layout and can show no real address, path or prompt. Regenerate them
with `cargo test --release screenshots -- --ignored`.

## Where it is careful

- **Playback is in-process.** Handing audio to a system player makes the transport mean
  something different on every platform - pause becomes killing a process, seeking becomes
  restarting from a temporary file, and the position drifts. The samples are decoded here and
  handed to the operating system's audio device.
- **Video decodes as it plays.** Decoding a clip up front costs hundreds of megabytes before
  the first picture appears, and the wait grows with the length of the clip. This keeps the
  packets and produces pictures around the playhead.
- **A cost estimate before a render, not after.** Video is the one kind where the answer can
  be an hour, which is not something to discover by waiting.
- **Contrast is a test, not an intention.** Muted text is pinned against the WCAG AA floor on
  both the background and the card surface, in both themes, so a palette change cannot quietly
  take fifty sites under it.

## Build requirements

Linux needs the ALSA development headers (`libasound2-dev`) for in-process audio; Windows and
macOS need no system package. `--no-default-features` builds without audio output, and the
transport then says so rather than behaving differently.

## Licence

MIT OR Apache-2.0, at your option.

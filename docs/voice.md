# Voice input (push-to-talk)

cox can turn speech into a prompt in the TUI. It records from your microphone while you hold (or toggle) a key, transcribes locally with [whisper.cpp](https://github.com/ggml-org/whisper.cpp) through `whisper-rs`, and puts the text in the composer. There is no cloud service and no API key.

Voice input is built only with the `voice` cargo feature, which is off by default. Release builds do not include it.

## Setup

1. Build with the feature. whisper.cpp is a C++ build, so you need `cmake` (pinned in `mise.toml`) and a C++ compiler; on Linux you also need the ALSA headers (`libasound2-dev` on Debian and Ubuntu).

   ```bash
   mise exec -- cargo build --release --features voice
   ```

2. Download a model. Only the models in cox's pinned table are offered. Each one is fetched from a fixed commit and checked against its SHA-256 before it is kept.

   ```bash
   cox voice model list               # name, size, and whether it is downloaded
   cox voice model download base.en   # asks first; pass --yes when stdin is not a terminal
   ```

   Models go to `~/.cox/models/whisper/` (or `$COX_HOME/models/whisper/`). The `.en` models are English-only and are faster and more accurate for English. `tiny.en` (74 MiB) is quickest, `base.en` (141 MiB) is the default, and `small.en` (465 MiB) is the most accurate.

3. Turn it on in `~/.cox/config.toml`:

   ```toml
   [voice]
   enabled = true
   model = "base.en"      # a name from `cox voice model list`
   language = "en"        # ISO-639-1 code; "auto" lets whisper detect it
   key = "alt+v"          # the push-to-talk key
   auto_submit = true     # send the transcript when the draft was empty
   max_seconds = 120      # longest single recording
   ```

   A project's `.cox/config.toml` cannot set any `[voice]` key. A cloned repository must not be able to turn on your microphone.

4. Run `cox doctor`. The `voice` row shows whether the feature is built, whether it is enabled, whether the model is present, and which input device it will use.

## Using it

- Press `Alt+V` to start recording and press it again to stop. In terminals that report key releases (kitty, WezTerm, Ghostty, and others with the kitty keyboard protocol), hold the key while you speak and let go to stop.
- `Esc` while recording cancels it, and nothing is inserted.
- The status row shows `● rec 0:07` while recording and `transcribing…` afterwards.
- The transcript goes into the composer at the cursor. If the draft was empty when you pressed the key, and still is, and `auto_submit` is on, the transcript is sent as `Enter` would send it. While a turn runs, it is queued the same way `Enter` queues. Text added to a draft you had already started is never sent automatically, so you can review it first.
- If nothing was heard, a dim notice says so and nothing is inserted.

To use a different key, set `[voice] key`, or bind the `voice` action in `~/.cox/keybindings.toml`. A non-default `[voice] key` wins over `keybindings.toml`.

On macOS, `Alt+V` reaches cox only if your terminal sends Option as Meta (Terminal: Profiles › Keyboard › "Use Option as Meta key"; iTerm2: Profiles › Keys › "Left Option key: Esc+"). Otherwise, choose another key.

## The macOS microphone prompt

The first recording makes macOS ask whether your terminal app (Terminal, iTerm2, Ghostty, and so on) may use the microphone. The permission belongs to the terminal, not to cox. If you said no, recording fails with a notice. Allow the terminal under System Settings › Privacy & Security › Microphone, then restart it.

## What never happens

- Audio never leaves your machine. Transcription runs in the cox process, and the only network request voice input ever makes is the model download you start yourself.
- Audio is never stored. Samples live in memory only until they are transcribed or you cancel, and they are never written to disk.
- Audio never enters the rollout or the cost ledger. Only the text you send becomes part of the session, exactly as if you had typed it. Local transcription costs nothing, so there is no usage row for it.

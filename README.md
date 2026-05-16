# dns-streaming

tunneling video chunks over DNS cus yeah...

The server encodes video frames into DNS TXT-style A record responses and streams
them chunk by chunk. Also handles fragmentation w the truncate (TC=1) flag.

```

client              server
|                     |
|-- chunk-{n}.local-->|
|<-- chunk bytes ---->|
```

## Usage

Start the server with a video file:

```bash
cargo run <video-file>
```

Then in another terminal tab:

```bash
cd client && cargo run
```

A window should pop up with the video playing (no audio tho :P)

This will be choppy too, UDP doesn't guarantee shit for delivery. DNS wasnt built for this and that's
kind of the point of this thing.

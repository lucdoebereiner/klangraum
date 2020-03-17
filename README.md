# klangraum
sum incoming websocket audio streams and send to jack

## run

``` bash
$ cargo run --release
```

Open client.html.

## TODO
* Websocket close
* next_buffers max size
* max streams
* Check client side for efficiency (audio worklet perhaps)
* Opus or mp3 for audio over websocket (now pcm)?

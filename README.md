# klangraum
sum incoming websocket audio streams and send to jack

## run

``` bash
$ cargo run --release localhost:8800 cert key
```

Open client.html.

## TODO
* max streams
* Check client side for efficiency (audio worklet perhaps)
* Opus or mp3 for audio over websocket (now pcm)?

# klangraum
sum incoming websocket audio streams and send to jack

## run

``` bash
$ cargo run --release localhost:8800 pfx-file pw channels
```

Open client.html.

## TODO
* handle connection closed more gracefully
* perhaps lower sr
* max streams
* return stats
* Check client side for efficiency (audio worklet perhaps)
* Opus or mp3 for audio over websocket (now pcm)?

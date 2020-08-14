# klangraum
sum incoming websocket audio streams and send to jack

## run

``` bash
$ cargo run --release localhost:8800 channels <optional pfx-file> <optional pw>
```

Open client.html.

## TODO
* max streams
* return stats
* Check client side for efficiency (audio worklet perhaps)


# sdl
Download multiple episodes from streaming sites

## Supported sites
### German
* [AniWorld](https://aniworld.to)
* [S.to](https://s.to)

## Supported extractors
* Doodstream
* Filemoon
* LoadX
* Speedfiles
* Streamtape
* Vidmoly
* Vidoza
* Voe

## Usage
### Downloading a single episode
By URL:
```bash
sdl 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily/staffel-1/episode-1'
```
By specifying it explicitly:
```bash
sdl -e 11 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily/staffel-2'
```

### Downloading an entire season
By URL:
```bash
sdl 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily/staffel-2'
sdl 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily/filme'
```
By specifying it explicitly:
```bash
sdl -s 2 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily'
sdl -s 0 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily'
```

### Downloading multiple episodes
```bash
sdl -e 1,2-6,9 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily/staffel-2'
```

### Downloading multiple seasons
```bash
sdl -s 1-2,4 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily'
```

### Downloading all seasons
```bash
sdl 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily'
```

### Downloading in other languages
```bash
sdl -t gersub 'https://s.to/serie/stream/higurashi-no-naku-koro-ni/staffel-1/episode-1'
```
Either dub or sub:
```bash
sdl -t ger 'https://s.to/serie/stream/higurashi-no-naku-koro-ni/staffel-1/episode-1'
sdl -t german 'https://s.to/serie/stream/higurashi-no-naku-koro-ni/staffel-1/episode-1'
```
If an episode has multiple languages, the general language preference is as follows:
* English Anime Website: EngSub > EngDub
* German Anime Website: GerDub > GerSub > EngSub > EngDub
* German non-Anime Website: GerDub > GerSub > EngDub > EngSub

### Prioritize specific extractors
First try Filemoon, then Voe, and finally try every other possible extractor using the `*` fallback:
```bash
sdl -p filemoon,voe,* 'https://aniworld.to/anime/stream/yuruyuri-happy-go-lily/staffel-1/episode-1'
```

### Downloading with extractor directly
```bash
sdl -u 'https://streamtape.com/e/DXYPVBeKrpCkMwD'
sdl -u=voe 'https://prefulfilloverdoor.com/e/8cu8qkojpsx9'
```

### Help output
```
Usage: sdl [OPTIONS] <URL>

Arguments:
  <URL>  Download URL

Options:
      --type <VIDEO_TYPE>
          Only download specific video type [possible values: raw, dub, sub]
      --lang <LANGUAGE>
          Only download specific language [possible values: english, german]
  -t <TYPE_LANGUAGE>
          Shorthand for language and video type
  -e, --episodes <RANGES>
          Only download specific episodes
  -s, --seasons <RANGES>
          Only download specific seasons
  -p, --extractor-priorities <PRIORITIES>
          Extractor priorities
  -u, --extractor[=<NAME>]
          Use underlying extractors directly
  -N, --concurrent-downloads <INF|NUMBER>
          Concurrent downloads [default: 5]
  -r, --limit-rate <RATE>
          Maximum download rate in bytes per second, e.g. 50K or 4.2MiB
  -R, --retries <INF|NUMBER>
          Number of download retries [default: 5]
      --ddos-wait-episodes <NEVER|NUMBER>
          Amount of requests before waiting [default: 4]
      --ddos-wait-ms <MILLISECONDS>
          The duration in milliseconds to wait [default: 60000]
      --mpv
          Play in mpv
  -d, --debug
          Enable debug mode
  -h, --help
          Print help
  -V, --version
          Print version
```

## Notes
If FFmpeg and ChromeDriver are not found in the `PATH`, they will be downloaded automatically.

Also, I don't plan to add new sites or extractors, but you're welcome to create a Pull Request if you want to add one.

By the way, it's also possible to use `sdl` as a library.

## Build from source
Currently, Rust 1.75 or newer is required.
```
cargo build --release
```
The resulting executable is found at `target/release/sdl`.

## Thanks
* [aniworld_scraper](https://github.com/wolfswolke/aniworld_scraper) for the inspiration and showing how it could be done

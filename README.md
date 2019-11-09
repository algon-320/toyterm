# toyterm

toyterm is a toy terminal emulator.


## Demo

![demo1.png](./demo/demo1.png)
![demo2.gif](./demo/demo2.gif)

## Features

- on Linux
    - it uses some Linux specific system calls
- written in Rust
- SDL2 (rendering)
- partially support VT100's control sequences
- 24bit color support


## Usage

### 1. Register terminfo

run following commands

```sh
$ tic -o ${HOME}/.terminfo/ src/toyterm.cap
```

When you want to uninstall, please simply remove `${HOME}/.terminfo/t/toyterm-256color`.

### 2. Edit `settings.toml`

```toml
[font]
regular = # full path to a ttf font file to use as the regular font
bold = # full path to a ttf font file to use as bold font
size = # the width of half-width char (in pixel)
```

**Only monospaced font is available (now).**

Here is my example
```toml
[font]
regular = "/usr/share/fonts/truetype/ricty-diminished/RictyDiminished-Regular.ttf"
bold = "/usr/share/fonts/truetype/ricty-diminished/RictyDiminished-Bold.ttf"
size = 10
```

### 3. Run

```sh
$ cargo run --release
```


## Functions

### escape sequences

see `src/terminal/control.rs` and `src/toyterm.cap`
# toyterm

toyterm is a toy terminal emulator.

## Usage

Install:

```sh
$ git clone https://github.com/algon-320/toyterm
$ cd toyterm
$ tic -o ${HOME}/.terminfo/ toyterm.info
$ cargo install --path .
```

Uninstall:
```sh
$ rm ${HOME}/.terminfo/t/toyterm-256color
$ cargo uninstall toyterm
```

## Features/Limitations

- hardware accelerated graphics
- support for SIXEL graphics
- support for X11 clipboard (copying & pasting)
- manual font fallback: you can specify the order of fonts for each style
- toyterm assumes UTF-8 encoding
- following basic functions are TODO
    - automatic font selection by integrating with fontconfig
    - configuration (shell, color scheme, keybindings, etc.)

## Keybinding

|Key|Function|
|:----------|:-------|
|Ctrl + `-` |Decrease font size|
|Ctrl + `=` |Increase font size|
|Ctrl + Shift + `c` |Copy selected text|
|Ctrl + Shift + `v` |Paste clipboard text|
|Ctrl + `l` |Clear history|
|Up key|Send `\x1b[[A`|
|Down key|Send `\x1b[[B`|
|Right key|Send `\x1b[[C`|
|Left key|Send `\x1b[[D`|
|PageUp key|Send `\x1b[5~`|
|PageDown key|Send `\x1b[6~`|
|Delete key|Send `\x1b[3~`|
|Backspace key|Send `\x7f`|
|Mouse Wheel|Same effect as arrow keys (Up/Down/Right/Left)|
|Shift + Mouse Wheel|Scroll history|

## Control Functions

toyterm aims to support the standard control functions described in ECMA-48.
Some private functions, widely used by modern terminals, may be supported as well.
Currently toyterm supports the following functions.

### C0 functions

- BS
- CR
- ESC
- FF
- HT
- LF
- VT

### C1 functions

- CSI

### Control Sequences

- CHA
- CUB
- CUD
- CUF
- CUP
- CUU
- DCH
- DL
- DSR
- ECH
- ED
- EL
- ICH
- IL
- RM
- SGR
- SM
- VPA

- SelectCursorStyle:
    - Block: `\e[2 q`
    - Bar: `\e[6 q`

## Device Control Function

- DCS `q` (sixel string...) ST
    - see <https://www.vt100.net/docs/vt3xx-gp/chapter14.html> for the representation

## Modes

toyterm supports the following modes.

- Cursor Visible Mode (`?25`)
    - Set: cursor is visible.
    - Reset: cursor is invisible.
- Sixel Scrolling Mode (`?80`)
    - Set: a sixel image is displayed at the current cursor position.
    - Reset: a sixel image is displayed at the upper left corner of the screen.
- Alternate Screen Buffer Mode (`?1049`)
    - Set: clear the screen, save the cursor position, and switch to the alternate screen.
    - Reset: restore the saved cursor position, and switch back to the primary screen.
- Bracketed Paste Mode (`?2004`)
    - Set: insert `\x1b[200~` at the beginning and `\x1b[201~` at the end of a pasted text.
    - Reset: a pasted text is send to the terminal as if it was typed by user.

## License

This software is licensed under MIT License.

The embedded font (M PLUS 1 Code) itself is redistributed under the Open Font License (OFL).
See `font/OFL.txt` for more details.

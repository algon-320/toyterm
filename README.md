# toyterm

toyterm is a toy terminal emulator.

## Features/Limitation

- hardware accelerated graphics
- toyterm assumes UTF-8 encoding

## Keybinding

|Combination|Function|
|:----------|:-------|
|Ctrl + `-` |Decrease font size|
|Ctrl + `=` |Increase font size|

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
- SGR
- VPA

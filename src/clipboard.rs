use x11_clipboard::xcb::x::Atom;
use x11_clipboard::Clipboard;

pub struct X11Clipboard {
    inner: Clipboard,
    atom_clipboard: Atom,
    atom_utf8_string: Atom,
    atom_toyterm: Atom,
}

impl X11Clipboard {
    pub fn new() -> Self {
        let cb = x11_clipboard::Clipboard::new().expect("Failed to initialize X11 clipboard");

        let ctx = &cb.getter;
        let atom_clipboard = ctx.get_atom("CLIPBOARD").unwrap();
        let atom_utf8_string = ctx.get_atom("UTF8_STRING").unwrap();
        let atom_toyterm = ctx.get_atom("toyterm").unwrap();

        Self {
            inner: cb,
            atom_clipboard,
            atom_utf8_string,
            atom_toyterm,
        }
    }

    // FIXME: specify error type
    pub fn load(&self) -> Result<String, ()> {
        let data: Vec<u8> = self
            .inner
            .load(
                self.atom_clipboard,
                self.atom_utf8_string,
                self.atom_toyterm,
                None,
            )
            .map_err(|_| ())?;

        String::from_utf8(data).map_err(|_| ())
    }

    // FIXME: specify error type
    pub fn store(&self, sel: &str) -> Result<(), ()> {
        self.inner
            .store(self.atom_clipboard, self.atom_utf8_string, sel)
            .map_err(|_| ())
    }
}

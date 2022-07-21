#![allow(dead_code)]

use crate::sixel;

#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum Function<'p> {
    GraphicChar(char),
    Unsupported,
    Invalid,

    // C0 set
    NUL,
    SOH,
    STX,
    EOT,
    ENQ,
    ACK,
    BEL,
    BS,
    HT,
    LF,
    VT,
    FF,
    CR,
    SO,
    SI,
    DLE,
    DC1,
    DC2,
    DC3,
    DC4,
    NAK,
    SYN,
    ETB,
    CAN,
    EM,
    SUB,
    ESC,
    IS4,
    IS3,
    IS2,
    IS1,

    // C1 set
    BPH,
    NBH,
    NEL,
    SSA,
    ESA,
    HTS,
    HTJ,
    VTS,
    PLD,
    PLU,
    RI,
    SS2,
    SS3,
    DCS,
    PU1,
    PU2,
    STS,
    CCH,
    MW,
    SPA,
    EPA,
    SOS,
    SCI,
    ST,
    OSC,
    PM,
    APC,

    // Control Sequence (w/o intermediate bytes)
    ICH(u16),
    CUU(u16),
    CUD(u16),
    CUF(u16),
    CUB(u16),
    CNL,
    CPL,
    CHA(u16),
    CUP(u16, u16),
    CHT,
    ED(u16),
    EL(u16),
    IL(u16),
    DL(u16),
    EF,
    EA,
    DCH(u16),
    SSE,
    CPR,
    SU,
    SD,
    NP,
    PP,
    CTC,
    ECH(u16),
    CVT,
    CBT,
    SRS,
    PTX,
    SDS,
    SIMD,
    HPA,
    HPR,
    REP,
    DA,
    VPA(u16),
    VPR,
    HVP,
    TBC,
    SM(u8, u16),
    MC,
    HPB,
    VPB,
    RM(u8, u16),
    SGR(&'p [u16]),
    DSR(u16),
    DAQ,

    // Control Sequence (w/ a single intermediate byte 0x20)
    SL,
    SR,
    GSM,
    GSS,
    FNT,
    TSS,
    JFY,
    SPI,
    QUAD,
    SSU,
    PFS,
    SHS,
    SVS,
    IGS,
    IDCS,
    PPA,
    PPR,
    PPB,
    SPD,
    DTA,
    SHL,
    SLL,
    FNK,
    SPQR,
    SEF,
    PEC,
    SSW,
    SACS,
    SAPV,
    STAB,
    GCC,
    TATE,
    TALE,
    TAC,
    TCC,
    TSR,
    SCO,
    SRCS,
    SCS,
    SLS,
    SCP,

    // private
    SixelImage(sixel::Image),
    SelectCursorStyle(u16),
}

enum State {
    Normal,
    EscapeSeq,
    ControlSeq,

    ApplicationProgramCommand,
    DeviceControlString,
    OperatingSystemCommand,
    PrivacyMessage,
    StartOfString,
}

struct Buffer {
    // for control seqence
    params: Vec<u16>,
    intermediate: u8,
    private: Option<u8>,

    // for control string
    string: Vec<char>,
}

impl Default for Buffer {
    fn default() -> Self {
        let mut buf = Self {
            params: Vec::with_capacity(16),
            intermediate: 0,
            private: None,
            string: Vec::with_capacity(0x1000),
        };
        buf.clear();
        buf
    }
}

impl Buffer {
    fn clear(&mut self) {
        self.params.clear();
        self.params.push(0); // default value
        self.intermediate = 0;
        self.private = None;
        self.string.clear();
    }
}

fn parse_normal<'b>(state: &mut State, ch: char) -> Option<Function<'b>> {
    match ch {
        '\x00' => None,
        '\x01' => None,
        '\x02' => None,
        '\x03' => None,
        '\x04' => None,
        '\x05' => None,
        '\x06' => None,
        '\x07' => Some(Function::BEL),
        '\x08' => Some(Function::BS),
        '\x09' => Some(Function::HT),
        '\x0A' => Some(Function::LF),
        '\x0B' => Some(Function::VT),
        '\x0C' => Some(Function::FF),
        '\x0D' => Some(Function::CR),
        '\x0E' => Some(Function::SO),
        '\x0F' => Some(Function::SI),
        '\x10' => None,
        '\x11' => Some(Function::DC1),
        '\x12' => None,
        '\x13' => Some(Function::DC3),
        '\x14' => None,
        '\x15' => None,
        '\x16' => None,
        '\x17' => None,
        '\x18' => Some(Function::CAN),
        '\x19' => None,
        '\x1A' => Some(Function::SUB),
        '\x1B' => {
            *state = State::EscapeSeq;
            None
        }
        '\x7F' => None,

        _ => Some(Function::GraphicChar(ch)),
    }
}

fn parse_escape_sequence<'b>(state: &mut State, ch: char) -> Option<Function<'b>> {
    match ch {
        // Restart
        '\x1B' => None,

        '\x40' => Some(Function::Unsupported),
        '\x41' => Some(Function::Unsupported),
        '\x42' => Some(Function::BPH),
        '\x43' => Some(Function::NBH),
        '\x44' => Some(Function::Unsupported),
        '\x45' => Some(Function::NEL),
        '\x46' => Some(Function::SSA),
        '\x47' => Some(Function::ESA),
        '\x48' => Some(Function::HTS),
        '\x49' => Some(Function::HTJ),
        '\x4A' => Some(Function::VTS),
        '\x4B' => Some(Function::PLD),
        '\x4C' => Some(Function::PLU),
        '\x4D' => Some(Function::RI),
        '\x4E' => Some(Function::SS2),
        '\x4F' => Some(Function::SS3),

        // DCS
        '\x50' => {
            *state = State::DeviceControlString;
            None
        }

        '\x51' => Some(Function::PU1),
        '\x52' => Some(Function::PU2),
        '\x53' => Some(Function::STS),
        '\x54' => Some(Function::CCH),
        '\x55' => Some(Function::MW),
        '\x56' => Some(Function::SPA),
        '\x57' => Some(Function::EPA),

        // SOS
        '\x58' => {
            *state = State::StartOfString;
            None
        }

        '\x59' => Some(Function::Unsupported),
        '\x5A' => Some(Function::SCI),

        // CSI
        '\x5B' => {
            *state = State::ControlSeq;
            None
        }

        '\x5C' => Some(Function::ST),

        // OSC
        '\x5D' => {
            *state = State::OperatingSystemCommand;
            None
        }

        // PM
        '\x5E' => {
            *state = State::PrivacyMessage;
            None
        }

        // APC
        '\x5F' => {
            *state = State::ApplicationProgramCommand;
            None
        }

        // Independent control functions (ECMA-48 5th-edition 5.5)
        '\x60'..='\x7F' => Some(Function::Unsupported),

        _ => Some(Function::Invalid),
    }
}

fn parse_control_sequence<'b>(
    state: &mut State,
    buf: &'b mut Buffer,
    ch: char,
) -> Option<Function<'b>> {
    use Function::*;
    match ch {
        // Restart
        '\x1B' => {
            buf.clear();
            *state = State::EscapeSeq;
            None
        }

        // parameter sub-string
        '0'..='9' => {
            let digit = ch.to_digit(10).unwrap() as u16;
            let last_param = buf.params.last_mut().unwrap();
            *last_param = last_param.saturating_mul(10).saturating_add(digit);
            None
        }
        ':' => {
            log::warn!("a separator in a parameter sub-string is not supported");
            Some(Unsupported)
        }

        // parameter separator
        ';' => {
            buf.params.push(0);
            None
        }

        // private
        '<' | '=' | '>' | '?' => {
            buf.private = Some(ch as u8);
            None
        }

        // intermediate bytes
        '\x20'..='\x2F' => {
            buf.intermediate = ch as u8;
            None
        }

        '\x40'..='\x7E' => {
            match (buf.intermediate, ch, buf.params.as_slice()) {
                // final bytes (w/o intermediate bytes)
                (0, '\x40', &[pn]) => Some(ICH(pn)),
                (0, '\x41', &[pn]) => Some(CUU(pn)),
                (0, '\x42', &[pn]) => Some(CUD(pn)),
                (0, '\x43', &[pn]) => Some(CUF(pn)),
                (0, '\x44', &[pn]) => Some(CUB(pn)),
                (0, '\x45', _) => Some(CNL),
                (0, '\x46', _) => Some(CPL),
                (0, '\x47', &[pn]) => Some(CHA(pn)),
                (0, '\x48', &[pn1, pn2]) => Some(CUP(pn1, pn2)),
                (0, '\x48', &[pn]) => Some(CUP(pn, 1)),
                (0, '\x49', _) => Some(CHT),
                (0, '\x4A', &[ps @ (0 | 1 | 2)]) => Some(ED(ps)),
                (0, '\x4B', &[ps @ (0 | 1 | 2)]) => Some(EL(ps)),
                (0, '\x4C', &[pn]) => Some(IL(pn)),
                (0, '\x4D', &[pn]) => Some(DL(pn)),
                (0, '\x4E', _) => Some(EF),
                (0, '\x4F', _) => Some(EA),
                (0, '\x50', &[pn]) => Some(DCH(pn)),
                (0, '\x51', _) => Some(SSE),
                (0, '\x52', _) => Some(CPR),
                (0, '\x53', _) => Some(SU),
                (0, '\x54', _) => Some(SD),
                (0, '\x55', _) => Some(NP),
                (0, '\x56', _) => Some(PP),
                (0, '\x57', _) => Some(CTC),
                (0, '\x58', &[pn]) => Some(ECH(pn)),
                (0, '\x59', _) => Some(CVT),
                (0, '\x5A', _) => Some(CBT),
                (0, '\x5B', _) => Some(SRS),
                (0, '\x5C', _) => Some(PTX),
                (0, '\x5D', _) => Some(SDS),
                (0, '\x5E', _) => Some(SIMD),
                (0, '\x5F', _) => Some(Unsupported),
                (0, '\x60', _) => Some(HPA),
                (0, '\x61', _) => Some(HPR),
                (0, '\x62', _) => Some(REP),
                (0, '\x63', _) => Some(DA),
                (0, '\x64', &[pn]) => Some(VPA(pn)),
                (0, '\x65', _) => Some(VPR),
                (0, '\x66', _) => Some(HVP),
                (0, '\x67', _) => Some(TBC),
                (0, '\x68', &[ps]) => {
                    let private = buf.private.unwrap_or(0);
                    Some(SM(private, ps))
                }
                (0, '\x69', _) => Some(MC),
                (0, '\x6A', _) => Some(HPB),
                (0, '\x6B', _) => Some(VPB),
                (0, '\x6C', &[ps]) => {
                    let private = buf.private.unwrap_or(0);
                    Some(RM(private, ps))
                }
                (0, '\x6D', ps) => Some(SGR(ps)),
                (0, '\x6E', &[ps @ (5 | 6)]) => Some(DSR(ps)),
                (0, '\x6F', _) => Some(DAQ),
                (0, '\x70'..='\x7E', params) => {
                    log::trace!(
                        "undefined private sequence: i=N/A, final=0x{:X}, params={:?}",
                        ch as u8,
                        params
                    );
                    Some(Unsupported)
                }

                // final bytes (w/ a single intermediate bytes \x20)
                (b'\x20', '\x40', _) => Some(SL),
                (b'\x20', '\x41', _) => Some(SR),
                (b'\x20', '\x42', _) => Some(GSM),
                (b'\x20', '\x43', _) => Some(GSS),
                (b'\x20', '\x44', _) => Some(FNT),
                (b'\x20', '\x45', _) => Some(TSS),
                (b'\x20', '\x46', _) => Some(JFY),
                (b'\x20', '\x47', _) => Some(SPI),
                (b'\x20', '\x48', _) => Some(QUAD),
                (b'\x20', '\x49', _) => Some(SSU),
                (b'\x20', '\x4A', _) => Some(PFS),
                (b'\x20', '\x4B', _) => Some(SHS),
                (b'\x20', '\x4C', _) => Some(SVS),
                (b'\x20', '\x4D', _) => Some(IGS),
                (b'\x20', '\x4E', _) => Some(Unsupported),
                (b'\x20', '\x4F', _) => Some(IDCS),
                (b'\x20', '\x50', _) => Some(PPA),
                (b'\x20', '\x51', _) => Some(PPR),
                (b'\x20', '\x52', _) => Some(PPB),
                (b'\x20', '\x53', _) => Some(SPD),
                (b'\x20', '\x54', _) => Some(DTA),
                (b'\x20', '\x55', _) => Some(SHL),
                (b'\x20', '\x56', _) => Some(SLL),
                (b'\x20', '\x57', _) => Some(FNK),
                (b'\x20', '\x58', _) => Some(SPQR),
                (b'\x20', '\x59', _) => Some(SEF),
                (b'\x20', '\x5A', _) => Some(PEC),
                (b'\x20', '\x5B', _) => Some(SSW),
                (b'\x20', '\x5C', _) => Some(SACS),
                (b'\x20', '\x5D', _) => Some(SAPV),
                (b'\x20', '\x5E', _) => Some(STAB),
                (b'\x20', '\x5F', _) => Some(GCC),
                (b'\x20', '\x60', _) => Some(TATE),
                (b'\x20', '\x61', _) => Some(TALE),
                (b'\x20', '\x62', _) => Some(TAC),
                (b'\x20', '\x63', _) => Some(TCC),
                (b'\x20', '\x64', _) => Some(TSR),
                (b'\x20', '\x65', _) => Some(SCO),
                (b'\x20', '\x66', _) => Some(SRCS),
                (b'\x20', '\x67', _) => Some(SCS),
                (b'\x20', '\x68', _) => Some(SLS),
                (b'\x20', '\x69', _) => Some(Unsupported),
                (b'\x20', '\x6A', _) => Some(Unsupported),
                (b'\x20', '\x6B', _) => Some(SCP),
                (b'\x20', '\x6C', _) => Some(Unsupported),
                (b'\x20', '\x6D', _) => Some(Unsupported),
                (b'\x20', '\x6E', _) => Some(Unsupported),
                (b'\x20', '\x6F', _) => Some(Unsupported),

                // private sequences
                (b'\x20', '\x71', &[ps]) => Some(SelectCursorStyle(ps)),
                (b'\x20', '\x70'..='\x7E', params) => {
                    log::trace!(
                        "undefined private sequence: i=0x20, final=0x{:X}, params={:?}",
                        ch as u8,
                        params,
                    );
                    Some(Unsupported)
                }

                (i @ b'\x21'..=b'\x2F', '\x40'..='\x7E', params) => {
                    log::trace!(
                        "unsupported control sequence: i=0x{:X}, final=0x{:X}, params={:?}",
                        i,
                        ch as u8,
                        params,
                    );
                    Some(Unsupported)
                }

                _ => Some(Invalid),
            }
        }

        _ => {
            log::warn!("invalid control sequence");
            Some(Invalid)
        }
    }
}

fn parse_control_string<'b>(
    state: &mut State,
    buf: &'b mut Buffer,
    sixel_parser: &mut sixel::Parser,
    ch: char,
) -> Option<Function<'b>> {
    // ST - STRING TERMINATOR
    if let (Some('\x1B'), '\x5C') = (buf.string.last(), ch) {
        buf.string.pop();

        match state {
            State::ApplicationProgramCommand => {
                log::trace!("application program command: {:?}", buf.string);
                Some(Function::Unsupported)
            }

            State::DeviceControlString => {
                log::trace!("device control string: {:?}", buf.string);
                match buf.string.get(0) {
                    Some('q') => {
                        // Sixel Sequence
                        let mut chars = buf.string[1..].iter().copied();
                        let image = sixel_parser.decode(&mut chars);
                        Some(Function::SixelImage(image))
                    }
                    _ => Some(Function::Unsupported),
                }
            }

            State::OperatingSystemCommand => {
                log::trace!("operating system command: {:?}", buf.string);
                Some(Function::Unsupported)
            }

            State::PrivacyMessage => {
                log::trace!("privacy message: {:?}", buf.string);
                Some(Function::Unsupported)
            }

            _ => unreachable!(),
        }
    } else if let '\x08'..='\x0D' | '\x1B' | '\x20'..='\x7E' = ch {
        buf.string.push(ch);
        None
    } else {
        Some(Function::Invalid)
    }
}

fn parse_character_string<'b>(
    _: &mut State,
    buf: &'b mut Buffer,
    ch: char,
) -> Option<Function<'b>> {
    // ST - STRING TERMINATOR
    if let (Some('\x1B'), '\x5C') = (buf.string.last(), ch) {
        buf.string.pop();
        log::trace!("character string: {:?}", buf.string);
        Some(Function::Unsupported)
    } else {
        buf.string.push(ch);
        None
    }
}

pub struct Parser {
    state: State,
    buf: Buffer,
    sixel_parser: sixel::Parser,
}

impl Parser {
    pub fn feed(&mut self, ch: char) -> Option<Function> {
        let func = match self.state {
            State::Normal => {
                self.buf.clear();
                parse_normal(&mut self.state, ch)
            }
            State::EscapeSeq => parse_escape_sequence(&mut self.state, ch),
            State::ControlSeq => parse_control_sequence(&mut self.state, &mut self.buf, ch),

            State::ApplicationProgramCommand
            | State::DeviceControlString
            | State::OperatingSystemCommand
            | State::PrivacyMessage => {
                parse_control_string(&mut self.state, &mut self.buf, &mut self.sixel_parser, ch)
            }
            State::StartOfString => parse_character_string(&mut self.state, &mut self.buf, ch),
        };

        if func.is_some() {
            self.state = State::Normal;
        }

        func
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            state: State::Normal,
            buf: Buffer::default(),
            sixel_parser: sixel::Parser::new(),
        }
    }
}

//! Nur fuer Tests: prueft ohne externe Pakete, ob ein Text gueltiges JSON ist.

/// Minimaler JSON-Pruefer (rekursiver Abstieg), damit die Tests ohne externe Pakete
/// sicherstellen, dass das Ergebnis gueltiges JSON ist.
struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && b" \n\r\t".contains(&self.s[self.i]) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("'{}' erwartet bei {}", c as char, self.i))
        }
    }
    fn value(&mut self) -> Result<(), String> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                self.ws();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(());
                }
                loop {
                    self.ws();
                    self.string()?;
                    self.eat(b':')?;
                    self.value()?;
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(());
                        }
                        _ => return Err(format!("',' oder '}}' erwartet bei {}", self.i)),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                self.ws();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(());
                }
                loop {
                    self.value()?;
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(());
                        }
                        _ => return Err(format!("',' oder ']' erwartet bei {}", self.i)),
                    }
                }
            }
            Some(b'"') => self.string(),
            Some(b't') => self.word("true"),
            Some(b'f') => self.word("false"),
            Some(b'n') => self.word("null"),
            Some(c) if *c == b'-' || c.is_ascii_digit() => {
                self.i += 1;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.')
                {
                    self.i += 1;
                }
                Ok(())
            }
            _ => Err(format!("Wert erwartet bei {}", self.i)),
        }
    }
    fn word(&mut self, w: &str) -> Result<(), String> {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(())
        } else {
            Err(format!("'{w}' erwartet bei {}", self.i))
        }
    }
    fn string(&mut self) -> Result<(), String> {
        self.eat(b'"')?;
        while let Some(&c) = self.s.get(self.i) {
            self.i += 1;
            match c {
                b'"' => return Ok(()),
                b'\\' => self.i += 1,
                c if c < 0x20 => return Err("Steuerzeichen im String".into()),
                _ => {}
            }
        }
        Err("String nicht abgeschlossen".into())
    }
}

pub(crate) fn validate(json: &str) -> Result<(), String> {
    let mut p = Parser {
        s: json.as_bytes(),
        i: 0,
    };
    p.value()?;
    p.ws();
    if p.i == json.len() {
        Ok(())
    } else {
        Err(format!("Rest nach dem Dokument ab {}", p.i))
    }
}

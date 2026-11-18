//! TC6 XML for tests, written element by element as an LD editor would write it.
//!
//! Connections are named as `"7"` (the one output of element 7) or `"7.Q"` (the output Q of
//! block 7). Positions are given; the front-end orders by them.
#![allow(dead_code)]

/// Escape text for an attribute or element.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A `connectionPointIn` with a connection for each of `from`.
pub fn conns(from: &[&str]) -> String {
    let mut s = String::from("<connectionPointIn>");
    for f in from {
        match f.split_once('.') {
            Some((id, pin)) => s.push_str(&format!(
                "<connection refLocalId=\"{id}\" formalParameter=\"{pin}\"/>"
            )),
            None => s.push_str(&format!("<connection refLocalId=\"{f}\"/>")),
        }
    }
    s.push_str("</connectionPointIn>");
    s
}

fn pos(x: i32, y: i32) -> String {
    format!("<position x=\"{x}\" y=\"{y}\"/>")
}

/// What reaches a block's input.
pub enum In<'a> {
    From(&'a [&'a str]),
    Expr(&'a str),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Contact {
    No,
    Nc,
    Rising,
    Falling,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Coil {
    Plain,
    Negated,
    Set,
    Reset,
    Rising,
    Falling,
}

pub struct Pou {
    name: String,
    kind: &'static str,
    result: Option<String>,
    sections: Vec<(String, String)>,
    body: String,
}

impl Pou {
    fn new(name: &str, kind: &'static str) -> Pou {
        Pou {
            name: name.into(),
            kind,
            result: None,
            sections: Vec::new(),
            body: String::new(),
        }
    }
    pub fn program(name: &str) -> Pou {
        Pou::new(name, "program")
    }
    pub fn function_block(name: &str) -> Pou {
        Pou::new(name, "functionBlock")
    }
    pub fn function(name: &str, result: &str) -> Pou {
        let mut p = Pou::new(name, "function");
        p.result = Some(result.into());
        p
    }

    /// A section (`localVars`, `inputVars`, `outputVars`, with attributes such as
    /// `retain="true"`) of variables `(name, type XML, initial value)`.
    pub fn section(mut self, tag: &str, vars: &[(&str, &str, Option<&str>)]) -> Pou {
        let mut s = String::new();
        for (name, ty, init) in vars {
            s.push_str(&format!(
                "<variable name=\"{name}\"><type>{}</type>",
                ty_xml(ty)
            ));
            if let Some(v) = init {
                s.push_str(&format!(
                    "<initialValue><simpleValue value=\"{}\"/></initialValue>",
                    esc(v)
                ));
            }
            s.push_str("</variable>");
        }
        self.sections.push((tag.into(), s));
        self
    }
    pub fn vars(self, vars: &[(&str, &str)]) -> Pou {
        let v: Vec<_> = vars.iter().map(|(n, t)| (*n, *t, None)).collect();
        self.section("localVars", &v)
    }
    pub fn inputs(self, vars: &[(&str, &str)]) -> Pou {
        let v: Vec<_> = vars.iter().map(|(n, t)| (*n, *t, None)).collect();
        self.section("inputVars", &v)
    }
    pub fn outputs(self, vars: &[(&str, &str)]) -> Pou {
        let v: Vec<_> = vars.iter().map(|(n, t)| (*n, *t, None)).collect();
        self.section("outputVars", &v)
    }
    /// Raw XML into the interface.
    pub fn interface_xml(mut self, xml: &str) -> Pou {
        self.sections.push((String::new(), xml.into()));
        self
    }

    /// Raw XML into the body.
    pub fn raw(mut self, xml: &str) -> Pou {
        self.body.push_str(xml);
        self
    }
    pub fn left(self, id: u64, y: i32) -> Pou {
        let x = format!(
            "<leftPowerRail localId=\"{id}\">{}<connectionPointOut formalParameter=\"\"/></leftPowerRail>",
            pos(0, y)
        );
        self.raw(&x)
    }
    pub fn right(self, id: u64, y: i32, from: &[&str]) -> Pou {
        let mut x = format!("<rightPowerRail localId=\"{id}\">{}", pos(1000, y));
        for f in from {
            x.push_str(&conns(&[f]));
        }
        x.push_str("</rightPowerRail>");
        self.raw(&x)
    }
    pub fn contact(self, id: u64, x: i32, y: i32, kind: Contact, var: &str, from: &[&str]) -> Pou {
        let attrs = match kind {
            Contact::No => "",
            Contact::Nc => " negated=\"true\"",
            Contact::Rising => " edge=\"rising\"",
            Contact::Falling => " edge=\"falling\"",
        };
        let e = format!(
            "<contact localId=\"{id}\"{attrs}>{}{}<connectionPointOut/><variable>{}</variable></contact>",
            pos(x, y),
            conns(from),
            esc(var)
        );
        self.raw(&e)
    }
    pub fn coil(self, id: u64, x: i32, y: i32, kind: Coil, var: &str, from: &[&str]) -> Pou {
        let attrs = match kind {
            Coil::Plain => "",
            Coil::Negated => " negated=\"true\"",
            Coil::Set => " storage=\"set\"",
            Coil::Reset => " storage=\"reset\"",
            Coil::Rising => " edge=\"rising\"",
            Coil::Falling => " edge=\"falling\"",
        };
        let e = format!(
            "<coil localId=\"{id}\"{attrs}>{}{}<connectionPointOut/><variable>{}</variable></coil>",
            pos(x, y),
            conns(from),
            esc(var)
        );
        self.raw(&e)
    }
    pub fn invar(self, id: u64, x: i32, y: i32, expr: &str) -> Pou {
        let e = format!(
            "<inVariable localId=\"{id}\">{}<connectionPointOut/><expression>{}</expression></inVariable>",
            pos(x, y),
            esc(expr)
        );
        self.raw(&e)
    }
    pub fn outvar(self, id: u64, x: i32, y: i32, expr: &str, from: &[&str]) -> Pou {
        let e = format!(
            "<outVariable localId=\"{id}\">{}{}<expression>{}</expression></outVariable>",
            pos(x, y),
            conns(from),
            esc(expr)
        );
        self.raw(&e)
    }
    /// A block calling `type_name`, for an instance or (without one) as a function.
    #[allow(clippy::too_many_arguments)]
    pub fn block(
        self,
        id: u64,
        x: i32,
        y: i32,
        type_name: &str,
        instance: Option<&str>,
        inputs: &[(&str, In)],
        outputs: &[&str],
    ) -> Pou {
        let inst = instance
            .map(|i| format!(" instanceName=\"{}\"", esc(i)))
            .unwrap_or_default();
        let mut e = format!(
            "<block localId=\"{id}\" typeName=\"{type_name}\"{inst}>{}<inputVariables>",
            pos(x, y)
        );
        for (formal, input) in inputs {
            e.push_str(&format!("<variable formalParameter=\"{formal}\">"));
            match input {
                In::From(from) => e.push_str(&conns(from)),
                In::Expr(text) => e.push_str(&format!(
                    "<connectionPointIn><expression>{}</expression></connectionPointIn>",
                    esc(text)
                )),
            }
            e.push_str("</variable>");
        }
        e.push_str("</inputVariables><inOutVariables/><outputVariables>");
        for o in outputs {
            e.push_str(&format!(
                "<variable formalParameter=\"{o}\"><connectionPointOut/></variable>"
            ));
        }
        e.push_str("</outputVariables></block>");
        self.raw(&e)
    }
    pub fn label(self, id: u64, y: i32, name: &str) -> Pou {
        let e = format!(
            "<label localId=\"{id}\" label=\"{name}\">{}</label>",
            pos(0, y)
        );
        self.raw(&e)
    }
    pub fn jump(self, id: u64, x: i32, y: i32, label: &str, from: &[&str]) -> Pou {
        let e = format!(
            "<jump localId=\"{id}\" label=\"{label}\">{}{}</jump>",
            pos(x, y),
            conns(from)
        );
        self.raw(&e)
    }
    pub fn ret(self, id: u64, x: i32, y: i32, from: &[&str]) -> Pou {
        let e = format!(
            "<return localId=\"{id}\">{}{}</return>",
            pos(x, y),
            conns(from)
        );
        self.raw(&e)
    }
    pub fn comment(self, id: u64, x: i32, y: i32, text: &str) -> Pou {
        let e = format!(
            "<comment localId=\"{id}\" height=\"20\" width=\"100\">{}<content><xhtml:p xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">{}</xhtml:p></content></comment>",
            pos(x, y),
            esc(text)
        );
        self.raw(&e)
    }

    pub fn xml(&self) -> String {
        let mut s = format!(
            "<pou name=\"{}\" pouType=\"{}\"><interface>",
            self.name, self.kind
        );
        if let Some(r) = &self.result {
            s.push_str(&format!("<returnType>{}</returnType>", ty_xml(r)));
        }
        for (tag, vars) in &self.sections {
            if tag.is_empty() {
                s.push_str(vars);
                continue;
            }
            let (open, close) = match tag.split_once(' ') {
                Some((t, _)) => (tag.clone(), t.to_string()),
                None => (tag.clone(), tag.clone()),
            };
            s.push_str(&format!("<{open}>{vars}</{close}>"));
        }
        s.push_str(&format!(
            "</interface><body><LD>{}</LD></body></pou>",
            self.body
        ));
        s
    }
}

/// A type as TC6 writes it: an elementary name, `STRING[n]`, `ARRAY[lo..hi] OF T`, or a
/// derived name.
pub fn ty_xml(ty: &str) -> String {
    const ELEMENTARY: &[&str] = &[
        "BOOL", "SINT", "INT", "DINT", "LINT", "REAL", "LREAL", "TIME", "BYTE", "WORD", "USINT",
    ];
    if let Some(rest) = ty.strip_prefix("ARRAY[") {
        let (range, elem) = rest.split_once("] OF ").expect("ARRAY[lo..hi] OF T");
        let (lo, hi) = range.split_once("..").expect("lo..hi");
        return format!(
            "<array><dimension lower=\"{lo}\" upper=\"{hi}\"/><baseType>{}</baseType></array>",
            ty_xml(elem)
        );
    }
    if let Some(n) = ty.strip_prefix("STRING[").and_then(|r| r.strip_suffix(']')) {
        return format!("<string length=\"{n}\"/>");
    }
    if ty == "STRING" {
        return "<string/>".into();
    }
    if ELEMENTARY.contains(&ty) {
        return format!("<{ty}/>");
    }
    format!("<derived name=\"{ty}\"/>")
}

/// A TC6 v2.01 project holding `pous`.
pub fn project(pous: &[Pou]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\"><fileHeader companyName=\"MythOS tests\" productName=\"tests\" productVersion=\"1\" creationDateTime=\"2026-11-18T00:00:00\"/><contentHeader name=\"tests\"><coordinateInfo><fbd><scaling x=\"1\" y=\"1\"/></fbd><ld><scaling x=\"1\" y=\"1\"/></ld><sfc><scaling x=\"1\" y=\"1\"/></sfc></coordinateInfo></contentHeader><types><dataTypes/><pous>",
    );
    for p in pous {
        s.push_str(&p.xml());
    }
    s.push_str("</pous></types><instances><configurations/></instances></project>");
    s
}

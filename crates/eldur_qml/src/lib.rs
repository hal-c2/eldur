//! A QML document model and its writer.
//!
//! The converter builds a [`Document`] tree; [`Document::to_qml`] prints it
//! as formatted QML (4-space indent, the layout `qmlformat` produces) so
//! output is diffable and stable. Nothing here knows about HTML or CSS.

mod writer;

pub use writer::{quote_string, write_document};

/// `import QtQuick` / `import QtQuick.Controls.Basic` / `import "dom.js" as Dom`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Import {
    /// Module URI (`QtQuick.Layouts`) or a quoted path (`"dom.js"`), as written.
    pub uri: String,
    /// `None` imports the latest version (Qt 6 style).
    pub version: Option<String>,
    pub alias: Option<String>,
}

impl Import {
    #[must_use]
    pub fn module(uri: &str) -> Self {
        Self {
            uri: uri.to_owned(),
            version: None,
            alias: None,
        }
    }

    /// `import "path" as Alias` (a JavaScript resource or a directory).
    #[must_use]
    pub fn path(path: &str, alias: Option<&str>) -> Self {
        Self {
            uri: quote_string(path),
            version: None,
            alias: alias.map(str::to_owned),
        }
    }
}

/// A complete `.qml` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    /// Leading comment lines (without `//`), e.g. provenance.
    pub header: Vec<String>,
    /// `pragma ComponentBehavior: Bound` and friends (text after `pragma `).
    pub pragmas: Vec<String>,
    /// Printed sorted and de-duplicated.
    pub imports: Vec<Import>,
    pub root: Object,
}

impl Document {
    #[must_use]
    pub fn new(root: Object) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }

    pub fn import(&mut self, import: Import) {
        if !self.imports.contains(&import) {
            self.imports.push(import);
        }
    }

    #[must_use]
    pub fn to_qml(&self) -> String {
        write_document(self)
    }
}

/// A QML value on the right of `name:`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A JavaScript expression or binding, printed verbatim
    /// (`parent.width * 0.5`, `Text.AlignHCenter`, `root.count > 0`).
    Expr(String),
    /// A string literal; the writer quotes and escapes it.
    String(String),
    /// Printed in the shortest form that round-trips; integers without `.0`.
    Number(f64),
    Bool(bool),
    /// A colour, printed as a string literal (`"#80ff0000"`, ARGB order).
    Color(Color),
    /// `[a, b]` of values (e.g. `transform: [Rotation {...}, Scale {...}]`).
    List(Vec<Value>),
    /// An inline object value (`font: Font {..}` is not valid QML, but
    /// `transform: Translate { x: 4 }` and `background: Rectangle {..}` are).
    Object(Box<Object>),
    /// A multi-statement block printed as `{ ... }` (for handlers/bindings
    /// that need statements). Lines are printed verbatim, re-indented.
    Block(Vec<String>),
}

impl Value {
    #[must_use]
    pub fn expr(s: impl Into<String>) -> Self {
        Self::Expr(s.into())
    }

    #[must_use]
    pub fn string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Number(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<Color> for Value {
    fn from(v: Color) -> Self {
        Self::Color(v)
    }
}

/// sRGB colour with straight (non-premultiplied) alpha, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgba(0, 0, 0, 255);
    pub const WHITE: Self = Self::rgba(255, 255, 255, 255);

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// QML colour string: `#rrggbb` when opaque, `#aarrggbb` otherwise
    /// (QML puts alpha first, unlike CSS `#rrggbbaa`).
    #[must_use]
    pub fn to_qml_string(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.a, self.r, self.g, self.b)
        }
    }
}

/// A member of an object body, printed in this order of kinds by the
/// writer: id, property declarations, signals, plain/attached property
/// assignments, functions, handlers, child objects. Within a kind, the
/// order given is kept.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    /// `property <ty> <name>: <value>` (value optional). `readonly` and
    /// `required` are flags.
    PropertyDecl {
        ty: String,
        name: String,
        value: Option<Value>,
        readonly: bool,
        required: bool,
    },
    /// `signal name(type a, type b)`; params are (type, name).
    Signal {
        name: String,
        params: Vec<(String, String)>,
    },
    /// `name: value`. Dotted names (`anchors.fill`, `Layout.fillWidth`,
    /// `font.pixelSize`, `border.width`) are allowed; the writer groups
    /// nothing, it prints each on its own line.
    Property { name: String, value: Value },
    /// `function name(a, b) { body }`; body lines printed verbatim, re-indented.
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<String>,
    },
    /// `onClicked: expr` or `onClicked: (mouse) => { ... }`. `name` includes
    /// the `on` prefix and may be dotted (`Keys.onPressed`).
    Handler {
        name: String,
        params: Vec<String>,
        body: Vec<String>,
    },
    /// `Behavior on opacity { NumberAnimation { duration: 200 } }`.
    Behavior { property: String, animation: Object },
    /// A child object.
    Child(Object),
    /// `// text` (one line per entry), kept where it is among children.
    Comment(String),
}

/// `TypeName { members }`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Object {
    pub type_name: String,
    pub id: Option<String>,
    pub members: Vec<Member>,
}

impl Object {
    #[must_use]
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            id: None,
            members: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets `name: value`, replacing an earlier assignment to the same name.
    pub fn set(&mut self, name: &str, value: impl Into<Value>) -> &mut Self {
        let value = value.into();
        for m in &mut self.members {
            if let Member::Property { name: n, value: v } = m
                && n == name
            {
                *v = value;
                return self;
            }
        }
        self.members.push(Member::Property {
            name: name.to_owned(),
            value,
        });
        self
    }

    /// Builder form of [`Object::set`].
    #[must_use]
    pub fn prop(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.set(name, value);
        self
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.members.iter().find_map(|m| match m {
            Member::Property { name: n, value } if n == name => Some(value),
            _ => None,
        })
    }

    pub fn push_child(&mut self, child: Object) -> &mut Self {
        self.members.push(Member::Child(child));
        self
    }

    #[must_use]
    pub fn child(mut self, child: Object) -> Self {
        self.push_child(child);
        self
    }

    pub fn children(&self) -> impl Iterator<Item = &Object> {
        self.members.iter().filter_map(|m| match m {
            Member::Child(o) => Some(o),
            _ => None,
        })
    }

    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut Object> {
        self.members.iter_mut().filter_map(|m| match m {
            Member::Child(o) => Some(o),
            _ => None,
        })
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.to_owned())
    }
}

/// Turns arbitrary text (an HTML id, a class name) into a valid, unique-able
/// QML id: lower camel case ASCII, starts with a lower-case letter, not a
/// JavaScript or QML reserved word. Uniqueness is the caller's job.
#[must_use]
pub fn sanitize_id(raw: &str) -> String {
    let mut out = String::new();
    let mut upper_next = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            if out.is_empty() {
                if c.is_ascii_digit() {
                    out.push_str("id");
                    out.push(c);
                } else {
                    out.push(c.to_ascii_lowercase());
                }
            } else if upper_next {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c);
            }
            upper_next = false;
        } else {
            upper_next = !out.is_empty();
        }
    }
    if out.is_empty() {
        out.push_str("item");
    }
    if is_reserved(&out) {
        out.push('_');
    }
    out
}

/// JavaScript reserved words and names QML gives a meaning in object bodies.
#[must_use]
pub fn is_reserved(word: &str) -> bool {
    const RESERVED: &[&str] = &[
        // Words ECMAScript once kept for later: Qt's lexer still refuses them
        // as ids, though a page may name an element `double`.
        "abstract",
        "boolean",
        "byte",
        "char",
        "double",
        "final",
        "float",
        "goto",
        "int",
        "long",
        "native",
        "short",
        "synchronized",
        "throws",
        "transient",
        "volatile",
        "arguments",
        "as",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "eval",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "package",
        "parent",
        "pragma",
        "private",
        "property",
        "protected",
        "public",
        "readonly",
        "required",
        "return",
        "signal",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "undefined",
        "var",
        "void",
        "while",
        "with",
        "yield",
        "id",
        "on",
        "component",
        "enum",
        "root",
    ];
    RESERVED.contains(&word)
}

#[cfg(test)]
mod tests;

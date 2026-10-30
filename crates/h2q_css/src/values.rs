//! Computed values: what the cascade hands the converter.
//!
//! These are CSS *computed* values (CSS Cascade 4 §4.4): `em`, `rem`,
//! `ex`, `ch`, absolute units, keywords like `bolder`, `currentColor` and
//! `var()` are resolved; percentages and viewport units are kept, because
//! in QML they become bindings to the parent's or the window's size.

use std::collections::BTreeMap;

/// sRGB colour, straight alpha, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);

    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[must_use]
    pub fn is_transparent(self) -> bool {
        self.a == 0
    }
}

/// A linear combination `px + percent% + vw + vh`, which is what every
/// `calc()` of lengths and percentages without `min()`/`max()` reduces to.
/// `percent` is relative to whatever the property says (usually the
/// containing block's width).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LengthPercentage {
    pub px: f32,
    pub percent: f32,
    pub vw: f32,
    pub vh: f32,
}

impl LengthPercentage {
    pub const ZERO: Self = Self::px(0.0);

    #[must_use]
    pub const fn px(px: f32) -> Self {
        Self {
            px,
            percent: 0.0,
            vw: 0.0,
            vh: 0.0,
        }
    }

    #[must_use]
    pub const fn percent(percent: f32) -> Self {
        Self {
            px: 0.0,
            percent,
            vw: 0.0,
            vh: 0.0,
        }
    }

    #[must_use]
    pub const fn vw(vw: f32) -> Self {
        Self {
            px: 0.0,
            percent: 0.0,
            vw,
            vh: 0.0,
        }
    }

    #[must_use]
    pub const fn vh(vh: f32) -> Self {
        Self {
            px: 0.0,
            percent: 0.0,
            vw: 0.0,
            vh,
        }
    }

    /// Multiplies every component by a scalar factor.
    #[must_use]
    pub fn scale(self, factor: f32) -> Self {
        Self {
            px: self.px * factor,
            percent: self.percent * factor,
            vw: self.vw * factor,
            vh: self.vh * factor,
        }
    }

    /// `Some(px)` when the value is a plain length.
    #[must_use]
    pub fn as_px(&self) -> Option<f32> {
        (self.percent == 0.0 && self.vw == 0.0 && self.vh == 0.0).then_some(self.px)
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.as_px() == Some(0.0)
    }
}

impl std::ops::Add for LengthPercentage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            px: self.px + other.px,
            percent: self.percent + other.percent,
            vw: self.vw + other.vw,
            vh: self.vh + other.vh,
        }
    }
}

/// `min()`, `max()`, `clamp()` and anything else that is not linear, kept as
/// a tree for the converter to print as `Math.min(...)` bindings.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcNode {
    Leaf(LengthPercentage),
    Min(Vec<CalcNode>),
    Max(Vec<CalcNode>),
    /// `clamp(min, val, max)`.
    Clamp(Box<CalcNode>, Box<CalcNode>, Box<CalcNode>),
    Sum(Vec<CalcNode>),
    /// A node times a unitless number.
    Product(Box<CalcNode>, f32),
}

/// `<length-percentage>` possibly with non-linear `calc()`.
#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    Linear(LengthPercentage),
    Calc(CalcNode),
}

impl Default for Length {
    fn default() -> Self {
        Self::Linear(LengthPercentage::ZERO)
    }
}

impl Length {
    #[must_use]
    pub fn px(px: f32) -> Self {
        Self::Linear(LengthPercentage::px(px))
    }

    #[must_use]
    pub fn as_px(&self) -> Option<f32> {
        match self {
            Self::Linear(l) => l.as_px(),
            Self::Calc(_) => None,
        }
    }
}

/// `<length-percentage> | auto` (margins, insets, flex-basis's length part).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LengthAuto {
    #[default]
    Auto,
    Length(Length),
}

/// `width`, `height`, `min-*`: plus intrinsic keywords. `max-*` use `None`.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Size {
    #[default]
    Auto,
    /// Only meaningful for `max-width`/`max-height`.
    None,
    Length(Length),
    MinContent,
    MaxContent,
    FitContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    Block,
    #[default]
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    FlowRoot,
    ListItem,
    Table,
    InlineTable,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
    TableColumnGroup,
    TableColumn,
    TableCaption,
    /// The element generates no box; its children are laid out as if they
    /// were its parent's.
    Contents,
    None,
}

impl Display {
    /// Block-level outer display (takes the full line in normal flow).
    #[must_use]
    pub fn is_block_level(self) -> bool {
        matches!(
            self,
            Self::Block
                | Self::Flex
                | Self::Grid
                | Self::FlowRoot
                | Self::ListItem
                | Self::Table
                | Self::TableCaption
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

/// One side of a border. `width` is already 0 when `style` is none/hidden
/// (computed-value rule).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderSide {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Rgba,
    /// Explicit `<color>` when set (`None` = initial `currentColor`).
    pub explicit_color: Option<Rgba>,
    /// Explicit `<line-width>` when set (`None` = initial `medium`, 3px).
    pub explicit_width: Option<f32>,
}

/// Top, right, bottom, left (CSS shorthand order).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sides<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Clone> Sides<T> {
    #[must_use]
    pub fn all(v: T) -> Self {
        Self {
            top: v.clone(),
            right: v.clone(),
            bottom: v.clone(),
            left: v,
        }
    }
}

impl<T: PartialEq> Sides<T> {
    #[must_use]
    pub fn is_uniform(&self) -> bool {
        self.top == self.right && self.right == self.bottom && self.bottom == self.left
    }
}

/// Top-left, top-right, bottom-right, bottom-left. Elliptical radii are
/// approximated by their horizontal radius (QML Rectangle has no ellipses).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Corners<T = LengthPercentage> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// `justify-content`, `align-content`, and with `Auto`/`Stretch`/`Baseline`
/// also `align-items`/`align-self`/`justify-items`. `Normal` is kept so the
/// converter can apply each layout mode's own default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Normal,
    /// `align-self: auto`.
    Auto,
    Start,
    End,
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Left,
    Right,
}

/// One track of `grid-template-columns`/`rows` after `repeat(<integer>, …)`
/// is expanded. `repeat(auto-fill|auto-fit, …)` is kept as `Repeat`.
#[derive(Debug, Clone, PartialEq)]
pub enum Track {
    Length(Length),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
    MinMax(Box<Track>, Box<Track>),
    FitContent(Length),
    AutoRepeat { fill: bool, tracks: Vec<Track> },
}

/// `grid-column` / `grid-row` start or end.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GridLine {
    #[default]
    Auto,
    /// 1-based line number; negative counts from the end.
    Line(i32),
    Span(u32),
    /// A named line or area.
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineHeight {
    #[default]
    Normal,
    /// Unitless multiplier (inherited as the number).
    Number(f32),
    /// Lengths and percentages are computed to px.
    Px(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Values are kept as written (`start` → `Start`, `left` → `Left`); the
    /// converter resolves them against `direction`.
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

/// `text-decoration-line` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextDecorationLine {
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Sub,
    Super,
    Top,
    TextTop,
    Middle,
    Bottom,
    TextBottom,
    Length(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
    pub inset: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    pub color: Rgba,
    /// Position along the gradient line; `None` = spread evenly
    /// (the converter fixes these up per CSS Images 3 §3.5.1).
    pub position: Option<LengthPercentage>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Image {
    /// URL as written, unresolved; the converter resolves it against the
    /// stylesheet's base URL it gets with the declaration's origin.
    Url(String),
    /// Angle in degrees, CSS convention (0 = to top, 90 = to right).
    LinearGradient {
        angle_deg: f32,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
    RadialGradient {
        /// `circle` vs `ellipse`.
        circle: bool,
        /// Centre, as percentages of the box.
        center_x: LengthPercentage,
        center_y: LengthPercentage,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
    ConicGradient {
        from_deg: f32,
        center_x: LengthPercentage,
        center_y: LengthPercentage,
        stops: Vec<ColorStop>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum BackgroundSize {
    #[default]
    Auto,
    Cover,
    Contain,
    Explicit(LengthAuto, LengthAuto),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundRepeat {
    #[default]
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
    Space,
    Round,
}

/// One background layer (`background-image` and its parallel lists).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BackgroundLayer {
    pub image: Option<Image>,
    pub size: BackgroundSize,
    pub position_x: LengthPercentage,
    pub position_y: LengthPercentage,
    pub repeat: BackgroundRepeat,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransformFn {
    Translate(LengthPercentage, LengthPercentage),
    /// A translation with a non-linear component (`min()`/`max()`/`clamp()`
    /// over percentages), kept as an expression for a QML binding.
    TranslateCalc(Length, Length),
    Scale(f32, f32),
    /// Degrees, clockwise.
    Rotate(f32),
    Skew(f32, f32),
    Matrix([f32; 6]),
    /// 3-D functions are flattened when possible, else kept here.
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerEvents {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Auto,
    Default,
    Pointer,
    Text,
    Move,
    NotAllowed,
    Wait,
    Help,
    Crosshair,
    Grab,
    Grabbing,
    ResizeEw,
    ResizeNs,
    ResizeNesw,
    ResizeNwse,
    /// Anything else, by name.
    Other,
}

/// `cubic-bezier(x1, y1, x2, y2)`; keywords are expanded (`ease` etc.).
/// `steps()` is kept as `Steps`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    CubicBezier(f32, f32, f32, f32),
    Steps(u32, bool),
    Linear,
}

impl Default for Easing {
    fn default() -> Self {
        // `ease`
        Self::CubicBezier(0.25, 0.1, 0.25, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transition {
    /// Property name, or `all`.
    pub property: String,
    pub duration_ms: f32,
    pub delay_ms: f32,
    pub easing: Easing,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Animation {
    /// Name of an `@keyframes` rule; look it up in [`crate::StyleMap::keyframes`].
    pub name: String,
    pub duration_ms: f32,
    pub delay_ms: f32,
    pub easing: Easing,
    /// `f32::INFINITY` for `infinite`.
    pub iteration_count: f32,
    pub alternate: bool,
    pub reverse: bool,
    pub fill_forwards: bool,
    pub fill_backwards: bool,
    pub paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListStyleType {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    None,
}

/// Generated content of `::before` / `::after` (`content` property).
#[derive(Debug, Clone, PartialEq)]
pub enum ContentItem {
    String(String),
    Attr(String),
    Counter(String, ListStyleType),
    OpenQuote,
    CloseQuote,
    Url(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// Every computed property the converter understands. Inherited properties
/// are marked; the rest reset to their initial value on each element.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    // Box generation and positioning
    pub display: Display,
    pub position: Position,
    pub top: LengthAuto,
    pub right: LengthAuto,
    pub bottom: LengthAuto,
    pub left: LengthAuto,
    /// `None` = `auto`.
    pub z_index: Option<i32>,
    pub float: Float,
    pub box_sizing: BoxSizing,

    // Sizing
    pub width: Size,
    pub height: Size,
    pub min_width: Size,
    pub min_height: Size,
    pub max_width: Size,
    pub max_height: Size,
    /// `aspect-ratio` as width / height; `None` = `auto`.
    pub aspect_ratio: Option<f32>,

    // Box model
    pub margin: Sides<LengthAuto>,
    pub padding: Sides<Length>,
    pub border: Sides<BorderSide>,
    pub border_radius: Corners,
    /// A corner's radius when it is a non-linear expression that cannot be
    /// resolved before layout; `Some` overrides the same corner of
    /// `border_radius`.
    pub border_radius_calc: Corners<Option<CalcNode>>,
    pub outline: BorderSide,
    pub outline_offset: f32,

    // Flexbox
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: Align,
    pub align_items: Align,
    pub align_self: Align,
    pub align_content: Align,
    pub justify_items: Align,
    pub justify_self: Align,
    pub row_gap: Option<Length>,
    pub column_gap: Option<Length>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    /// `auto` is `Size::Auto`, `content` is `Size::MaxContent`.
    pub flex_basis: Size,
    pub order: i32,

    // Grid
    pub grid_template_columns: Vec<Track>,
    pub grid_template_rows: Vec<Track>,
    /// Rows of area names; `.` cells are `None`.
    pub grid_template_areas: Vec<Vec<Option<String>>>,
    pub grid_auto_flow_column: bool,
    /// `grid-auto-flow: dense`: auto-placed items backfill earlier holes.
    pub grid_auto_flow_dense: bool,
    pub grid_auto_columns: Vec<Track>,
    pub grid_auto_rows: Vec<Track>,
    pub grid_column_start: GridLine,
    pub grid_column_end: GridLine,
    pub grid_row_start: GridLine,
    pub grid_row_end: GridLine,

    // Backgrounds and effects
    pub background_color: Rgba,
    /// Bottom-most layer last, as in CSS.
    pub background_layers: Vec<BackgroundLayer>,
    pub box_shadow: Vec<Shadow>,
    pub opacity: f32,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    /// The individual `translate`/`rotate`/`scale` properties. They are
    /// already included at the front of `transform`; kept separately so
    /// each can be replaced without touching the `transform` property.
    pub translate: Option<TransformFn>,
    pub rotate: Option<TransformFn>,
    pub scale: Option<TransformFn>,
    /// The effective list: `translate`, `rotate`, `scale` (when set), then
    /// the `transform` property's functions (CSS Transforms 2 §4).
    pub transform: Vec<TransformFn>,
    /// Percentages of the border box; default 50% 50%.
    pub transform_origin: (LengthPercentage, LengthPercentage),
    pub object_fit: ObjectFit,
    /// Raw `filter` value; QML has MultiEffect for blur/brightness etc.
    pub filter: Option<String>,

    // Inherited: text and fonts
    pub color: Rgba,
    pub font_family: Vec<String>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub small_caps: bool,
    pub line_height: LineHeight,
    /// `None` = `normal`.
    pub letter_spacing: Option<f32>,
    pub word_spacing: Option<f32>,
    pub text_align: TextAlign,
    pub text_indent: Length,
    pub text_transform: TextTransform,
    pub white_space: WhiteSpace,
    pub word_break: WordBreak,
    pub overflow_wrap_anywhere: bool,
    pub text_shadow: Vec<Shadow>,
    pub direction: Direction,
    pub visibility: Visibility,
    pub cursor: Cursor,
    pub pointer_events: PointerEvents,
    pub list_style_type: ListStyleType,
    pub list_style_inside: bool,
    pub list_style_image: Option<String>,
    pub user_select_none: bool,

    // Not inherited, but propagated to inline descendants by the converter
    pub text_decoration_line: TextDecorationLine,
    pub text_decoration_color: Option<Rgba>,
    pub text_decoration_style: BorderStyle,
    pub text_overflow: TextOverflow,
    pub vertical_align: VerticalAlign,

    // Animation
    pub transitions: Vec<Transition>,
    pub animations: Vec<Animation>,

    // Generated content (only set on ::before / ::after styles)
    pub content: Option<Vec<ContentItem>>,

    /// Custom properties (`--name` → token text after `var()` substitution
    /// in their own values). Inherited.
    pub custom_properties: BTreeMap<String, String>,
}

impl Default for ComputedStyle {
    /// CSS initial values (and the UA defaults for inherited ones:
    /// 16px serif black text).
    fn default() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            top: LengthAuto::Auto,
            right: LengthAuto::Auto,
            bottom: LengthAuto::Auto,
            left: LengthAuto::Auto,
            z_index: None,
            float: Float::None,
            box_sizing: BoxSizing::ContentBox,
            width: Size::Auto,
            height: Size::Auto,
            min_width: Size::Auto,
            min_height: Size::Auto,
            max_width: Size::None,
            max_height: Size::None,
            aspect_ratio: None,
            margin: Sides::all(LengthAuto::Length(Length::px(0.0))),
            padding: Sides::all(Length::px(0.0)),
            border: Sides::all(BorderSide::default()),
            border_radius: Corners::default(),
            border_radius_calc: Corners::default(),
            outline: BorderSide::default(),
            outline_offset: 0.0,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: Align::Normal,
            align_items: Align::Normal,
            align_self: Align::Auto,
            align_content: Align::Normal,
            justify_items: Align::Normal,
            justify_self: Align::Auto,
            row_gap: None,
            column_gap: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Size::Auto,
            order: 0,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_template_areas: Vec::new(),
            grid_auto_flow_column: false,
            grid_auto_flow_dense: false,
            grid_auto_columns: Vec::new(),
            grid_auto_rows: Vec::new(),
            grid_column_start: GridLine::Auto,
            grid_column_end: GridLine::Auto,
            grid_row_start: GridLine::Auto,
            grid_row_end: GridLine::Auto,
            background_color: Rgba::TRANSPARENT,
            background_layers: Vec::new(),
            box_shadow: Vec::new(),
            opacity: 1.0,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            translate: None,
            rotate: None,
            scale: None,
            transform: Vec::new(),
            transform_origin: (
                LengthPercentage::percent(50.0),
                LengthPercentage::percent(50.0),
            ),
            object_fit: ObjectFit::Fill,
            filter: None,
            color: Rgba::BLACK,
            font_family: vec!["serif".to_owned()],
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            small_caps: false,
            line_height: LineHeight::Normal,
            letter_spacing: None,
            word_spacing: None,
            text_align: TextAlign::Start,
            text_indent: Length::px(0.0),
            text_transform: TextTransform::None,
            white_space: WhiteSpace::Normal,
            word_break: WordBreak::Normal,
            overflow_wrap_anywhere: false,
            text_shadow: Vec::new(),
            direction: Direction::Ltr,
            visibility: Visibility::Visible,
            cursor: Cursor::Auto,
            pointer_events: PointerEvents::Auto,
            list_style_type: ListStyleType::Disc,
            list_style_inside: false,
            list_style_image: None,
            user_select_none: false,
            text_decoration_line: TextDecorationLine::default(),
            text_decoration_color: None,
            text_decoration_style: BorderStyle::Solid,
            text_overflow: TextOverflow::Clip,
            vertical_align: VerticalAlign::Baseline,
            transitions: Vec::new(),
            animations: Vec::new(),
            content: None,
            custom_properties: BTreeMap::new(),
        }
    }
}

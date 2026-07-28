//! Helpers shared by the tools in this crate.

use crate::*;

/// (emoji, name, search keywords). ponytail: a curated set, not the 1,900-entry
/// `unicode-emoji-json` dump — embed the full table when the picker ships.
pub static EMOJIS: &[(&str, &str, &str)] = &[
    ("😀", "grinning face", "smile happy joy grin"),
    ("😃", "grinning face with big eyes", "smile happy joy"),
    ("😄", "grinning face with smiling eyes", "smile happy laugh"),
    ("😁", "beaming face with smiling eyes", "smile grin happy"),
    ("😆", "grinning squinting face", "laugh haha happy"),
    ("😅", "grinning face with sweat", "hot laugh relief"),
    ("🤣", "rolling on the floor laughing", "rofl lol laugh"),
    ("😂", "face with tears of joy", "lol cry laugh tears"),
    ("🙂", "slightly smiling face", "smile"),
    ("🙃", "upside-down face", "silly sarcasm"),
    ("😉", "winking face", "wink flirt"),
    ("😊", "smiling face with smiling eyes", "blush shy happy"),
    ("😍", "smiling face with heart-eyes", "love crush heart"),
    ("😘", "face blowing a kiss", "kiss love"),
    ("😗", "kissing face", "kiss"),
    ("🤪", "zany face", "crazy silly goofy"),
    ("😜", "winking face with tongue", "tongue silly"),
    ("🤗", "smiling face with open hands", "hug"),
    ("🤔", "thinking face", "think hmm consider"),
    ("🤨", "face with raised eyebrow", "suspicious skeptic"),
    ("😐", "neutral face", "meh indifferent"),
    ("😑", "expressionless face", "blank meh"),
    ("😶", "face without mouth", "silent quiet"),
    ("🙄", "face with rolling eyes", "eyeroll annoyed"),
    ("😏", "smirking face", "smug flirt"),
    ("😴", "sleeping face", "sleep zzz tired"),
    ("😪", "sleepy face", "tired sleep"),
    ("😷", "face with medical mask", "sick mask covid"),
    ("🤒", "face with thermometer", "sick ill fever"),
    ("🤕", "face with head-bandage", "hurt injured"),
    ("🤢", "nauseated face", "sick vomit gross"),
    ("🥵", "hot face", "heat sweat"),
    ("🥶", "cold face", "freezing ice"),
    ("😵", "face with crossed-out eyes", "dizzy dead ko"),
    ("🤯", "exploding head", "mind blown shocked"),
    ("😎", "smiling face with sunglasses", "cool sunglasses"),
    ("🥳", "partying face", "party celebrate birthday"),
    ("😕", "confused face", "confused unsure"),
    ("😟", "worried face", "worried concern"),
    ("🙁", "slightly frowning face", "sad frown"),
    ("😢", "crying face", "sad cry tear"),
    ("😭", "loudly crying face", "sob cry sad"),
    ("😤", "face with steam from nose", "triumph angry proud"),
    ("😠", "angry face", "mad angry"),
    ("😡", "enraged face", "rage furious mad"),
    ("🤬", "face with symbols on mouth", "swearing cursing"),
    ("😱", "face screaming in fear", "scared shock fear"),
    ("😨", "fearful face", "scared afraid"),
    ("😰", "anxious face with sweat", "nervous anxious"),
    ("🥺", "pleading face", "please beg puppy eyes"),
    ("😳", "flushed face", "embarrassed blush"),
    ("🤥", "lying face", "lie pinocchio"),
    ("🤐", "zipper-mouth face", "silence secret"),
    ("😈", "smiling face with horns", "devil evil"),
    ("💀", "skull", "dead death skull"),
    ("💩", "pile of poo", "poop crap"),
    ("🤡", "clown face", "clown circus"),
    ("👻", "ghost", "halloween spooky"),
    ("👽", "alien", "ufo space extraterrestrial"),
    ("🤖", "robot", "bot ai machine"),
    ("😺", "grinning cat", "cat happy"),
    ("❤️", "red heart", "love heart red"),
    ("🧡", "orange heart", "love heart orange"),
    ("💛", "yellow heart", "love heart yellow"),
    ("💚", "green heart", "love heart green"),
    ("💙", "blue heart", "love heart blue"),
    ("💜", "purple heart", "love heart purple"),
    ("🖤", "black heart", "love heart black"),
    ("💔", "broken heart", "heartbreak sad"),
    ("💯", "hundred points", "100 perfect score"),
    ("💥", "collision", "boom explosion bang"),
    ("🔥", "fire", "hot lit flame burn"),
    ("✨", "sparkles", "shiny stars magic"),
    ("⭐", "star", "star favourite"),
    ("🎉", "party popper", "party celebrate tada"),
    ("🎊", "confetti ball", "party celebrate"),
    ("🎂", "birthday cake", "birthday cake party"),
    ("🎁", "wrapped gift", "present gift"),
    ("👍", "thumbs up", "yes approve like ok"),
    ("👎", "thumbs down", "no disapprove dislike"),
    ("👌", "OK hand", "ok perfect"),
    ("🤝", "handshake", "deal agreement"),
    ("👏", "clapping hands", "applause clap bravo"),
    ("🙏", "folded hands", "please thanks pray"),
    ("💪", "flexed biceps", "strong muscle gym"),
    ("🤞", "crossed fingers", "luck hope"),
    ("🖖", "vulcan salute", "spock star trek"),
    ("👋", "waving hand", "hello bye wave"),
    ("✋", "raised hand", "stop high five"),
    ("👀", "eyes", "look watch see"),
    ("🧠", "brain", "smart mind think"),
    ("🚀", "rocket", "launch ship space deploy"),
    ("🐛", "bug", "insect bug defect"),
    ("💻", "laptop", "computer pc dev"),
    ("🖥️", "desktop computer", "computer monitor"),
    ("⌨️", "keyboard", "typing keys"),
    ("🖱️", "computer mouse", "mouse click"),
    ("💾", "floppy disk", "save disk"),
    ("📦", "package", "box parcel shipping"),
    ("🔒", "locked", "lock secure private"),
    ("🔑", "key", "key password unlock"),
    ("🔍", "magnifying glass tilted left", "search find zoom"),
    ("⚙️", "gear", "settings config cog"),
    ("🔧", "wrench", "tool fix repair"),
    ("🔨", "hammer", "tool build"),
    ("📝", "memo", "note write memo"),
    ("📌", "pushpin", "pin location"),
    ("📎", "paperclip", "attach clip"),
    ("📊", "bar chart", "chart stats graph"),
    ("📈", "chart increasing", "growth up trend"),
    ("📉", "chart decreasing", "loss down trend"),
    ("⏰", "alarm clock", "time alarm clock"),
    ("⏳", "hourglass not done", "time wait loading"),
    ("✅", "check mark button", "yes done ok check"),
    ("❌", "cross mark", "no wrong error x"),
    ("⚠️", "warning", "caution warning alert"),
    ("♻️", "recycling symbol", "recycle green"),
    ("☕", "hot beverage", "coffee tea drink"),
    ("🍕", "pizza", "food pizza slice"),
    ("🍺", "beer mug", "beer drink pub"),
    ("🌍", "globe showing Europe-Africa", "earth world globe"),
    ("🌙", "crescent moon", "night moon sleep"),
    ("☀️", "sun", "sunny weather day"),
    ("🌈", "rainbow", "pride rainbow colors"),
    ("⚡", "high voltage", "lightning power fast"),
    ("❄️", "snowflake", "snow cold winter"),
];

/// Search `EMOJIS` by name, keyword or literal emoji. Empty query = everything.
pub fn emoji_search(query: &str) -> Vec<&'static (&'static str, &'static str, &'static str)> {
    let q = query.trim().to_lowercase();
    EMOJIS
        .iter()
        .filter(|(e, name, kw)| {
            q.is_empty() || name.contains(&q) || kw.contains(&q) || *e == q.as_str()
        })
        .collect()
}

/// Read `key` for tool `id`, falling back to the declared field default.
/// `Field::text` with a non-empty default; the shared constructor has no slot
/// for one, and the colour inputs upstream are pickers that always hold a value.
pub(crate) const fn text_default(
    key: &'static str,
    label: &'static str,
    default: &'static str,
) -> Field {
    Field {
        key,
        label,
        kind: FieldKind::Text,
        default,
        options: &[],
    }
}

pub(crate) fn a<'x>(args: &'x Args, id: &str, key: &str) -> &'x str {
    match TOOLS.iter().find(|t| t.id == id) {
        Some(t) => crate::arg(args, t, key),
        None => args.get(key).map(String::as_str).unwrap_or(""),
    }
}

pub(crate) fn parse_num(label: &str, s: &str) -> anyhow::Result<f64> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("{label} is required");
    }
    s.parse::<f64>()
        .map_err(|_| anyhow::anyhow!("{label}: `{s}` is not a number"))
}

pub(crate) fn uint(label: &str, s: &str, max: usize) -> anyhow::Result<usize> {
    let v = parse_num(label, s)?;
    if !v.is_finite() || v < 0.0 {
        anyhow::bail!("{label} must be a positive number");
    }
    let v = v as usize;
    if v > max {
        anyhow::bail!("{label} must be at most {max}");
    }
    Ok(v)
}

pub(crate) fn truthy(s: &str) -> bool {
    matches!(s.trim(), "true" | "1" | "yes" | "on")
}

pub(crate) fn nonempty<'x>(label: &str, s: &'x str) -> anyhow::Result<&'x str> {
    if s.trim().is_empty() {
        anyhow::bail!("{label} is empty");
    }
    Ok(s)
}

/// Trim float noise without losing the value: JS-style shortest-ish output.
pub(crate) fn fmt_f(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let r = (v * 1e6).round() / 1e6;
    format!("{r}")
}

/// One-line sibling of [`crate::section`], for the converters that it-tools
/// renders as a flat list of readonly `label: value` rows (date-time, colour,
/// temperature). Same contract: a row that cannot be produced says so in place
/// instead of blanking its neighbours. `section` is used where a panel is
/// genuinely multi-line.
pub(crate) fn row(label: &str, body: anyhow::Result<String>) -> String {
    match body {
        Ok(v) => format!("{label}: {v}\n"),
        Err(e) => format!("{label}: ({e:#})\n"),
    }
}

/// The `qrcode` SVG renderer prefixes an XML prolog; the UI inlines the markup.
pub(crate) const XML_PROLOG: &str = r#"<?xml version="1.0" standalone="yes"?>"#;

pub(crate) fn qr_ec_level(s: &str) -> anyhow::Result<qrcode::EcLevel> {
    Ok(match s.trim() {
        "low" | "L" | "l" => qrcode::EcLevel::L,
        "medium" | "M" | "m" | "" => qrcode::EcLevel::M,
        "quartile" | "Q" | "q" => qrcode::EcLevel::Q,
        "high" | "H" | "h" => qrcode::EcLevel::H,
        other => anyhow::bail!("unknown error correction level `{other}`"),
    })
}

/// Normalise any CSS color to `#rrggbb[aa]`; blank falls back to `fallback`.
pub(crate) fn qr_color(label: &str, s: &str, fallback: &str) -> anyhow::Result<String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(fallback.to_string());
    }
    let c = csscolorparser::parse(s).map_err(|e| anyhow::anyhow!("invalid {label} `{s}`: {e}"))?;
    Ok(c.to_css_hex())
}

/// Encode `payload` as inline SVG source. Errors (never panics) on empty input
/// and on payloads too long for the chosen error-correction level.
pub(crate) fn qr_svg(payload: &str, fg: &str, bg: &str, ec: &str) -> anyhow::Result<String> {
    nonempty("text", payload)?;
    let ec = qr_ec_level(ec)?;
    let fg = qr_color("foreground color", fg, "#000000")?;
    let bg = qr_color("background color", bg, "#ffffff")?;
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), ec)
        .map_err(|e| anyhow::anyhow!("cannot encode QR code: {e}"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .dark_color(qrcode::render::svg::Color(&fg))
        .light_color(qrcode::render::svg::Color(&bg))
        .build();
    Ok(svg.strip_prefix(XML_PROLOG).unwrap_or(&svg).to_string())
}

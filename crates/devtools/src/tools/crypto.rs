//! Crypto & security tools. Owned by the crypto batch.
//!
//! Ported from it-tools `src/tools/*`. Behaviour (and quirks — see
//! `base32_to_hex`) is kept identical so the ported test vectors hold.
use crate::{Args, Category, Field, FieldKind, Kind, Tool};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{KeyInit, Mac, SimpleHmac};
use sha2::Digest;

// ---------------------------------------------------------------- arg helpers
// ponytail: local getters instead of crate::arg() so a `run` never needs a
// back-reference to its own static. Defaults are repeated in the Field, which
// is two chars of duplication per tool and no plumbing.
fn s<'a>(a: &'a Args, k: &str, d: &'a str) -> &'a str {
    a.get(k).map(String::as_str).unwrap_or(d)
}
fn n(a: &Args, k: &str, d: i64) -> i64 {
    a.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d)
}
fn b(a: &Args, k: &str, d: bool) -> bool {
    match a.get(k).map(|v| v.trim()) {
        Some("true" | "1" | "yes" | "on") => true,
        Some("false" | "0" | "no" | "off") => false,
        _ => d,
    }
}

// ------------------------------------------------------------- byte utilities
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// it-tools `hexToBytes`: pairs of hex chars, a lone trailing char is its own
/// byte (`hexToBytes('1') === [1]`).
fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    let chars: Vec<char> = hex.chars().collect();
    chars
        .chunks(2)
        .map(|c| {
            let s: String = c.iter().collect();
            u8::from_str_radix(&s, 16).with_context(|| format!("not hexadecimal: {s:?}"))
        })
        .collect()
}

/// it-tools `convertHexToBin`: every hex digit becomes 4 bits.
fn convert_hex_to_bin(hex: &str) -> Result<String> {
    hex.trim()
        .chars()
        .map(|c| {
            let v = c
                .to_digit(16)
                .with_context(|| format!("not hexadecimal: {c:?}"))?;
            Ok(format!("{v:04b}"))
        })
        .collect()
}

fn encode_digest(bytes: &[u8], encoding: &str) -> Result<String> {
    let hex = hex_encode(bytes);
    Ok(match encoding {
        "Hex" => hex,
        "Bin" => convert_hex_to_bin(&hex)?,
        "Base64" => STANDARD.encode(bytes),
        "Base64url" => URL_SAFE_NO_PAD.encode(bytes),
        other => bail!("unknown encoding: {other}"),
    })
}

/// it-tools lists Bin first in the select, Hex is the default.
const ENCODINGS: &[&str] = &["Bin", "Hex", "Base64", "Base64url"];
/// Order of the rows in it-tools' hash-text card (`Object.keys(algos)`).
const HASH_ALGOS: &[&str] = &[
    "MD5",
    "SHA1",
    "SHA256",
    "SHA224",
    "SHA512",
    "SHA384",
    "SHA3",
    "RIPEMD160",
];
/// hmac-generator declares its own `algos` object, in a different order.
const HMAC_ALGOS: &[&str] = &[
    "MD5",
    "RIPEMD160",
    "SHA1",
    "SHA3",
    "SHA224",
    "SHA256",
    "SHA384",
    "SHA512",
];

/// `SHA3` is crypto-js' `SHA3` default: Keccak with a 512 bit digest (the
/// pre-standard padding), *not* FIPS SHA3-512.
fn hash_bytes(algo: &str, data: &[u8]) -> Result<Vec<u8>> {
    fn d<D: Digest>(data: &[u8]) -> Vec<u8> {
        D::digest(data).to_vec()
    }
    Ok(match algo {
        "MD5" => d::<md5::Md5>(data),
        "SHA1" => d::<sha1::Sha1>(data),
        "SHA224" => d::<sha2::Sha224>(data),
        "SHA256" => d::<sha2::Sha256>(data),
        "SHA384" => d::<sha2::Sha384>(data),
        "SHA512" => d::<sha2::Sha512>(data),
        "SHA3" => d::<sha3::Keccak512>(data),
        "RIPEMD160" => d::<ripemd::Ripemd160>(data),
        other => bail!("unknown hash algorithm: {other}"),
    })
}

fn hmac_bytes(algo: &str, key: &[u8], msg: &[u8]) -> Result<Vec<u8>> {
    // SimpleHmac (rather than Hmac) because Keccak is a sponge and does not
    // implement EagerHash, which Hmac requires.
    fn m<D>(key: &[u8], msg: &[u8]) -> Result<Vec<u8>>
    where
        D: Digest + hmac::digest::block_api::BlockSizeUser,
    {
        let mut mac = <SimpleHmac<D>>::new_from_slice(key).map_err(|e| anyhow::anyhow!("{e}"))?;
        Mac::update(&mut mac, msg);
        Ok(mac.finalize().into_bytes().to_vec())
    }
    Ok(match algo {
        "MD5" => m::<md5::Md5>(key, msg)?,
        "SHA1" => m::<sha1::Sha1>(key, msg)?,
        "SHA224" => m::<sha2::Sha224>(key, msg)?,
        "SHA256" => m::<sha2::Sha256>(key, msg)?,
        "SHA384" => m::<sha2::Sha384>(key, msg)?,
        "SHA512" => m::<sha2::Sha512>(key, msg)?,
        "SHA3" => m::<sha3::Keccak512>(key, msg)?,
        "RIPEMD160" => m::<ripemd::Ripemd160>(key, msg)?,
        other => bail!("unknown hash algorithm: {other}"),
    })
}

// ------------------------------------------------------------------ hash-text
fn run_hash_text(a: &Args) -> Result<String> {
    let text = s(a, "text", "");
    let encoding = s(a, "encoding", "Hex");
    let mut out = String::new();
    for algo in HASH_ALGOS {
        let digest = encode_digest(&hash_bytes(algo, text.as_bytes())?, encoding)?;
        out.push_str(&format!("{algo}: {digest}\n"));
    }
    Ok(out)
}

// ------------------------------------------------------------- hmac-generator
fn run_hmac(a: &Args) -> Result<String> {
    let mac = hmac_bytes(
        s(a, "algo", "SHA256"),
        s(a, "secret", "").as_bytes(),
        s(a, "text", "").as_bytes(),
    )?;
    encode_digest(&mac, s(a, "encoding", "Hex"))
}

// --------------------------------------------------------------------- bcrypt
// it-tools renders two independent cards, "Hash" and "Compare string with
// hash", both always visible. So both are always emitted, each as its own
// section: a salt count the hasher rejects must not hide the comparison.
/// Hash `text`, applying the salt-round rule in one place so the registry entry
/// and the app's bcrypt panel cannot disagree about it.
///
/// bcryptjs' `genSaltSync` silently clamps rounds into 4..=31, which is why
/// it-tools can offer a 0..=100 number input at all.
///
/// ponytail: the upper end is clamped to 15, not 31 — cost 31 is ~18 hours of
/// CPU for one hash. Say so instead of freezing the UI.
pub fn bcrypt_hash_with_rounds(text: &str, rounds: i64) -> Result<String> {
    let rounds = rounds.clamp(4, 31);
    if rounds > 15 {
        bail!("salt rounds above 15 would take minutes to hours per hash; not run here");
    }
    bcrypt::hash(text, rounds as u32).map_err(Into::into)
}

/// Whether `text` is the string behind `hash`.
///
/// `Err` means `hash` is not a bcrypt hash at all, which is a different answer
/// from "does not match" — the text output collapses the two, a UI should not.
pub fn bcrypt_verify(text: &str, hash: &str) -> Result<bool> {
    bcrypt::verify(text, hash.trim()).map_err(Into::into)
}

fn bcrypt_hash(a: &Args) -> Result<String> {
    bcrypt_hash_with_rounds(s(a, "text", ""), n(a, "rounds", 10))
}

fn bcrypt_compare(a: &Args) -> Result<String> {
    let matches = bcrypt_verify(s(a, "compare_text", ""), s(a, "compare_hash", ""));
    Ok(match matches.unwrap_or(false) {
        true => "Yes".into(),
        false => "No".into(),
    })
}

fn run_bcrypt(a: &Args) -> Result<String> {
    Ok(format!(
        "{}\n{}\n",
        crate::section("Hash", bcrypt_hash(a)),
        crate::section("Compare string with hash", bcrypt_compare(a))
    ))
}

// ------------------------------------------------------------------ encryption
// it-tools uses crypto-js, whose `Algo.encrypt(text, password)` emits the
// OpenSSL envelope: base64("Salted__" + 8 salt bytes + ciphertext), with key
// and IV from EVP_BytesToKey (MD5, iteration count 1).
//
// ponytail: Rabbit is in it-tools' list but has no Rust crate, so it is simply
// absent from ENCRYPT_ALGOS. Mislabelling another stream cipher as Rabbit would
// silently produce undecryptable output. Add it if someone ports the cipher.
const ENCRYPT_ALGOS: &[&str] = &["AES", "TripleDES", "RC4"];
const OPENSSL_MAGIC: &[u8] = b"Salted__";

/// crypto-js `EvpKDF` defaults / OpenSSL `EVP_BytesToKey`: MD5, one iteration.
/// Returns `key_len + iv_len` bytes, key first.
fn evp_bytes_to_key(pass: &[u8], salt: &[u8], key_len: usize, iv_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(key_len + iv_len);
    let mut block = Vec::new();
    while out.len() < key_len + iv_len {
        let mut h = md5::Md5::new();
        h.update(&block);
        h.update(pass);
        h.update(salt);
        block = h.finalize().to_vec();
        out.extend_from_slice(&block);
    }
    out.truncate(key_len + iv_len);
    out
}

/// `(key_len, iv_len)` from the crypto-js cipher configs: AES keySize 256/32
/// with a 128 bit block, TripleDES 192/32 with a 64 bit block, RC4 256/32 and
/// no IV.
fn algo_sizes(algo: &str) -> Result<(usize, usize)> {
    Ok(match algo {
        "AES" => (32, 16),
        "TripleDES" => (24, 8),
        "RC4" => (32, 0),
        other => bail!("unknown algorithm: {other}"),
    })
}

/// CBC with PKCS#7 for one concrete block cipher. `cbc` is built without its
/// `alloc` feature here, hence the explicit buffer (block size is <= 16).
macro_rules! cbc_apply {
    ($cipher:ty, $key:expr, $iv:expr, $data:expr, $encrypt:expr) => {{
        use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
        let mut buf = vec![0u8; $data.len() + 16];
        let bad_key = || anyhow::anyhow!("derived key or IV has the wrong length");
        let n = if $encrypt {
            <cbc::Encryptor<$cipher>>::new_from_slices($key, $iv)
                .map_err(|_| bad_key())?
                .encrypt_padded_b2b::<Pkcs7>($data, &mut buf)
                .map_err(|_| anyhow::anyhow!("output buffer too small"))?
                .len()
        } else {
            <cbc::Decryptor<$cipher>>::new_from_slices($key, $iv)
                .map_err(|_| bad_key())?
                .decrypt_padded_b2b::<Pkcs7>($data, &mut buf)
                .map_err(|_| anyhow::anyhow!("wrong password, or corrupted ciphertext"))?
                .len()
        };
        buf.truncate(n);
        buf
    }};
}

fn cipher_apply(algo: &str, key: &[u8], iv: &[u8], data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    Ok(match algo {
        "AES" => cbc_apply!(aes::Aes256, key, iv, data, encrypt),
        "TripleDES" => cbc_apply!(des::TdesEde3, key, iv, data, encrypt),
        // RC4 is a stream cipher: keystream XOR, same both directions, no IV.
        "RC4" => {
            let mut c = <rc4::Rc4 as rc4::KeyInit>::new_from_slice(key)
                .map_err(|_| anyhow::anyhow!("derived key has the wrong length"))?;
            let mut buf = data.to_vec();
            rc4::StreamCipher::apply_keystream(&mut c, &mut buf);
            buf
        }
        other => bail!("unknown algorithm: {other}"),
    })
}

fn crypto_js_encrypt(algo: &str, secret: &str, plain: &str) -> Result<String> {
    let (key_len, iv_len) = algo_sizes(algo)?;
    let salt: [u8; 8] = std::array::from_fn(|_| rand::random());
    let kdf = evp_bytes_to_key(secret.as_bytes(), &salt, key_len, iv_len);
    let mut out = OPENSSL_MAGIC.to_vec();
    out.extend_from_slice(&salt);
    out.extend_from_slice(&cipher_apply(
        algo,
        &kdf[..key_len],
        &kdf[key_len..],
        plain.as_bytes(),
        true,
    )?);
    Ok(STANDARD.encode(out))
}

fn crypto_js_decrypt(algo: &str, secret: &str, payload: &str) -> Result<String> {
    let (key_len, iv_len) = algo_sizes(algo)?;
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let raw = STANDARD
        .decode(&compact)
        .context("ciphertext is not valid base64")?;
    if raw.len() < 16 || &raw[..8] != OPENSSL_MAGIC {
        bail!("not a crypto-js/OpenSSL payload: expected a \"Salted__\" header");
    }
    let kdf = evp_bytes_to_key(secret.as_bytes(), &raw[8..16], key_len, iv_len);
    let plain = cipher_apply(algo, &kdf[..key_len], &kdf[key_len..], &raw[16..], false)?;
    String::from_utf8(plain)
        .context("decrypted bytes are not valid UTF-8: wrong password or algorithm")
}

// The `.vue` has no direction toggle: "Encrypt" and "Decrypt" are two separate
// cards with their own text, secret and algorithm, both live at once. So there
// is no `mode` field, and a failing decrypt (the common case, since the box
// usually holds a half-pasted payload) only reports inside its own section.
fn run_encryption(a: &Args) -> Result<String> {
    Ok(format!(
        "{}\n{}\n",
        crate::section(
            "Encrypt",
            crypto_js_encrypt(s(a, "algo", "AES"), s(a, "secret", ""), s(a, "text", "")),
        ),
        crate::section(
            "Decrypt",
            crypto_js_decrypt(
                s(a, "decrypt_algo", "AES"),
                s(a, "decrypt_secret", ""),
                s(a, "decrypt_text", ""),
            ),
        )
    ))
}

// ------------------------------------------------------------ token-generator
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = ".,;:!?./-\"'#{([-|\\@)]=}*+";

/// `length` chars drawn uniformly from `alphabet`.
fn random_token(alphabet: &str, length: usize) -> String {
    let chars: Vec<char> = alphabet.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    (0..length)
        .map(|_| chars[rand::random_range(0..chars.len())])
        .collect()
}

fn run_token(a: &Args) -> Result<String> {
    // it-tools drives the length with a slider, min 1, max 512.
    let length = n(a, "length", 64);
    if !(1..=512).contains(&length) {
        bail!("length must be between 1 and 512");
    }
    let mut alphabet = String::new();
    if b(a, "uppercase", true) {
        alphabet.push_str(UPPER);
    }
    if b(a, "lowercase", true) {
        alphabet.push_str(LOWER);
    }
    if b(a, "numbers", true) {
        alphabet.push_str(DIGITS);
    }
    if b(a, "symbols", false) {
        alphabet.push_str(SYMBOLS);
    }
    Ok(random_token(&alphabet, length as usize))
}

// -------------------------------------------------- password-strength-analyser
fn charset_length(password: &str) -> u32 {
    let mut len = 0;
    if password.chars().any(|c| c.is_ascii_lowercase()) {
        len += 26;
    }
    if password.chars().any(|c| c.is_ascii_uppercase()) {
        len += 26;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        len += 10;
    }
    // JS `/\W|_/`: anything that is not [A-Za-z0-9], or an underscore.
    if password.chars().any(|c| c == '_' || !c.is_alphanumeric()) {
        len += 32;
    }
    len
}

fn prettify_quantity(q: f64) -> String {
    if q >= 1e21 {
        // JS switches to exponential notation here.
        let exp = q.log10().floor();
        return format!("{:.2}e+{}", q / 10f64.powf(exp), exp as i64);
    }
    // Thousands separators, like JS toLocaleString on an integer.
    let digits: Vec<char> = format!("{}", q as u64).chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

const TIME_UNITS: &[(&str, &str, f64)] = &[
    ("millennium", "millennia", 31_536_000_000.0),
    ("century", "centuries", 3_153_600_000.0),
    ("decade", "decades", 315_360_000.0),
    ("year", "years", 31_536_000.0),
    ("month", "months", 2_592_000.0),
    ("week", "weeks", 604_800.0),
    ("day", "days", 86_400.0),
    ("hour", "hours", 3_600.0),
    ("minute", "minutes", 60.0),
    ("second", "seconds", 1.0),
];

fn human_duration(mut seconds: f64) -> String {
    if seconds <= 0.001 {
        return "Instantly".into();
    }
    if seconds <= 1.0 {
        return "Less than a second".into();
    }
    let mut parts = Vec::new();
    for (unit, plural, in_unit) in TIME_UNITS {
        let quantity = (seconds / in_unit).floor();
        seconds %= in_unit;
        if quantity <= 0.0 {
            continue;
        }
        let label = if quantity > 1.0 { plural } else { unit };
        parts.push(format!("{} {label}", prettify_quantity(quantity)));
        if parts.len() == 2 {
            break;
        }
    }
    parts.join(", ")
}

/// it-tools hard-codes `guessesPerSecond` (the service parameter is never
/// passed from the `.vue`), so it is not a field here either.
const GUESSES_PER_SECOND: f64 = 1e9;

fn run_password_strength(a: &Args) -> Result<String> {
    let password = s(a, "password", "");
    let charset = charset_length(password);
    let length = password.chars().count();
    let entropy = if password.is_empty() {
        0.0
    } else {
        (charset as f64).log2() * length as f64
    };
    let seconds_to_crack = 2f64.powf(entropy) / GUESSES_PER_SECOND;
    // Same five readonly rows the `.vue` derives, in its order: the crack
    // duration card first, then length / entropy / charset size / score.
    Ok(format!(
        "duration to crack this password with brute force: {}\n\
         password length: {length}\n\
         entropy: {entropy:.2}\n\
         character set size: {charset}\n\
         score: {} / 100\n",
        human_duration(seconds_to_crack),
        ((entropy / 128.0).min(1.0) * 100.0).round(),
    ))
}

// -------------------------------------------- otp-code-generator-and-validator
const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// it-tools `base32toHex`. Quirk faithfully kept: the bit string is chunked in
/// 8s and a trailing partial chunk is parsed as-is, so `ABCDEF` -> `00443205`.
fn base32_to_hex(base32: &str) -> Result<String> {
    let mut bits = String::new();
    for c in base32.to_uppercase().trim_end_matches('=').chars() {
        let idx = BASE32_ALPHABET
            .iter()
            .position(|&b| b == c as u8)
            .with_context(|| format!("not a base32 character: {c:?}"))?;
        bits.push_str(&format!("{idx:05b}"));
    }
    let chars: Vec<char> = bits.chars().collect();
    Ok(chars
        .chunks(8)
        .map(|chunk| {
            let s: String = chunk.iter().collect();
            format!("{:02x}", u8::from_str_radix(&s, 2).unwrap_or(0))
        })
        .collect())
}

fn generate_hotp(key: &str, counter: u64) -> Result<String> {
    let secret = hex_to_bytes(&base32_to_hex(key)?)?;
    let digest = hmac_bytes("SHA1", &secret, &counter.to_be_bytes())?;
    let offset = (digest[19] & 0x0f) as usize;
    let v = ((digest[offset] as u32 & 0x7f) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | (digest[offset + 3] as u32);
    Ok(format!("{:06}", v % 1_000_000))
}

fn counter_from_time(now_ms: i64, time_step: i64) -> u64 {
    (now_ms / 1000 / time_step).max(0) as u64
}

fn build_key_uri(secret: &str, app: &str, account: &str, digits: i64, period: i64) -> String {
    let e = |v: &str| urlencoding::encode(v).into_owned();
    format!(
        "otpauth://totp/{}:{}?issuer={}&secret={}&algorithm=SHA1&digits={digits}&period={period}",
        e(app),
        e(account),
        e(app),
        e(secret)
    )
}

/// The `.vue` has exactly one input — the secret — and no direction or mode
/// toggle: the time step is fixed at 30s, the digit count at 6, and there is no
/// HOTP counter and no "validate this code" box. Validation is visual: the
/// previous / current / next codes are shown side by side.
///
/// Not reproducible in a one-shot pure function, and deliberately absent:
///  * the progress bar and "Next in NNs" countdown, which re-render on a timer;
///  * the QR code image of the key URI (needs a raster/canvas target — the
///    key URI itself is emitted so it can be pasted into an authenticator).
///
/// `timestamp_ms` is not an it-tools field; it exists so the clock can be
/// pinned in tests. Empty (the default) means now, as in the UI.
const OTP_TIME_STEP: i64 = 30;

fn run_otp(a: &Args) -> Result<String> {
    let secret = s(a, "secret", "").trim().to_string();
    if secret.is_empty() {
        bail!("please set a secret (a base32 string)");
    }
    let now_ms = match s(a, "timestamp_ms", "").trim() {
        "" => chrono::Utc::now().timestamp_millis(),
        v => v
            .parse()
            .context("timestamp must be a number of milliseconds")?,
    };
    let counter = counter_from_time(now_ms, OTP_TIME_STEP);
    let step_ms = OTP_TIME_STEP * 1000;
    let code = |ms: i64| generate_hotp(&secret, counter_from_time(ms, OTP_TIME_STEP));
    let codes = || -> Result<String> {
        Ok(format!(
            "  previous: {}\n  current:  {}\n  next:     {}",
            code(now_ms - step_ms)?,
            code(now_ms)?,
            code(now_ms + step_ms)?
        ))
    };
    Ok([
        crate::section("Codes", codes()),
        crate::section("Secret in hexadecimal", base32_to_hex(&secret)),
        crate::section("Epoch", Ok(format!("  {}", now_ms / 1000))),
        crate::section("Iteration count", Ok(format!("  {counter}"))),
        crate::section(
            "Iteration count, padded hex",
            Ok(format!("  {counter:016x}")),
        ),
        crate::section(
            "Key URI",
            Ok(format!(
                "  {}",
                build_key_uri(&secret, "IT-Tools", "demo-user", 6, OTP_TIME_STEP)
            )),
        ),
    ]
    .join("\n"))
}

// ------------------------------------------------------------ bip39-generator
// ponytail: English only — the other wordlists are cargo features on `bip39`
// and Cargo.toml is off-limits here.
fn run_bip39(a: &Args) -> Result<String> {
    let mnemonic_in = s(a, "mnemonic", "").trim();
    let mnemonic = if !mnemonic_in.is_empty() {
        bip39::Mnemonic::parse(mnemonic_in).context("invalid mnemonic")?
    } else {
        let entropy = match s(a, "entropy", "").trim() {
            "" => (0..16).map(|_| rand::random::<u8>()).collect(),
            hex => {
                if hex.len() % 2 != 0 {
                    bail!("entropy must be an even-length hexadecimal string");
                }
                hex_to_bytes(hex)?
            }
        };
        if !(16..=32).contains(&entropy.len()) || entropy.len() % 4 != 0 {
            bail!("entropy length should be >= 16, <= 32 and be a multiple of 4 bytes");
        }
        bip39::Mnemonic::from_entropy(&entropy).context("invalid entropy")?
    };
    Ok(format!(
        "entropy: {}\nmnemonic: {mnemonic}\n",
        hex_encode(&mnemonic.to_entropy())
    ))
}

// ----------------------------------------------------- rsa-key-pair-generator
const RSA_KEY_SIZES: &[&str] = &["512", "1024", "2048", "4096"];

/// `rand` 0.10 bridged onto the `rand_core` 0.6 that `rsa` 0.9 expects.
// ponytail: six trait methods instead of turning on rsa's `getrandom` feature,
// because Cargo.toml is off-limits. `rand::random` is a CSPRNG, so claiming
// `CryptoRng` is honest.
struct RsaRng;
impl rsa::rand_core::RngCore for RsaRng {
    fn next_u32(&mut self) -> u32 {
        rand::random()
    }
    fn next_u64(&mut self) -> u64 {
        rand::random()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        dest.fill_with(rand::random);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rsa::rand_core::Error> {
        dest.fill_with(rand::random);
        Ok(())
    }
}
impl rsa::rand_core::CryptoRng for RsaRng {}

fn run_rsa(a: &Args) -> Result<String> {
    // ponytail: generation is CPU-bound and blocking — 4096 bits takes seconds
    // to a minute — while the UI calls `run` on every keystroke. So nothing is
    // computed until `generate` is explicitly switched on. Upgrade path: move
    // the tool off Kind::Pure onto a background task.
    if !b(a, "generate", false) {
        return Ok("set generate to true to produce a key pair".into());
    }
    let bits = s(a, "bits", "2048").trim();
    if !RSA_KEY_SIZES.contains(&bits) {
        bail!("key size must be one of {}", RSA_KEY_SIZES.join(", "));
    }
    let bits: usize = bits.parse().expect("RSA_KEY_SIZES are numbers");
    let private =
        rsa::RsaPrivateKey::new(&mut RsaRng, bits).context("RSA key generation failed")?;
    let public = rsa::RsaPublicKey::from(&private);
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
    // it-tools shows the public key panel above the private one.
    Ok(format!(
        "{}\n{}",
        crate::section(
            "Public key",
            public
                .to_public_key_pem(LineEnding::LF)
                .context("cannot encode the public key")
        ),
        crate::section(
            "Private key",
            private
                .to_pkcs8_pem(LineEnding::LF)
                .context("cannot encode the private key")
                .map(|pem| pem.to_string())
        )
    ))
}

// -------------------------------------------------------------- uuid-generator
const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// RFC 4122 name-based UUID (v3 = MD5, v5 = SHA-1). ponytail: five lines beats
/// enabling two more `uuid` features someone else owns.
fn uuid_name_based(namespace: &str, name: &str, version: u8) -> Result<String> {
    let ns = uuid::Uuid::parse_str(namespace.trim()).context("invalid namespace UUID")?;
    let mut data = ns.as_bytes().to_vec();
    data.extend_from_slice(name.as_bytes());
    let hash = hash_bytes(if version == 3 { "MD5" } else { "SHA1" }, &data)?;
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | (version << 4);
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes).to_string())
}

/// it-tools' `v35Args` default. Its namespace buttons offer DNS
/// (`6ba7b810-…`), URL, OID and X500, with URL preselected.
const URL_NAMESPACE: &str = "6ba7b811-9dad-11d1-80b4-00c04fd430c8";

fn run_uuid(a: &Args) -> Result<String> {
    let count = n(a, "count", 1);
    if !(1..=50).contains(&count) {
        bail!("quantity must be between 1 and 50");
    }
    let version = s(a, "version", "v4");
    let namespace = s(a, "namespace", URL_NAMESPACE);
    let name = s(a, "name", "");
    let mut out = String::new();
    for _ in 0..count {
        let id = match version {
            "NIL" => NIL_UUID.to_string(),
            "v1" => {
                let node: [u8; 6] = std::array::from_fn(|_| rand::random());
                uuid::Uuid::now_v1(&node).to_string()
            }
            "v3" => uuid_name_based(namespace, name, 3)?,
            "v4" => uuid::Uuid::new_v4().to_string(),
            "v5" => uuid_name_based(namespace, name, 5)?,
            other => bail!("unknown UUID version: {other}"),
        };
        out.push_str(&id);
        out.push('\n');
    }
    Ok(out)
}

// -------------------------------------------------------------- ulid-generator
fn run_ulid(a: &Args) -> Result<String> {
    let count = n(a, "count", 1);
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let ids: Vec<String> = (0..count)
        .map(|_| ulid::Ulid::generate().to_string())
        .collect();
    Ok(match s(a, "format", "Raw") {
        "Raw" => ids.join("\n"),
        "JSON" => serde_json::to_string_pretty(&ids)?,
        other => bail!("unknown format: {other}"),
    })
}

// ----------------------------------------------------------------- jwt-parser
fn jwt_part(part: &str) -> Result<serde_json::Value> {
    let raw = URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .context("JWT segment is not valid base64url")?;
    serde_json::from_slice(&raw).context("JWT segment is not valid JSON")
}

fn friendly_date(v: &serde_json::Value) -> Option<String> {
    let secs = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))?;
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

/// it-tools' `ALGORITHM_DESCRIPTIONS`, from RFC 7518 section 3.1.
const ALGORITHM_DESCRIPTIONS: &[(&str, &str)] = &[
    ("HS256", "HMAC using SHA-256"),
    ("HS384", "HMAC using SHA-384"),
    ("HS512", "HMAC using SHA-512"),
    ("RS256", "RSASSA-PKCS1-v1_5 using SHA-256"),
    ("RS384", "RSASSA-PKCS1-v1_5 using SHA-384"),
    ("RS512", "RSASSA-PKCS1-v1_5 using SHA-512"),
    ("ES256", "ECDSA using P-256 and SHA-256"),
    ("ES384", "ECDSA using P-384 and SHA-384"),
    ("ES512", "ECDSA using P-521 and SHA-512"),
    ("PS256", "RSASSA-PSS using SHA-256 and MGF1 with SHA-256"),
    ("PS384", "RSASSA-PSS using SHA-384 and MGF1 with SHA-384"),
    ("PS512", "RSASSA-PSS using SHA-512 and MGF1 with SHA-512"),
    ("none", "No digital signature or MAC performed"),
];

/// it-tools' `CLAIM_DESCRIPTIONS`, the IANA JWT claim registry. The table in
/// the `.vue` prints this next to every claim it recognises.
const CLAIM_DESCRIPTIONS: &[(&str, &str)] = &[
    ("typ", "Type"),
    ("alg", "Algorithm"),
    ("iss", "Issuer"),
    ("sub", "Subject"),
    ("aud", "Audience"),
    ("exp", "Expiration Time"),
    ("nbf", "Not Before"),
    ("iat", "Issued At"),
    ("jti", "JWT ID"),
    ("name", "Full name"),
    ("given_name", "Given name(s) or first name(s)"),
    ("family_name", "Surname(s) or last name(s)"),
    ("middle_name", "Middle name(s)"),
    ("nickname", "Casual name"),
    (
        "preferred_username",
        "Shorthand name by which the End-User wishes to be referred to",
    ),
    ("profile", "Profile page URL"),
    ("picture", "Profile picture URL"),
    ("website", "Web page or blog URL"),
    ("email", "Preferred e-mail address"),
    (
        "email_verified",
        "True if the e-mail address has been verified; otherwise false",
    ),
    ("gender", "Gender"),
    ("birthdate", "Birthday"),
    ("zoneinfo", "Time zone"),
    ("locale", "Locale"),
    ("phone_number", "Preferred telephone number"),
    (
        "phone_number_verified",
        "True if the phone number has been verified; otherwise false",
    ),
    ("address", "Preferred postal address"),
    ("updated_at", "Time the information was last updated"),
    (
        "azp",
        "Authorized party - the party to which the ID Token was issued",
    ),
    (
        "nonce",
        "Value used to associate a Client session with an ID Token",
    ),
    ("auth_time", "Time when the authentication occurred"),
    ("at_hash", "Access Token hash value"),
    ("c_hash", "Code hash value"),
    ("acr", "Authentication Context Class Reference"),
    ("amr", "Authentication Methods References"),
    (
        "sub_jwk",
        "Public key used to check the signature of an ID Token",
    ),
    ("cnf", "Confirmation"),
    ("sip_from_tag", "SIP From tag header field parameter value"),
    ("sip_date", "SIP Date header field value"),
    ("sip_callid", "SIP Call-Id header field value"),
    (
        "sip_cseq_num",
        "SIP CSeq numeric header field parameter value",
    ),
    (
        "sip_via_branch",
        "SIP Via branch header field parameter value",
    ),
    ("orig", "Originating Identity String"),
    ("dest", "Destination Identity String"),
    ("mky", "Media Key Fingerprint String"),
    ("events", "Security Events"),
    ("toe", "Time of Event"),
    ("txn", "Transaction Identifier"),
    ("rph", "Resource Priority Header Authorization"),
    ("sid", "Session ID"),
    ("vot", "Vector of Trust value"),
    ("vtm", "Vector of Trust trustmark URL"),
    ("attest", "Attestation level as defined in SHAKEN framework"),
    (
        "origid",
        "Originating Identifier as defined in SHAKEN framework",
    ),
    ("act", "Actor"),
    ("scope", "Scope Values"),
    ("client_id", "Client Identifier"),
    (
        "may_act",
        "Authorized Actor - the party that is authorized to become the actor",
    ),
    ("jcard", "jCard data"),
    (
        "at_use_nbr",
        "Number of API requests for which the access token can be used",
    ),
    ("div", "Diverted Target of a Call"),
    ("opt", "Original PASSporT (in Full Form)"),
    (
        "vc",
        "Verifiable Credential as specified in the W3C Recommendation",
    ),
    (
        "vp",
        "Verifiable Presentation as specified in the W3C Recommendation",
    ),
    ("sph", "SIP Priority header field"),
    (
        "ace_profile",
        "ACE profile a token is supposed to be used with.",
    ),
    ("cnonce", "Client nonce"),
    ("exi", "Expires in"),
    ("roles", "Roles"),
    ("groups", "Groups"),
    ("entitlements", "Entitlements"),
    ("token_introspection", "Token introspection response"),
];

fn describe(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    // ponytail: linear scan over ~70 static entries, once per claim.
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// it-tools' `getFriendlyValue`: a readable date for the time claims, and the
/// RFC 7518 name for `alg`.
fn friendly_value(claim: &str, value: &serde_json::Value) -> Option<String> {
    match claim {
        "exp" | "nbf" | "iat" => friendly_date(value),
        "alg" => describe(ALGORITHM_DESCRIPTIONS, value.as_str()?).map(str::to_owned),
        _ => None,
    }
}

/// One `Header` / `Payload` table body. Every claim present is listed, with its
/// human-readable name where the registry has one.
fn jwt_claims(value: &serde_json::Value) -> Result<String> {
    let map = value
        .as_object()
        .context("this JWT segment is not a JSON object")?;
    let mut out = String::new();
    for (claim, value) in map {
        let name = describe(CLAIM_DESCRIPTIONS, claim)
            .map(|d| format!(" ({d})"))
            .unwrap_or_default();
        // Objects and arrays are printed as indented JSON, like the `.vue`.
        let formatted = match value {
            serde_json::Value::String(s) => s.clone(),
            v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)) => {
                serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
            }
            other => other.to_string(),
        };
        out.push_str(&format!("  {claim}{name}: {formatted}"));
        if let Some(friendly) = friendly_value(claim, value) {
            out.push_str(&format!(" ({friendly})"));
        }
        out.push('\n');
    }
    Ok(out)
}

fn run_jwt(a: &Args) -> Result<String> {
    let jwt = s(a, "jwt", "").trim();
    let parts: Vec<&str> = jwt.split('.').collect();
    // Nothing at all is decodable without two non-empty segments, so this one
    // case is a real Err — the `.vue` likewise hides the table and shows
    // "Invalid JWT". A segment that decodes reports itself even if the other
    // does not.
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("invalid JWT: expected header.payload.signature");
    }
    Ok(format!(
        "{}\n{}",
        crate::section("Header", jwt_part(parts[0]).and_then(|v| jwt_claims(&v))),
        crate::section("Payload", jwt_part(parts[1]).and_then(|v| jwt_claims(&v)))
    ))
}

// -------------------------------------------------------- basic-auth-generator
fn run_basic_auth(a: &Args) -> Result<String> {
    let credentials = format!("{}:{}", s(a, "username", ""), s(a, "password", ""));
    Ok(format!(
        "Authorization: Basic {}",
        STANDARD.encode(credentials)
    ))
}

// ----------------------------------------------------------- string-obfuscator
fn obfuscate(
    input: &str,
    replacement: char,
    keep_first: usize,
    keep_last: usize,
    keep_space: bool,
) -> String {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if keep_space && c == ' ' {
                return c;
            }
            if i < keep_first || i + keep_last >= len {
                c
            } else {
                replacement
            }
        })
        .collect()
}

fn run_obfuscator(a: &Args) -> Result<String> {
    // it-tools has no replacement-character input: the model hard-codes '*'.
    Ok(obfuscate(
        s(a, "text", ""),
        '*',
        n(a, "keep_first", 4).max(0) as usize,
        n(a, "keep_last", 4).max(0) as usize,
        b(a, "keep_space", true),
    ))
}

// ------------------------------------------------------- pdf-signature-checker
fn run_pdf_signature(_: &Args) -> Result<String> {
    // it-tools: a drag-and-drop .pdf upload, then one panel per embedded
    // signature listing its certificate chain. No text input at all, hence no
    // fields and Kind::Custom.
    bail!(
        "needs bespoke UI: takes a dropped PDF file and lists the certificates \
         of each embedded PKCS#7 signature"
    )
}

// ------------------------------------------------------------------- registry
pub static TOOLS: &[Tool] = &[
    Tool {
        id: "hash-text",
        name: "Hash text",
        description: "Hash a text with MD5, SHA1, SHA2, SHA3 (Keccak) and RIPEMD160.",
        category: Category::Crypto,
        keywords: &[
            "hash", "md5", "sha", "sha1", "sha256", "sha512", "ripemd", "digest",
        ],
        fields: &[
            Field::area("text", "Your text to hash"),
            Field::select("encoding", "Digest encoding", ENCODINGS, "Hex"),
        ],
        kind: Kind::Pure,
        run: run_hash_text,
    },
    Tool {
        id: "hmac-generator",
        name: "Hmac generator",
        description: "Compute a hash-based message authentication code (HMAC) of a text.",
        category: Category::Crypto,
        keywords: &["hmac", "hash", "mac", "secret", "signature"],
        fields: &[
            Field::area("text", "Plain text to compute the hash"),
            Field::text("secret", "Secret key"),
            Field::select("algo", "Hashing function", HMAC_ALGOS, "SHA256"),
            Field::select("encoding", "Output encoding", ENCODINGS, "Hex"),
        ],
        kind: Kind::Pure,
        run: run_hmac,
    },
    Tool {
        id: "bcrypt",
        name: "Bcrypt",
        description: "Hash a string with bcrypt, or check a string against a bcrypt hash.",
        category: Category::Crypto,
        keywords: &["bcrypt", "hash", "password", "salt", "compare"],
        // Two always-visible cards upstream: Hash (string + salt count) and
        // Compare string with hash (string + hash -> Yes/No).
        fields: &[
            Field::text("text", "Your string"),
            Field::number("rounds", "Salt count", "10"),
            Field::text("compare_text", "Your string (compare)"),
            Field::text("compare_hash", "Your hash (compare)"),
        ],
        kind: Kind::Pure,
        run: run_bcrypt,
    },
    Tool {
        id: "encryption",
        name: "Encrypt / decrypt text",
        description: "Encrypt and decrypt text with AES, TripleDES or RC4 (crypto-js compatible).",
        category: Category::Crypto,
        keywords: &["encrypt", "decrypt", "aes", "tripledes", "rc4", "cypher"],
        // Encrypt and Decrypt are separate cards upstream, each with its own
        // text, secret and algorithm — no direction toggle.
        fields: &[
            Field::area("text", "Your text (encrypt)"),
            Field::text("secret", "Your secret key (encrypt)"),
            Field::select(
                "algo",
                "Encryption algorithm (encrypt)",
                ENCRYPT_ALGOS,
                "AES",
            ),
            Field::area("decrypt_text", "Your encrypted text (decrypt)"),
            Field::text("decrypt_secret", "Your secret key (decrypt)"),
            Field::select(
                "decrypt_algo",
                "Encryption algorithm (decrypt)",
                ENCRYPT_ALGOS,
                "AES",
            ),
        ],
        kind: Kind::Pure,
        run: run_encryption,
    },
    Tool {
        id: "token-generator",
        name: "Token generator",
        description: "Generate a random token with the character sets you pick.",
        category: Category::Crypto,
        keywords: &["token", "random", "password", "generator", "secret"],
        fields: &[
            Field::number("length", "Length", "64"),
            Field::bool("uppercase", "Uppercase", "true"),
            Field::bool("lowercase", "Lowercase", "true"),
            Field::bool("numbers", "Numbers", "true"),
            Field::bool("symbols", "Symbols", "false"),
        ],
        kind: Kind::Pure,
        run: run_token,
    },
    Tool {
        id: "password-strength-analyser",
        name: "Password strength analyser",
        description: "Estimate the entropy of a password and how long it takes to crack it.",
        category: Category::Crypto,
        keywords: &["password", "strength", "entropy", "crack", "brute", "force"],
        fields: &[Field::text("password", "Password")],
        kind: Kind::Pure,
        run: run_password_strength,
    },
    Tool {
        id: "otp-code-generator-and-validator",
        name: "OTP code generator and validator",
        description: "Generate and validate time-based OTP (TOTP) codes from a base32 secret.",
        category: Category::Crypto,
        keywords: &["otp", "totp", "hotp", "2fa", "mfa", "authenticator", "code"],
        fields: &[
            Field::text("secret", "Secret"),
            Field::text("timestamp_ms", "Timestamp in ms (empty = now)"),
        ],
        kind: Kind::Pure,
        run: run_otp,
    },
    Tool {
        id: "bip39-generator",
        name: "Bip39 passphrase generator",
        description: "Convert between a BIP39 entropy (seed) and its English mnemonic.",
        category: Category::Crypto,
        keywords: &[
            "bip39",
            "mnemonic",
            "passphrase",
            "seed",
            "entropy",
            "wallet",
        ],
        fields: &[
            Field::text("entropy", "Entropy (hex, empty = random)"),
            Field::text("mnemonic", "Mnemonic (overrides entropy)"),
        ],
        kind: Kind::Pure,
        run: run_bip39,
    },
    Tool {
        id: "rsa-key-pair-generator",
        name: "RSA key pair generator",
        description: "Generate a new RSA private and public key pair.",
        category: Category::Crypto,
        keywords: &["rsa", "key", "pair", "private", "public", "pem"],
        fields: &[
            Field::select("bits", "Key size (bits)", RSA_KEY_SIZES, "2048"),
            Field::bool("generate", "Generate (slow, off by default)", "false"),
        ],
        kind: Kind::Pure,
        run: run_rsa,
    },
    Tool {
        id: "uuid-generator",
        name: "UUID generator",
        description: "Generate NIL, v1, v3, v4 or v5 UUIDs.",
        category: Category::Crypto,
        keywords: &["uuid", "guid", "unique", "identifier", "v4", "nil"],
        fields: &[
            Field::select(
                "version",
                "UUID version",
                &["NIL", "v1", "v3", "v4", "v5"],
                "v4",
            ),
            Field::number("count", "Quantity", "1"),
            // Prefilled with the URL namespace, as the upstream buttons are.
            Field {
                key: "namespace",
                label: "Namespace (v3/v5)",
                kind: FieldKind::Text,
                default: URL_NAMESPACE,
                options: &[],
            },
            Field::text("name", "Name (v3/v5)"),
        ],
        kind: Kind::Pure,
        run: run_uuid,
    },
    Tool {
        id: "ulid-generator",
        name: "ULID generator",
        description: "Generate sortable, lexicographically ordered ULIDs.",
        category: Category::Crypto,
        keywords: &["ulid", "unique", "identifier", "sortable", "generator"],
        fields: &[
            Field::number("count", "Quantity", "1"),
            Field::select("format", "Format", &["Raw", "JSON"], "Raw"),
        ],
        kind: Kind::Pure,
        run: run_ulid,
    },
    Tool {
        id: "jwt-parser",
        name: "JWT parser",
        description: "Decode the header and payload claims of a JSON Web Token.",
        category: Category::Crypto,
        keywords: &["jwt", "token", "json", "web", "decode", "parser", "claims"],
        fields: &[Field::area("jwt", "JWT to decode")],
        kind: Kind::Pure,
        run: run_jwt,
    },
    Tool {
        id: "basic-auth-generator",
        name: "Basic auth generator",
        description: "Build a base64 basic authentication header from a user and password.",
        category: Category::Crypto,
        keywords: &["basic", "auth", "authorization", "header", "base64", "http"],
        fields: &[
            Field::text("username", "Username"),
            Field::text("password", "Password"),
        ],
        kind: Kind::Pure,
        run: run_basic_auth,
    },
    Tool {
        id: "string-obfuscator",
        name: "String obfuscator",
        description: "Obfuscate a string by keeping only its first and last characters.",
        category: Category::Crypto,
        keywords: &["obfuscate", "mask", "hide", "secret", "redact"],
        fields: &[
            Field::area("text", "String to obfuscate"),
            Field::number("keep_first", "Keep first", "4"),
            Field::number("keep_last", "Keep last", "4"),
            Field::bool("keep_space", "Keep spaces", "true"),
        ],
        kind: Kind::Pure,
        run: run_obfuscator,
    },
    Tool {
        id: "pdf-signature-checker",
        name: "PDF signature checker",
        description: "Verify the digital signatures embedded in a PDF file.",
        category: Category::Crypto,
        keywords: &["pdf", "signature", "certificate", "verify", "sign"],
        fields: &[],
        kind: Kind::Custom,
        run: run_pdf_signature,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> Args {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ------------------------------------------------------------- hash-text
    #[test]
    fn convert_hex_to_bin_vectors() {
        assert_eq!(convert_hex_to_bin("").unwrap(), "");
        assert_eq!(convert_hex_to_bin("FF").unwrap(), "11111111");
        assert_eq!(
            convert_hex_to_bin(&"F".repeat(200)).unwrap(),
            "1111".repeat(200)
        );
        assert_eq!(
            convert_hex_to_bin("2123006AD00F694CE120").unwrap(),
            "00100001001000110000000001101010110100000000111101101001010011001110000100100000"
        );
        assert!(convert_hex_to_bin("zz").is_err());
    }

    #[test]
    fn known_digests_of_hello() {
        let h = |algo| hex_encode(&hash_bytes(algo, b"Hello").unwrap());
        assert_eq!(h("MD5"), "8b1a9953c4611296a827abf8c47804d7");
        assert_eq!(h("SHA1"), "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
        assert_eq!(
            h("SHA256"),
            "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
        );
        assert_eq!(
            h("SHA224"),
            "4149da18aa8bfc2b1e382c6c26556d01a92c261b6436dad5e3be3fcc"
        );
        assert_eq!(
            h("SHA512"),
            "3615f80c9d293ed7402687f94b22d58e529b8cc7916f8fac7fddf7fbd5af4cf777d3d795a7a00a16bf7e7f3fb9561ee9baae480da9fe7a18769e71886b03f315"
        );
        assert_eq!(h("RIPEMD160"), "d44426aca8ae0a69cdbc4021c64fa5ad68ca32fe");
        assert!(hash_bytes("SHA3", b"Hello").unwrap().len() == 64);
        assert!(hash_bytes("nope", b"Hello").is_err());
    }

    #[test]
    fn empty_string_digests() {
        assert_eq!(
            hex_encode(&hash_bytes("MD5", b"").unwrap()),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            hex_encode(&hash_bytes("SHA1", b"").unwrap()),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        // Keccak-512 of the empty string (crypto-js SHA3 default).
        assert_eq!(
            hex_encode(&hash_bytes("SHA3", b"").unwrap()),
            "0eab42de4c3ceb9235fc91acffe746b29c29a8c366b7c60e4e67c466f36a4304c00fa9caf9d87976ba469bcbe06713b435f091ef2769fb160cdab33d3670680e"
        );
    }

    #[test]
    fn hash_text_encodings() {
        let out = run_hash_text(&args(&[("text", "Hello"), ("encoding", "Base64")])).unwrap();
        assert!(out.contains("MD5: ixqZU8RhEpaoJ6v4xHgE1w=="), "{out}");
        let out = run_hash_text(&args(&[("text", "Hello"), ("encoding", "Base64url")])).unwrap();
        assert!(out.contains("MD5: ixqZU8RhEpaoJ6v4xHgE1w"), "{out}");
        let out = run_hash_text(&args(&[("text", "Hello"), ("encoding", "Bin")])).unwrap();
        assert!(out.contains("MD5: 10001011000110101001"), "{out}");
        assert!(run_hash_text(&args(&[("encoding", "rot13")])).is_err());
        // every algorithm is listed
        let out = run_hash_text(&args(&[])).unwrap();
        assert_eq!(out.lines().count(), HASH_ALGOS.len());
    }

    // ---------------------------------------------------------------- hmac
    #[test]
    fn hmac_rfc_vectors() {
        // RFC 2202 test case 1
        let key = vec![0x0bu8; 16];
        assert_eq!(
            hex_encode(&hmac_bytes("MD5", &key, b"Hi There").unwrap()),
            "9294727a3638bb1c13f48ef8158bfc9d"
        );
        // RFC 4231 test case 2
        assert_eq!(
            hex_encode(&hmac_bytes("SHA256", b"Jefe", b"what do ya want for nothing?").unwrap()),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex_encode(&hmac_bytes("SHA1", b"Jefe", b"what do ya want for nothing?").unwrap()),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
    }

    #[test]
    fn hmac_tool() {
        let out = run_hmac(&args(&[
            ("text", "what do ya want for nothing?"),
            ("secret", "Jefe"),
            ("algo", "SHA256"),
        ]))
        .unwrap();
        assert_eq!(
            out,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert!(run_hmac(&args(&[("algo", "nope")])).is_err());
    }

    // -------------------------------------------------------------- bcrypt
    #[test]
    fn bcrypt_hash_and_verify() {
        // Fixed vector: this cost-10 hash verifies "password" and nothing else.
        let hash = "$2b$10$TBE7NUU3/Wl0F9..9h8iBe6trDPbRxdGiDbJjgQWgn5hSJ7Ags32W";
        let out = run_bcrypt(&args(&[
            ("text", "whatever"),
            ("rounds", "4"),
            ("compare_text", "password"),
            ("compare_hash", hash),
        ]))
        .unwrap();
        assert!(out.contains("Compare string with hash:\nYes"), "{out}");

        let out = run_bcrypt(&args(&[
            ("rounds", "4"),
            ("compare_text", "wrong"),
            ("compare_hash", hash),
        ]))
        .unwrap();
        assert!(out.contains("Compare string with hash:\nNo"), "{out}");
    }

    #[test]
    fn bcrypt_round_trip_and_bounds() {
        let out = run_bcrypt(&args(&[("text", "secret"), ("rounds", "4")])).unwrap();
        let produced = out.lines().nth(1).unwrap();
        assert!(bcrypt::verify("secret", produced).unwrap());
        // Below 4 is clamped up to 4, as bcryptjs does; above 15 is refused,
        // but only inside the Hash panel — Compare still answers.
        assert!(
            run_bcrypt(&args(&[("rounds", "1")]))
                .unwrap()
                .contains("$2")
        );
        let out = run_bcrypt(&args(&[("rounds", "99")])).unwrap();
        assert!(out.contains("Hash:\n  (salt rounds above 15"), "{out}");
        assert!(out.contains("Compare string with hash:\nNo"), "{out}");
    }

    // ---------------------------------------------------------- encryption
    // Interop vectors: produced by crypto-js 4.2 itself (the library it-tools
    // uses) via `CryptoJS.<Algo>.encrypt('Lorem ipsum dolor sit amet', 'my
    // secret key').toString()`. The AES and TripleDES ones were then confirmed
    // byte-for-byte against the OpenSSL CLI, e.g.
    //   echo "<b64>" | openssl enc -d -aes-256-cbc -a -md md5 -k "my secret key"
    // RC4 cannot be checked that way: OpenSSL's `enc -rc4` derives a 16 byte
    // key, crypto-js a 32 byte one, so crypto-js is the sole authority there.
    const CJS_TEXT: &str = "Lorem ipsum dolor sit amet";
    const CJS_SECRET: &str = "my secret key";
    const CJS_VECTORS: [(&str, &str); 3] = [
        (
            "AES",
            "U2FsdGVkX193Qa6b5hgK6cEkMG5p5tSla0zqq+1A6SyuNZ0KaaoeifnJNT5ZkeLd",
        ),
        (
            "TripleDES",
            "U2FsdGVkX1/F97NFaiJVK9aT19wISeGLPJJK1pswgxHekBrkAyjN1BSuWrr8cFWV",
        ),
        (
            "RC4",
            "U2FsdGVkX1+Dxw/H5I7eOjggKuZDy6Fl5S9CUDG5yRaMuoxJD8i6x7J0",
        ),
    ];

    #[test]
    fn decrypts_crypto_js_ciphertext() {
        for (algo, payload) in CJS_VECTORS {
            assert_eq!(
                crypto_js_decrypt(algo, CJS_SECRET, payload).unwrap(),
                CJS_TEXT,
                "{algo}"
            );
            // ...and through the tool, whose Decrypt panel is always live.
            let out = run_encryption(&args(&[
                ("decrypt_algo", algo),
                ("decrypt_text", payload),
                ("decrypt_secret", CJS_SECRET),
            ]))
            .unwrap();
            assert!(
                out.contains(&format!("Decrypt:\n{CJS_TEXT}")),
                "{algo}: {out}"
            );
        }
    }

    /// Both cards are always rendered, and a failing one only reports itself.
    #[test]
    fn encryption_panels_are_independent() {
        let out = run_encryption(&args(&[
            ("text", "hello"),
            ("secret", "pw"),
            ("decrypt_text", "not base64 !!!"),
            ("decrypt_secret", "pw"),
        ]))
        .unwrap();
        assert!(out.contains("Encrypt:\nU2FsdGVkX1"), "{out}");
        assert!(
            out.contains("Decrypt:\n  (ciphertext is not valid base64"),
            "{out}"
        );
    }

    #[test]
    fn evp_bytes_to_key_vector() {
        // Same derivation crypto-js's EvpKDF performs for the salt 0102030405060708.
        let kdf = evp_bytes_to_key(CJS_SECRET.as_bytes(), &[1, 2, 3, 4, 5, 6, 7, 8], 32, 16);
        assert_eq!(
            hex_encode(&kdf),
            "64a425c9fac4cfbfc73042e5005633b3ec4bc35ac7f20af686faf24823c006c5\
             8d858c00f23d97879b0faf84de65603a"
        );
    }

    #[test]
    fn encryption_round_trips() {
        for algo in ENCRYPT_ALGOS {
            for text in [
                "",
                "a",
                CJS_TEXT,
                &"x".repeat(1000),
                "unicode: héllo · 世界",
            ] {
                let ct = crypto_js_encrypt(algo, "pw", text).unwrap();
                assert!(ct.starts_with("U2FsdGVkX1"), "{algo}: {ct}");
                assert_eq!(crypto_js_decrypt(algo, "pw", &ct).unwrap(), text, "{algo}");
            }
        }
    }

    #[test]
    fn encryption_rejects_bad_input() {
        let decrypt = |algo: &str, text: &str, secret: &str| crypto_js_decrypt(algo, secret, text);
        // Wrong password on the fixed vectors: an Err, never a panic.
        for (algo, payload) in CJS_VECTORS {
            assert!(
                decrypt(algo, payload, "not the password").is_err(),
                "{algo}"
            );
        }
        assert!(decrypt("AES", "not base64 !!!", CJS_SECRET).is_err());
        assert!(decrypt("AES", "", CJS_SECRET).is_err());
        // valid base64 but no Salted__ header
        assert!(decrypt("AES", &STANDARD.encode([0u8; 32]), CJS_SECRET).is_err());
        // truncated ciphertext -> not a whole number of blocks
        assert!(decrypt("AES", &CJS_VECTORS[0].1[..24], CJS_SECRET).is_err());
        // Rabbit is in it-tools' list but has no Rust crate; it must not be
        // silently swapped for another cipher.
        assert!(crypto_js_encrypt("Rabbit", "pw", "x").is_err());
        assert!(!ENCRYPT_ALGOS.contains(&"Rabbit"));
    }

    // ------------------------------------------------------------------ rsa
    #[test]
    fn rsa_guard_does_no_work_when_generate_is_unset() {
        for pairs in [vec![], vec![("bits", "4096")], vec![("generate", "false")]] {
            let out = run_rsa(&args(&pairs)).unwrap();
            assert_eq!(out, "set generate to true to produce a key pair");
        }
    }

    #[test]
    fn rsa_generates_parseable_pem() {
        use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
        // 512 bits only: anything larger makes the test suite crawl.
        let out = run_rsa(&args(&[("bits", "512"), ("generate", "true")])).unwrap();
        // Public key panel first, as upstream.
        assert!(
            out.starts_with("Public key:\n-----BEGIN PUBLIC KEY-----"),
            "{out}"
        );
        let (public, private) = out.split_once("Private key:\n").unwrap();
        let public = public.trim_start_matches("Public key:\n").trim_end();
        let key = rsa::RsaPrivateKey::from_pkcs8_pem(private).unwrap();
        assert_eq!(rsa::traits::PublicKeyParts::size(&key) * 8, 512);
        assert_eq!(
            rsa::RsaPublicKey::from_public_key_pem(public).unwrap(),
            key.to_public_key()
        );
        assert!(run_rsa(&args(&[("bits", "777"), ("generate", "true")])).is_err());
    }

    // ----------------------------------------------------- token-generator
    #[test]
    fn token_generator_charsets() {
        let t = |pairs: &[(&str, &str)]| run_token(&args(pairs)).unwrap();
        assert_eq!(
            t(&[
                ("length", "10"),
                ("uppercase", "false"),
                ("lowercase", "false"),
                ("numbers", "false"),
                ("symbols", "false")
            ])
            .len(),
            0
        );
        for len in [5usize, 10, 100] {
            let token = t(&[("length", &len.to_string()), ("symbols", "true")]);
            assert_eq!(token.chars().count(), len);
        }
        let upper = t(&[
            ("length", "256"),
            ("lowercase", "false"),
            ("numbers", "false"),
        ]);
        assert!(upper.chars().all(|c| c.is_ascii_uppercase()), "{upper}");
        let lower = t(&[
            ("length", "256"),
            ("uppercase", "false"),
            ("numbers", "false"),
        ]);
        assert!(lower.chars().all(|c| c.is_ascii_lowercase()), "{lower}");
        let digits = t(&[
            ("length", "256"),
            ("uppercase", "false"),
            ("lowercase", "false"),
        ]);
        assert!(digits.chars().all(|c| c.is_ascii_digit()), "{digits}");
        let symbols = t(&[
            ("length", "256"),
            ("uppercase", "false"),
            ("lowercase", "false"),
            ("numbers", "false"),
            ("symbols", "true"),
        ]);
        assert!(symbols.chars().all(|c| SYMBOLS.contains(c)), "{symbols}");
        assert!(run_token(&args(&[("length", "-1")])).is_err());
    }

    // ------------------------------------------- password-strength-analyser
    #[test]
    fn charset_length_vectors() {
        assert_eq!(charset_length("abcdefghijklmnopqrstuvwxyz"), 26);
        assert_eq!(charset_length("ABCDEFGHIJKLMNOPQRSTUVWXYZ"), 26);
        assert_eq!(charset_length("0123456789"), 10);
        assert_eq!(charset_length("-_("), 32);
        assert_eq!(charset_length(""), 0);
        assert_eq!(charset_length("abcdefghijklmnopqrstuvwxyz0123456789"), 36);
        assert_eq!(
            charset_length("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_("),
            94
        );
    }

    #[test]
    fn password_strength_output() {
        let out = run_password_strength(&args(&[("password", "0123456789")])).unwrap();
        assert!(out.contains("character set size: 10"), "{out}");
        assert!(out.contains("password length: 10"), "{out}");
        // log2(10) * 10 = 33.22 bits
        assert!(out.contains("entropy: 33.22"), "{out}");
        assert!(out.contains("score: 26 / 100"), "{out}");
        assert!(
            out.contains("duration to crack this password with brute force: "),
            "{out}"
        );
        let empty = run_password_strength(&args(&[("password", "")])).unwrap();
        assert!(empty.contains("entropy: 0.00"), "{empty}");
        assert!(empty.contains("score: 0 / 100"), "{empty}");
    }

    #[test]
    fn human_duration_vectors() {
        assert_eq!(human_duration(0.0), "Instantly");
        assert_eq!(human_duration(0.5), "Less than a second");
        assert_eq!(human_duration(90.0), "1 minute, 30 seconds");
        assert_eq!(human_duration(3661.0), "1 hour, 1 minute");
        assert_eq!(human_duration(31_536_000.0 * 3.0), "3 years");
    }

    // ------------------------------------------------------------ otp / totp
    #[test]
    fn hex_to_bytes_vectors() {
        assert_eq!(hex_to_bytes("1").unwrap(), vec![1]);
        assert_eq!(hex_to_bytes("ffffff").unwrap(), vec![255, 255, 255]);
        assert_eq!(hex_to_bytes("000000000").unwrap(), vec![0, 0, 0, 0, 0]);
        assert_eq!(
            hex_to_bytes("a3218bcef89").unwrap(),
            vec![163, 33, 139, 206, 248, 9]
        );
        assert_eq!(hex_to_bytes("063679ca").unwrap(), vec![6, 54, 121, 202]);
        assert_eq!(
            hex_to_bytes("0102030405060708090a0b0c0d0e0f").unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
        assert!(hex_to_bytes("zz").is_err());
    }

    #[test]
    fn base32_to_hex_vectors() {
        assert_eq!(base32_to_hex("ABCDEF").unwrap(), "00443205");
        assert_eq!(base32_to_hex("7777").unwrap(), "ffff0f");
        assert_eq!(
            base32_to_hex("JBSWY3DPEHPK3PXP").unwrap(),
            "48656c6c6f21deadbeef"
        );
        assert_eq!(base32_to_hex("ABC").unwrap(), base32_to_hex("abc").unwrap());
        assert!(base32_to_hex("1").is_err());
    }

    const OTP_KEY: &str = "JBSWY3DPEHPK3PXP";
    const HOTP_CODES: [&str; 5] = ["282760", "996554", "602287", "143627", "960129"];

    #[test]
    fn generate_hotp_vectors() {
        for (counter, code) in HOTP_CODES.iter().enumerate() {
            assert_eq!(&generate_hotp(OTP_KEY, counter as u64).unwrap(), code);
        }
    }

    #[test]
    fn totp_vectors_at_fixed_timestamps() {
        for (token, now) in [
            ("282760", 0i64),
            ("341128", 1_465_324_707_000),
            ("089029", 1_365_324_707_000),
        ] {
            let out = run_otp(&args(&[
                ("secret", OTP_KEY),
                ("timestamp_ms", &now.to_string()),
            ]))
            .unwrap();
            assert!(out.contains(&format!("current:  {token}")), "{out}");
        }
    }

    /// The three codes shown side by side are the counter's neighbours — this is
    /// how the upstream UI lets you validate a code you were given.
    #[test]
    fn totp_shows_previous_current_next() {
        let out = run_otp(&args(&[("secret", OTP_KEY), ("timestamp_ms", "30000")])).unwrap();
        assert!(
            out.contains(&format!("previous: {}", HOTP_CODES[0])),
            "{out}"
        );
        assert!(
            out.contains(&format!("current:  {}", HOTP_CODES[1])),
            "{out}"
        );
        assert!(
            out.contains(&format!("next:     {}", HOTP_CODES[2])),
            "{out}"
        );
        assert!(out.contains("Iteration count:\n  1"), "{out}");
        assert!(
            out.contains("Iteration count, padded hex:\n  0000000000000001"),
            "{out}"
        );
        assert!(out.contains("Epoch:\n  30"), "{out}");
        assert!(
            out.contains("Secret in hexadecimal:\n48656c6c6f21deadbeef"),
            "{out}"
        );
    }

    #[test]
    fn otp_key_uri_and_empty_secret() {
        let out = run_otp(&args(&[("secret", OTP_KEY)])).unwrap();
        assert!(
            out.contains(
                "otpauth://totp/IT-Tools:demo-user?issuer=IT-Tools&\
                 secret=JBSWY3DPEHPK3PXP&algorithm=SHA1&digits=6&period=30"
            ),
            "{out}"
        );
        assert_eq!(
            build_key_uri(OTP_KEY, "IT-Tools", "demo-user", 6, 30),
            "otpauth://totp/IT-Tools:demo-user?issuer=IT-Tools&secret=JBSWY3DPEHPK3PXP&algorithm=SHA1&digits=6&period=30"
        );
        // No secret: nothing at all is computable, so this one is a real Err.
        assert!(run_otp(&args(&[])).is_err());
        // A non-base32 secret still shows the key URI, epoch and counter.
        let out = run_otp(&args(&[("secret", "111"), ("timestamp_ms", "0")])).unwrap();
        assert!(out.contains("Codes:\n  (not a base32 character"), "{out}");
        assert!(out.contains("secret=111"), "{out}");
    }

    // ------------------------------------------------------------- bip39
    #[test]
    fn bip39_known_vectors() {
        // BIP39 English test vector: all-zero entropy.
        let out = run_bip39(&args(&[("entropy", "00000000000000000000000000000000")])).unwrap();
        assert!(
            out.contains(
                "mnemonic: abandon abandon abandon abandon abandon abandon abandon abandon \
                 abandon abandon abandon about"
            ),
            "{out}"
        );
        let out = run_bip39(&args(&[(
            "mnemonic",
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
        )]))
        .unwrap();
        assert!(
            out.contains("entropy: 7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f"),
            "{out}"
        );
    }

    #[test]
    fn bip39_random_and_errors() {
        // empty args -> random 16 byte entropy -> 12 words
        let out = run_bip39(&args(&[])).unwrap();
        let mnemonic = out.lines().nth(1).unwrap().trim_start_matches("mnemonic: ");
        assert_eq!(mnemonic.split_whitespace().count(), 12);
        assert!(run_bip39(&args(&[("entropy", "aabb")])).is_err());
        assert!(run_bip39(&args(&[("entropy", "zz")])).is_err());
        assert!(run_bip39(&args(&[("mnemonic", "not a real mnemonic")])).is_err());
    }

    // -------------------------------------------------------------- uuid/ulid
    #[test]
    fn uuid_versions() {
        assert_eq!(
            run_uuid(&args(&[("version", "NIL")])).unwrap().trim(),
            NIL_UUID
        );
        let out = run_uuid(&args(&[("count", "5")])).unwrap();
        assert_eq!(out.lines().count(), 5);
        assert!(out.lines().all(|l| uuid::Uuid::parse_str(l).is_ok()));
        // RFC 4122 name-based vectors for the DNS namespace and "example.com"
        let dns = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        assert_eq!(
            uuid_name_based(dns, "example.com", 3).unwrap(),
            "9073926b-929f-31c2-abc9-fad77ae3e8eb"
        );
        assert_eq!(
            uuid_name_based(dns, "example.com", 5).unwrap(),
            "cfbff0d1-9375-5685-968c-48ce8b15ae17"
        );
        assert!(run_uuid(&args(&[("version", "v3"), ("namespace", "nope")])).is_err());
        assert!(run_uuid(&args(&[("version", "v9")])).is_err());
        // it-tools offers NIL and v1/v3/v4/v5 only — no v7, no max UUID.
        assert!(run_uuid(&args(&[("version", "v7")])).is_err());
        assert!(run_uuid(&args(&[("count", "0")])).is_err());
        assert!(run_uuid(&args(&[("count", "51")])).is_err());
        for v in ["v1", "v4"] {
            let id = run_uuid(&args(&[("version", v)])).unwrap();
            assert!(uuid::Uuid::parse_str(id.trim()).is_ok(), "{v}: {id}");
        }
    }

    #[test]
    fn ulid_formats() {
        let out = run_ulid(&args(&[("count", "3")])).unwrap();
        assert_eq!(out.lines().count(), 3);
        assert!(out.lines().all(|l| ulid::Ulid::from_string(l).is_ok()));
        let json = run_ulid(&args(&[("count", "2"), ("format", "JSON")])).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(run_ulid(&args(&[("format", "XML")])).is_err());
        assert!(run_ulid(&args(&[("count", "1000")])).is_err());
    }

    // ---------------------------------------------------------------- jwt
    #[test]
    fn jwt_decode() {
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
                   eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
                   SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = run_jwt(&args(&[("jwt", jwt)])).unwrap();
        // Every claim, with the IANA/RFC 7518 human-readable names the table shows.
        assert!(
            out.contains("alg (Algorithm): HS256 (HMAC using SHA-256)"),
            "{out}"
        );
        assert!(out.contains("typ (Type): JWT"), "{out}");
        assert!(out.contains("sub (Subject): 1234567890"), "{out}");
        assert!(out.contains("name (Full name): John Doe"), "{out}");
        assert!(
            out.contains("iat (Issued At): 1516239022 (2018-01-18 01:30:22 UTC)"),
            "{out}"
        );
        assert!(out.starts_with("Header:\n"), "{out}");
        assert!(out.contains("\nPayload:\n"), "{out}");
        assert!(run_jwt(&args(&[("jwt", "garbage")])).is_err());
        assert!(run_jwt(&args(&[])).is_err());
        // An unparseable segment reports itself in its own panel.
        let out = run_jwt(&args(&[("jwt", "aaaa.bbbb.cccc")])).unwrap();
        assert!(out.contains("Header:\n  ("), "{out}");
        assert!(out.contains("Payload:\n  ("), "{out}");
        // An unknown claim is still listed, just without a description.
        let out = run_jwt(&args(&[(
            "jwt",
            "eyJhbGciOiJub25lIn0.eyJteV9jbGFpbSI6eyJhIjpbMV19fQ.",
        )]))
        .unwrap();
        assert!(out.contains("my_claim: {"), "{out}");
        assert!(
            out.contains("alg (Algorithm): none (No digital signature or MAC performed)"),
            "{out}"
        );
    }

    // --------------------------------------------------------- basic auth
    #[test]
    fn basic_auth_header() {
        assert_eq!(
            run_basic_auth(&args(&[
                ("username", "Aladdin"),
                ("password", "open sesame")
            ]))
            .unwrap(),
            "Authorization: Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert_eq!(
            run_basic_auth(&args(&[])).unwrap(),
            "Authorization: Basic Og=="
        );
    }

    // ---------------------------------------------------- string obfuscator
    #[test]
    fn obfuscate_vectors() {
        assert_eq!(obfuscate("1234567890", '*', 4, 0, true), "1234******");
        assert_eq!(obfuscate("1234567890", 'x', 4, 0, true), "1234xxxxxx");
        assert_eq!(obfuscate("1234567890", '*', 5, 0, true), "12345*****");
        assert_eq!(obfuscate("1234567890", '*', 0, 5, true), "*****67890");
        assert_eq!(obfuscate("1234567890", '*', 5, 5, true), "1234567890");
        assert_eq!(obfuscate("1234567890", 'x', 2, 2, true), "12xxxxxx90");
        assert_eq!(obfuscate("12345 67890", '*', 4, 0, true), "1234* *****");
        assert_eq!(obfuscate("12345 67890", '*', 4, 0, false), "1234*******");
    }

    #[test]
    fn obfuscator_tool() {
        let out = run_obfuscator(&args(&[("text", "1234567890"), ("keep_last", "0")])).unwrap();
        assert_eq!(out, "1234******");
        // No replacement-character field upstream: always '*'.
        assert!(
            !TOOLS
                .iter()
                .find(|t| t.id == "string-obfuscator")
                .unwrap()
                .fields
                .iter()
                .any(|f| f.key == "replacement")
        );
        assert_eq!(run_obfuscator(&args(&[])).unwrap(), "");
    }

    // ------------------------------------------------------------- registry
    #[test]
    fn custom_tools_explain_themselves() {
        for tool in TOOLS.iter().filter(|t| t.kind == Kind::Custom) {
            let err = (tool.run)(&Args::new()).unwrap_err().to_string();
            assert!(err.contains("needs bespoke UI"), "{}: {err}", tool.id);
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate tool id");
    }

    #[test]
    fn no_tool_panics_on_empty_args() {
        for tool in TOOLS {
            let _ = (tool.run)(&Args::new());
            // and on garbage in every declared field
            let garbage: Args = tool
                .fields
                .iter()
                .map(|f| (f.key.to_string(), "\u{fffd}!!".into()))
                .collect();
            let _ = (tool.run)(&garbage);
        }
    }
}

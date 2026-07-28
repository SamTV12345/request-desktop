# it-tools Pull Requests — Kandidatenliste

Stand: 28.07.2026, 326 offene PRs in `CorentinTh/it-tools`. Das Projekt merged
nichts mehr, die PRs sind also eine Fundgrube und keine Warteschlange.

**Kriterien**, nach denen hier sortiert wird:

1. Passt es in die Registry-Form `fields in → text out` (`crates/devtools`)?
   Alles was Kamera, Canvas, Dateiauswahl oder einen Netzwerkaufruf braucht,
   passt nicht — das wäre `Kind::Custom` plus eine eigene Pane in `crates/app`.
2. Kostet es eine neue Dependency, und ist die ihr Gewicht wert?
3. Ist es *interessant* — also etwas, das man beim Debuggen von HTTP wirklich
   braucht. Kein weiterer nationaler ID-Validator, kein Trinkgeldrechner.
4. Haben wir es schon? Viele PRs sind Verbesserungen an Tools, die längst
   portiert sind — die stehen unten in einer eigenen Kategorie.

Aufwand: **S** = ein Nachmittag im vorhandenen Muster, **M** = eigener Parser
oder Datentabelle, **L** = größere Sache oder eigene UI.

---

## 0. Bereits portiert (`crates/devtools/community/`)

| PR | Tool |
|:--|:--|
| — | Fake data generator (kein Upstream-PR, neu) |
| #1260 | Passphrase Generator |
| #1261 | PIN Code Generator |
| #1138 | Nano ID Generator |
| #1453 | JWT Generator |
| #1140 | CRC Calculator |
| #1463, #1233 | Mongo ObjectId Generator/Parser |
| #1211 | Snowflake ID Parser |
| #1320 | Luhn Checker |
| #1348 | Floating Point Converter |
| #1166 | JSON to JSON Schema |
| #1323 | JSON Size Analyzer |
| #1531 | HAR Sanitizer |
| #1303 | List Comparison |
| #1182 | Sensitive Data Masker |
| #1807 | Markdown Table Generator |
| #1258 | Punycode Converter |
| #1695, #929 | GZIP String Converter |
| #1289, #948 | Data Storage Unit Converter |
| #1456 | Geo Distance Computer |
| #1304 | SLA Computer |
| #1336 | Week Numbers Converter |
| #1626 | Conventional Commits Cheatsheet |

---

## 1. Erledigt — komplett abgearbeitet (28.07.2026)

40 Tools, keine einzige neue Dependency. Alles reine Textfunktionen, alle nötigen
Crates sind schon da (`ipnet`, `regex`, `serde_json`, `pulldown-cmark`, `hmac`,
`bcrypt`, `chrono`, `csv`).

### 1a. Netzwerk — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #871 | CIDR → IP Range | `cidr-to-ip-range` |
| #869 | IP Range → CIDR | `ip-range-to-cidr` |
| #993 | Ist diese IP in dem Netz? | `ip-in-cidr` |
| #1451 | Subnetz-Ausschluss-Rechner | `subnet-exclude-calculator` |
| #963, #944, #1465 | IPv6 Subnetz-Rechner | `ipv6-subnet-calculator` |
| #870 | IPv6 Address Converter | `ipv6-address-converter` |
| #1264 | Integer ↔ IP | `integers-to-ip` |
| #867 | MAC Address Converter | `mac-address-converter` |
| #1029 | Port-Lookup | `port-info-search` |

### 1b. URLs & Web — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #932 | URL Defanger/Refanger | `url-defanger` |
| #959 | URL Cleaner | `url-cleaner` |
| #962 | UTM URL Generator | `utm-url-generator` |
| #918 | URL Text Fragment Generator | `url-text-fragment-generator` |
| #1524 | SharePoint URL Decoder | `sharepoint-url-decoder` |

### 1c. JSON & Code-Generierung — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #1002, #1031, #1102, #1192, #1105, #1246, #1811 | JSON → Typen, sechs Zielsprachen in *einem* Tool | `json-to-types` |
| #966 | JSON Escape/Unescape | `json-escape-unescape` |

### 1d. Text — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #1171, #1811 | cURL → .http-Request | `curl-to-http` |
| #1307 | Stacktrace Formatter | `stacktrace-formatter` |
| #1290, #976 | Smart Text Replacer + Zeilenumbrüche | `smart-text-replacer` |
| #1106 | Markdown TOC Generator | `markdown-toc-generator` |
| #1021 | Morse-Konverter | `morse-converter` |
| #1020 | Ordnerbaum-Diagramm | `folder-tree-diagram` |
| #1266 | AI Prompt Splitter | `ai-prompt-splitter` |
| #930 | MIME Encoder/Decoder | `mime-encoder-decoder` |

### 1e. Krypto & Auth — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #1515 | HOTP (zählerbasiert) | `hotp-generator` |
| #902 | htpasswd-Generator + Prüfer | `htpasswd-generator` |
| #1013 | RSA Ver-/Entschlüsselung | `rsa-encryption` |

### 1f. Rechner & Einheiten — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #1324, #984 | Dauer / Datum+Dauer / Tagesdifferenz | `date-duration-calculator` |
| #1033 | TTL-Rechner | `ttl-calculator` |
| #1218 | RAID-Rechner | `raid-calculator` |
| #1022 | HDD-Größenrechner | `hdd-size-calculator` |
| #1532 | Würfelwurf mit `2d6+3`-Notation | `dice-roller` |

### 1g. Sonstiges — **erledigt** (28.07.2026)

| PR | Tool | id |
|:--|:--|:--|
| #1530, #1525 | Hex ↔ Base64 ↔ Text Konverter | `hex-base64-converter` |
| #1322 | Smart Raw Error Converter | **gestrichen** — `stacktrace-formatter` und `smart-text-replacer` decken es ab |

---

## 2. Lohnt sich, kostet aber eine Dependency oder echte Arbeit

| PR | Tool | Kosten | Bewertung |
|:--|:--|:--|:--|
| #1284 | Timezone Converter | `chrono-tz` (reine Datentabelle) | ja, gute Ergänzung zu `date-time-converter` |
| #1295, #1183 | Unicode-Suche / Zeichennamen | `unicode-names2` | ja, „welches Zeichen ist das“ ist eine echte Frage |
| #904, #632, #1811 | HTML → Markdown | HTML-Parser (`html5ever`/`scraper`) | ja, macht auch #1075 und #961 möglich |
| #1075 | Text aus HTML extrahieren | derselbe Parser | ja, danach klein |
| #961 | HTML Cleaner | derselbe Parser | ja, danach klein |
| #1327 | Länder / ISO 3166 Suche | ~250 Zeilen eingebettete Tabelle | ja, Referenz-Pane kann das anzeigen |
| #1330 | Allgemeiner Einheiten-Konverter | Tabelle + Parser | vielleicht, Überschneidung mit 1f |
| #1331 | iCal Generator/Parser | eigener Parser, aber reiner Text | vielleicht |
| #1262 | jq / JSONPath Tester | `jaq` oder `jsonpath-rust` | ja, stark für eine HTTP-App |
| #917, #1306 | Zertifikat-Parser & -Konverter | `x509-parser` | ja — TLS-Debugging gehört zu dieser App |
| #887 | Ed25519 Keypair | `ed25519-dalek` | ja, klein |
| #1027, #1108 | ECDSA Keygen / Signieren & Prüfen | `p256`/`ecdsa` | ja, mittelgroß |
| #363 | Argon2 Hashing | `argon2` (klein, sauber) | ja, ergänzt bcrypt |
| #1259 | WPA PSK Raw Key | PBKDF2-SHA1, `pbkdf2` oder von Hand | ja, klein |
| #985 | SIP-Authentifizierung | nur MD5-Digest | nischig, aber billig |
| #1305 | Bounce-Mail-Parser | eigener Parser | nischig |
| #1254 | E-Mail-Parser (EML) | eigener Parser; `.msg` ist binär → raus | halbes ja |
| #1457 | OVH X-VR-SPAMCAUSE Decoder | 20 Zeilen | nur wenn man OVH nutzt |
| #1014 | Nginx-Config-Formatter | eigener Parser | vielleicht |
| #847 | Docker Compose → docker run | Umkehrung des vorhandenen Tools | ja |
| #864 | Docker Compose Format Converter | YAML-Umbau | ja |
| #890, #889 | Compose / docker run → Kubernetes | YAML-Umbau, größer | vielleicht |
| #1452 | Dockerfile Linter | Regelwerk selbst schreiben | vielleicht |
| #820 | Spring-Properties ↔ YAML | flach ↔ verschachtelt, klein | ja |
| #1016 | Dateityp-Erkennung (Magic Bytes) | Tabelle; Hex-Eingabe statt Datei | ja, in der Textform |
| #1372 | Energiekosten-Rechner | trivial | wenn Lust |
| #1201 | Torrent → Magnet | Bencode-Parser, braucht Datei | eher Custom-Pane |

---

## 3. Braucht eine eigene Pane (`Kind::Custom`) — machbar, aber UI-Arbeit

Wir haben das Muster schon (`crates/app/src/custom/`: base64-file, bcrypt,
chmod, emoji-picker), es ist nur jedes Mal echte GPUI-Arbeit.

| PR | Tool | Braucht |
|:--|:--|:--|
| #535, #1141 | Datei-Hasher | Dateiauswahl + Fortschritt |
| #1263 | Hex-Datei-Konverter | Dateiauswahl |
| #1103 | JSON-Editor | Baum-Widget |
| #1019, #1753, #1230 | Markdown-Editor / -Vorschau | Live-Vorschau |
| #1287 | CSS Border Generator | Live-Vorschau |
| #1265 | Color Wheel | Canvas |
| #1319 | Code Highlighter | `syntect` + gefärbte Ausgabe |
| #914 | QR-Code-Decoder | Kamera oder Bilddatei |
| #1229, #865 | EXIF lesen/entfernen | Bilddatei |
| #1276, #1328, #1018 | Bildformat-Konverter (ICO/PNG/HEIC) | Bild-Pipeline |
| #953 | Bild → ASCII-Art | Bild-Pipeline |
| #1740 | Icon-Generator | Bild-Pipeline + ZIP |
| #1167 | Potrace (Bitmap → SVG) | Bild-Pipeline |
| #1100 | Barcode Reader/Generator | Kamera + Rendering |
| #1529 | Parquet-Reader | schwere Dependency + Tabelle |
| #1318 | WYSIWYG: Farben und Tabellen | betrifft ein Tool, das wir gar nicht haben |

---

## 4. Verbesserungen an schon portierten Tools — **erledigt** (28.07.2026)

28 Pull Requests, verteilt auf 26 vorhandene Tools, keine neue Dependency.

| PR | Tool | Was sich geändert hat |
|:--|:--|:--|
| #1441 | `uuid-generator` | v6 und v7 im Versions-Select |
| #913 | `token-generator` | mehrere Tokens auf einmal, Zeichen ausschließen |
| #986 | `hash-text`, `hmac-generator` | die vier FIPS-SHA3-Varianten neben crypto-js' Keccak-512, das jetzt in der Zeile sagt, was es ist |
| #1156 | `hmac-generator` | Schlüssel als Text, Hex oder Base64 |
| #1157, #373, #1585 | `encryption` | Schlüsselkodierung und explizites IV; leeres IV = altes Verhalten |
| #888 | `rsa-key-pair-generator` | PKCS#1/PKCS#8 wählbar, Public Key zusätzlich im OpenSSH-Format |
| #901 | `integer-base-converter` | `0x`/`0b`/`0o`/`#`/`h` überstimmen die Eingabebasis, Präfixe in der Ausgabe |
| #1087, #1739, #1534, #1094 | `text-to-unicode` | Nicht-BMP korrekt, dazu Backslash-u-Escapes, geschweifte Form, Hex- und Java-Entities |
| #1085 | `text-to-binary` | arbeitet auf UTF-8-Bytes, Umlaute und Emoji gehen durch |
| #1277 | `json-to-csv` | einzelnes Objekt, verschachtelte Felder als `address.city` |
| #1542, #1539, #1643 | `json-viewer` | zusätzliches Panel für JSON, das als String in JSON steckt |
| #1434, #957 | `yaml-viewer` | große Ganzzahlen bleiben ganz, Fehler nennen Zeile und Spalte |
| #1302, #1288 | `list-converter` | Präfix/Suffix entfernen, splitten, sortieren mit „Nicht sortieren“ als Default |
| #1391 | `regex-tester` | **Fehler behoben**: nicht teilnehmende optionale Gruppen verschoben die Nummerierung |
| #1010 | `text-statistics` | Sätze, Absätze, Lesezeit, häufigste Wörter |
| #1184 | `lorem-ipsum-generator` | Latein, Deutsch, Kyrillisch, Griechisch, CJK |
| #1153, #1491 | `temperature-converter` | absoluter Nullpunkt wird abgelehnt, keine Rundungsartefakte mehr |
| #1017 | `color-converter` | nächster CSS-Farbname und WCAG-Kontrast gegen Schwarz und Weiß |
| #1104, #1439, #1326, #1479 | `wifi-qr-code-generator` | Fehlerkorrektur wählbar, WPA3 und WPA2/WPA3, Payload als kopierbares Panel |
| #763, #1204 | `git-memo` | Branches, stash, worktree, cherry-pick, rebase --onto, reflog, bisect |
| #1185 | `regex-memo` | Unicode-Klassen, non-greedy, benannte Gruppen, Flags |
| #873 | `url-parser` | **Fehler behoben**: wiederholte Query-Parameter gingen verloren |
| #1474 | `url-encoder` | `encodeURIComponent`/`encodeURI`/RFC 3986, `+` für Leerzeichen |
| #969, #1465 | `ipv4-subnet-calculator` | nutzbare Hosts, Adresstyp, `in-addr.arpa`; **Panik bei `/0` behoben** |
| #1026, #1283 | `crontab-generator` | AWS-EventBridge-Dialekt, Zeitzone (UTC und feste Offsets) |
| #964 | `chmod-calculator` | liest `755`, `rwxr-xr-x`, `u=rwx,go=rx`, plus setuid/setgid/sticky |
| #1232 | `slugify-string` | Trennzeichen wählbar, Kleinschreibung abschaltbar |
| #1107 | `phone-parser-and-formatter` | Standard-Ländervorwahl, fertige `tel:`/`sms:`/`wa.me`-Links |

Nicht umgesetzt, mit Grund:

| PR | Warum nicht |
|:--|:--|
| #1669 (JWT-Werte-Editor), #1221 (OTP-Secret speichern) | UI-Features, keine Registry-Änderung — gehören in `crates/app`, nicht in ein Tool |
| #1585 (AES-GCM) | GHASH bräuchte eine neue Dependency; eine Galois-Feld-Multiplikation für einen authentifizierten Modus schreibt man nicht von Hand. CBC bleibt, Begründung im Dateikopf |
| Zeitzonen-Namen im Cron | ohne `chrono-tz` gibt es kein DST; `Europe/Berlin` wird abgelehnt statt still als UTC gerechnet |
| #1152 (bcrypt-Fehlerzustände) | betrifft die eigene Pane in `crates/app`, nicht das Tool |

## 5. Kommt nicht in Frage

| Gruppe | PRs | Grund |
|:--|:--|:--|
| Übersetzungen / i18n | #1825, #1817, #1805, #1795, #1769, #1764, #1727, #1712, #1679, #1671, #1661, #1637, #1629, #1604, #1603, #1579, #1549, #1507, #1469, #1461, #1409, #1378, #1269, #1247, #1220, #1189, #1178, #1177, #1155, #1128, #1127, #1050, #978, #927, #885, #692, #691, #690, #689 | Request Desktop ist einsprachig |
| Dependency-/Security-Bumps | #1802, #1796, #1785, #1784, #1758, #1757, #1756, #1734, #1716, #1681, #1680, #1644, #1504, #1423, #1412, #1407, #1311, #1296, #900, #898 | npm-Baum, den wir nicht haben |
| Docker / Nginx / CI / Repo | #1819, #1670, #1645, #1574, #1518, #1499, #1443, #1426, #1394, #1298, #1282, #1209, #1186, #1170, #1003, #648, #427, #1737, #1786 | Deployment und Repo-Pflege von it-tools |
| Upstream-UI-Fixes | #1640, #1547, #1545, #1510, #1440, #1064, #846, #614 | Vue-Komponenten, andere Oberfläche |
| Braucht einen Netzwerkaufruf | #1455 (My IP), #866 (IP-Geolocation), #1371 (DNS over HTTPS), #1321 (Währungen), #919/#1255 (OCR) | Ein Tool, das im Hintergrund funkt, ist in dieser App das Falsche — dafür gibt es den Request-Editor |
| Duplikat der App selbst | #1168 (API Tester), #1278 (WebSocket Tester) | Das *ist* Request Desktop; ein WebSocket-Client gehört ggf. als Feature dorthin, nicht in „Tools“ |
| Nationale IDs & Nischenrechner | #1810 (belgische ID), #1683 (IBAN-Generator), #1329 (USt-IdNr.), #868 (ISBN), #1779 (Trinkgeld), #1523 (Beleuchtungsstärke), #1032 (RMB in Worten) | genau das Genre, das nicht rein soll |
| Browser-/Geräte-APIs | #1340 (Mikrofontest), #1602 (CLI-Editor), #1230 (Zwischenablage als Markdown) | hängt am Browser |
| Schwergewichte ohne Nutzen hier | #1011, #1012 (PGP), #910 (CSR), #915, #1812 (X.509 erzeugen), #912 (Ansible Vault), #1099, #1702 (JS-(De)Obfuscator), #1257, #1256, #1200 (XPath/XSLT), #1454 (GPT-Token-Zähler), #958 (JS/CSS-Prettier), #1563 (Mermaid-Export), #1803 (Markdown-Diff), #970, #968, #931 (PDF), #1266 → siehe 1d | jeweils eine große Dependency für einen seltenen Handgriff |
| Unklar / kaputt | #1612, #1006, #1000, #992, #1781, #1559, #1651, #1690, #1774, #1685 (figlet-Fontpfade — bei uns eingebettet), #1671 | leere oder gegenstandslose PRs |

---

## Empfehlung

Kategorien 1 und 4 sind abgearbeitet. Was bleibt:

1. **Kategorie 2** — jedes dieser Tools kostet genau eine Dependency. Die drei
   mit dem besten Verhältnis: `chrono-tz` (Zeitzonen-Konverter, und der
   Crontab-Generator könnte endlich `Europe/Berlin` rechnen), `x509-parser`
   (Zertifikat-Parser — TLS-Debugging gehört in eine HTTP-App) und ein
   HTML-Parser, der auf einen Schlag HTML→Markdown, Text-Extraktion und den
   HTML-Cleaner ermöglicht.
2. **Kategorie 3** — die Tools mit eigener Pane. Das Muster steht
   (`crates/app/src/custom/`), es ist nur jedes Mal echte GPUI-Arbeit.

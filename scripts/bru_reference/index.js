// Reference oracle for the differential test: runs Bruno's own bruToJson over a
// list of .bru files and prints what it produced.
//
// Protocol (one process for the whole corpus — spawning node 275 times is slow):
//   stdin  : JSON array of absolute file paths
//   stdout : JSON array, same order, of { path, json } or { path, error }
//
// The parser comes from the published `@usebruno/lang` package (pinned in
// package.json). `main` is plain CommonJS, so there is no build step. Set
// BRU_LANG_DIR to a local bruno checkout's `packages/bruno-lang` to override.
const fs = require('fs');
const path = require('path');

const langDir = process.env.BRU_LANG_DIR
  ? path.resolve(process.env.BRU_LANG_DIR)
  : path.join(__dirname, 'node_modules', '@usebruno', 'lang');

const bruToJson = require(path.join(langDir, 'v2', 'src', 'bruToJson.js'));

const paths = JSON.parse(fs.readFileSync(0, 'utf8'));
const out = paths.map((p) => {
  try {
    return { path: p, json: bruToJson(fs.readFileSync(p, 'utf8')) };
  } catch (e) {
    return { path: p, error: String(e.message || e).split('\n')[0] };
  }
});
process.stdout.write(JSON.stringify(out));

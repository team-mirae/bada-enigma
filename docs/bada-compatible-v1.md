# bada-compatible v1

The interface between a **bada-compatible IME engine** and the program that feeds it keys and shows
what it types (the **host**: bada ime's macOS shell and web simulator, superkeyboard, your own).

**v1 is the C ABI of bada ime 0.2.0's engine** (`team-mirae/bada-ime`, `engine/include/bada.h`,
`engine/src/ffi.rs`), written down so other engines can follow it. It is the same as
superkeyboard's "IME contract, version 1" (`team-mirae/superkeyboard`, `docs/ime-contract.md`),
which was written from bada ime 0.1.6; the ABI did not change between 0.1.6 and 0.2.0. (0.2.0's
Caps Lock is its macOS shell's, not the engine's: §7.7.) A host written for bada ime 0.2.0 can load
any bada-compatible v1 engine by changing the symbol prefix.

The words **must**, **should** and **may** are used as in RFC 2119. In this repository,
`src/compat/` implements the engine side and `src/compat/host.rs` a minimal host.

- [1. Versions](#1-versions)
- [2. Shape](#2-shape)
- [3. Functions](#3-functions)
- [4. Memory and threads](#4-memory-and-threads)
- [5. Key events](#5-key-events)
- [6. The Output object](#6-the-output-object)
- [7. What the host does](#7-what-the-host-does)
- [8. Profiles, settings, catalog, dictionary](#8-profiles-settings-catalog-dictionary)
- [9. The manifest](#9-the-manifest)
- [10. Conformance](#10-conformance)
- [11. Where bada ime 0.2.0 is more specific](#11-where-bada-ime-020-is-more-specific)
- [12. State](#12-state)

## 1. Versions

- **v1**: bada ime 0.2.0. This document.
- **v1.1**: v1 and the optional `P_state` (§12). A v1 host needs nothing new; a v1.1 host checks
  for `P_state` as for any optional function.
- A new **required** function, a changed signature, or a changed meaning of a field or function is a
  new major version (v2). New **optional** functions and new Output fields are not: v1 hosts ignore
  them, and v1 engines don't have to provide them.
- Each future bada ime release that changes the ABI names the bada-compatible version it starts.

## 2. Shape

- The engine is a **library with a C ABI**: a shared library (`.dylib`, `.so`, `.dll`) for hosts
  that load it at run time, a static library for a shell that links it (bada's macOS shell links
  `libbada.a`). Any language that exports C functions will do; in Rust, `cdylib` / `staticlib`.
- Every function is named `<prefix>_<name>`. The prefix is the engine's own (`bada`, `shout`).
- Structured values cross as **UTF-8 JSON** in NUL-terminated strings: key events in, Outputs out.
  The only binary arguments are the buffers of `P_data_new`.
- **Event in → output out, no I/O.** The engine doesn't read files, open windows or look at the
  clock to decide what to type. The host loads data, passes keys and host text in, and applies each
  Output to the text field. Everything the engine does is visible in its Outputs.
- **Two modes.** `0` **muhenkan** (無変換, 무변환: no composition; keys type directly or pass to the
  app) and `1` **henkan** (変換, 변환: keys compose into a preedit, which is committed). Any other
  value is henkan. An engine with one mode may treat both alike.

## 3. Functions

`P` is the prefix. `Data` and `Engine` are opaque to the host.

### Required

| Function | C signature | Returns |
|---|---|---|
| `P_new` | `Engine *P_new(const Data *data)` | A new engine on `data`. NULL on failure. `data` is NULL when the engine has no `P_data_new`; an engine with one **may** fail on NULL (bada does) or use defaults |
| `P_free` | `void P_free(Engine *e)` | Frees the engine. NULL: no-op |
| `P_key` | `const char *P_key(Engine *e, const char *event_json)` | Output JSON for one key event (§5, §6) |
| `P_set_mode` | `const char *P_set_mode(Engine *e, uint32_t mode)` | Output JSON. Commits what was staged (in `commit`), then switches |
| `P_flush` | `const char *P_flush(Engine *e)` | Output JSON. Commits what is staged, as when the field loses focus |

### Optional

Hosts look each up (`dlsym`) and do without it when it's missing. Export only what the engine does.

| Function | C signature | Returns |
|---|---|---|
| `P_data_new` | `const Data *P_data_new(const uint8_t *b0, size_t n0, …)` | Shared data from **N buffers** (pointer, length pairs; 0 ≤ N ≤ 16). N and their order are the engine's. `(NULL, 0)` is an empty buffer. NULL on failure |
| `P_data_free` | `void P_data_free(const Data *d)` | Frees the data, after every engine on it is freed |
| `P_set_settings` | `const char *P_set_settings(Engine *e, const char *json)` | **The settings in effect**, as JSON (not an Output). Replaces all settings: missing or unknown fields take defaults; invalid JSON resets to defaults |
| `P_set_profile` | `const char *P_set_profile(Engine *e, const char *name)` | Output JSON. Commits what was staged, switches henkan profile (§8.1) |
| `P_host_selection` | `const char *P_host_selection(Engine *e, const char *text)` | Output JSON. The host's answer to `need_selection` (§7.5) |
| `P_catalog` | `const char *P_catalog(Engine *e)` | JSON describing what the engine offers (§8.3) |
| `P_dictionary_load` | `char *P_dictionary_load(const char *json)` | Process-wide (no engine). Loads a user dictionary; `""` clears it. A JSON summary the **caller frees** with `P_string_free` |
| `P_string_free` | `void P_string_free(char *s)` | Frees a string from `P_dictionary_load` |
| `P_state` | `const char *P_state(Engine *e)` | v1.1. **The engine's state**, as JSON (§12; not an Output). Read-only: changes nothing |

## 4. Memory and threads

- **Returned `const char *`** belong to the engine and stay valid **until the next call on the same
  engine**. The host copies what it keeps.
- **Arguments** are valid **for the call only**; the engine copies what it keeps. `P_data_new` must
  not keep pointers into its buffers.
- **Data** is immutable once built. Several engines may share one `Data` and run **on different
  threads at once**: `Data` must be safe to share.
- **An engine** is used by one thread at a time, and may move between threads.
- `P_dictionary_load` affects every engine of the library.
- Nothing unwinds across the boundary (Rust: catch panics, or `panic = "abort"`).
- A NULL engine pointer may return NULL; hosts never pass one.
- Text is UTF-8. Positions (selection, caret) are counted in Unicode scalar values, never bytes.

## 5. Key events

```json
{"key": "KeyA", "shift": false, "ctrl": false, "alt": false, "meta": false}
```

- `key` is a **W3C `KeyboardEvent.code`**: the key's **physical position**. Missing modifiers are
  false. (`ctrl` ⌃, `alt` ⌥, `meta` ⌘.)
- The keyboard is **JIS** (日本語配列):

| Group | Codes |
|---|---|
| Number row | `Digit1`…`Digit0`, `Minus`, `Equal` (^), `IntlYen` (¥) |
| Letter rows | `KeyA`…`KeyZ`, `BracketLeft` (@), `BracketRight` ([), `Semicolon`, `Quote` (:), `Backslash` (]), `Comma`, `Period`, `Slash`, `IntlRo` (ろ, \\ _) |
| 半角/全角 | `Backquote` |
| Control | `Space`, `Enter`, `Backspace`, `Tab`, `Escape`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Delete`, `Home`, `End` |
| F keys | `F1`…`F12` (bada intercepts `F6`–`F10` in henkan) |
| Language keys | `Lang1` (かな), `Lang2` (英数), `Convert` (変換), `Lang3` (カタカナ). Hosts handle them (§7.4) and send only `Lang1`, in henkan, as "next menu entry" (⇧: previous) |

- The first 48 (number row through `IntlRo`) are the **JIS kana space**, where bada's layouts type.
- An engine must accept **any** string as `key`; a code it doesn't know is a key it doesn't handle:
  commit what's staged if that's its rule, and `consumed: false`. Invalid JSON: likewise.
- Key repeat is not modelled: every event is one press.
- **Caps Lock is the host's** (§7.7). Events carry no Caps Lock state and hosts don't send
  `CapsLock`; an engine that gets it treats it as a key it doesn't know.

## 6. The Output object

Every function marked "Output JSON" returns one. All fields are optional to read (a missing field
is its default); bada ime writes all of them.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `commit` | string | `""` | Text to insert now. Replaces the **marked text** (the old preedit), or the **selection** if nothing is marked or `replace_selection` is set |
| `replace_selection` | bool | false | `commit` replaces the selection even when text is marked |
| `preedit` | string | `""` | The marked (composing) text **after** the commit. `""`: nothing is marked |
| `menu` | string[] | `[]` | The candidate menu's rows, top to bottom. Empty: no menu |
| `selected` | integer | 0 | The highlighted row. For a horizontal menu, past the end: none |
| `zero` | integer | 0 | The row of **entry 0**, the top-ranked reading. Rows above it are **negative entries** (script forms: −1 hiragana, −2 katakana in bada); a row's entry is `row − zero` |
| `consumed` | bool | false | The engine took the key. **false: the app gets the key too**, after the commit |
| `horizontal` | bool | false | The menu is one row of single characters (bada's ⌥ + letter variants) |
| `notes` | string[] | `[]` | Per row: a note beside it (romaji, a reading). Empty when no row has one |
| `japanese` | bool[] | `[]` | Per row: draw in a Japanese font. Empty when no row needs it |
| `need_selection` | string \| null | null | The engine needs the host's selected text to finish (bada: `"hanja"`, `"full"`, `"half"`); the host answers with `P_host_selection` |

Engines may add fields; hosts ignore fields they don't know.

## 7. What the host does

The text field is a list of characters, a **selection** `(start, end)` (equal: a caret) and an
optional **marked range** `(start, length)`. Every Output is applied the same way, whichever
function returned it.

### 7.1 Applying an Output

```
apply(out):
  if out.commit != "":
      range = the marked range if any and not out.replace_selection, else the selection
      replace range with out.commit; caret after it; nothing marked
  if out.preedit != "":
      range = the marked range if any, else the selection
      replace range with out.preedit; mark it; caret after it
  else if something is marked:
      delete the marked text
  show out.menu, out.selected, out.zero
  if out.need_selection and this is not already the answer to one:
      apply(P_host_selection(selected text if nothing is marked, else ""))   # once, never deeper
```

### 7.2 A key press

```
press(event):
  if event is a language key: §7.4, done
  out = P_key(event); apply(out)
  if not out.consumed: the app gets the key (§7.3)
```

So **commit, then act** (Space, Enter, Esc with a preedit) is one Output with `commit` set and
`consumed: false`.

### 7.3 Keys passed on

The app does what it does. A plain text field: ⌘ / ⌃ / ⌥ combinations do nothing (shortcuts,
option characters); a character key types its JIS legend (⇧: the shifted one; with Caps Lock on, in
capitals, ⇧ or not); `Space`, `Tab`,
`Enter` insert `" "`, `"\t"`, `"\n"`; `Backspace` / `Delete` delete the selection or the character
before / after the caret; arrows, `Home`, `End` move the caret; anything else does nothing.

### 7.4 Language keys

The host owns mode switching (on macOS, input sources are the shell's), so these mostly don't reach
`P_key`. After every `P_set_mode`, the host calls `P_set_profile` with the current profile, if the
engine has it.

| Key | Mode now | The host |
|---|---|---|
| `Lang2` (英数) | any | `P_set_mode(0)` |
| ⌃⌥⇧ `Lang1` | any | `P_set_mode` to the other mode |
| `Lang1` (かな) | muhenkan | profile `mac-kana`, `P_set_mode(1)` |
| `Lang1` | henkan, profile `mac-kana`, or a menu of more than one row | `P_key({"key":"Lang1","shift":…})`: next entry (⇧: previous) |
| `Lang1` | henkan, another profile, menu of ≤ 1 row | `P_set_profile("mac-kana")` |
| `Convert` (変換) / `Lang3` (カタカナ) | muhenkan | profile `henkan` / `katakana`, `P_set_mode(1)` |
| `Convert` / `Lang3` | henkan, same profile | `P_key` `Lang1`: next entry |
| `Convert` / `Lang3` | henkan, other profile | `P_set_profile(…)` |

### 7.5 The host's selection

When an Output has `need_selection`, the engine waits for the selected text. The host calls
`P_host_selection` with it (`""` if nothing is selected or something is marked) and applies the
reply, which usually has `commit` with `replace_selection: true`.

### 7.6 Host edits

A click in the text or an edit by the app commits what's staged: the host calls `P_flush` and
applies it before changing the field.

### 7.7 Caps Lock

The engine never sees Caps Lock: a host that handles it does so around `P_key`, as bada ime 0.2.0's
shell does (bada `docs/spec.md` §6.7, §6.2; its web simulator does the same):

- Caps Lock on affects only what the **app** types (§7.3): a passed-on character key is typed in
  capitals. The engine gets the same events, so muhenkan keys it types itself and henkan
  composition are unchanged (henkan ignores Caps Lock).
- A host may make Caps Lock a mode key: bada's default **toggle** switches mode, as ⌃⌥⇧ `Lang1`
  (§7.4), with `P_set_mode`; ⇧ ⌃ ⌥ ⌘ + Caps Lock is then Caps Lock, in muhenkan only. Its other
  settings: **transient** (Caps Lock in muhenkan only, off when muhenkan is left) and **respect**
  (Caps Lock everywhere).
- bada's **UPPER_SNAKE** rule, in muhenkan with Caps Lock on: the host flushes (`P_flush`, applied)
  and types `_` for `Space`, a space for ⌥`Space`, without calling `P_key`.

## 8. Profiles, settings, catalog, dictionary

### 8.1 Profiles

A henkan profile is a named set of henkan settings. bada's names are `mac-kana` (かな), `henkan`
(変換) and `katakana` (カタカナ), the ones hosts use (§7.4). `P_set_profile` commits what's staged
and switches; an unknown name falls back to the default profile.

### 8.2 Settings

The settings object is the engine's own; hosts treat it as opaque JSON, and may store what
`P_set_settings` returns. `P_set_settings("{}")` gives the defaults. bada's main field is
`enabled`, the henkan layouts by id (bada `docs/spec.md` §5.3).

### 8.3 Catalog

Free-form JSON for settings UIs and test planners. bada's is `{"layouts": [{"id", "name",
"henkan"}], "actions": [{"method", "action", "label"}]}`.

### 8.4 Dictionary

`P_dictionary_load` loads a user dictionary for the whole process, in the engine's format (bada:
its `docs/dictionary-contract.md`). The summary is JSON; bada's shape is
`{"ok", "error", "positive", "negative"}`.

## 9. The manifest

Hosts that load engines at run time describe each with a JSON manifest. superkeyboard's
(`docs/ime-contract.md` §8 there): `name`, `description`, `root`, `library` (`{ext}` → `dylib`,
`so`, `dll`), `prefix`, `data` (the buffers for `P_data_new`, in order), `settings`, `mode`,
`keyboard`. This repository's is [imes/shout.json](../imes/shout.json).

## 10. Conformance

- [ ] The five required functions are exported under one prefix.
- [ ] Every Output is valid JSON; unknown keys and invalid events give `consumed: false`, never a crash.
- [ ] `P_set_mode` and `P_set_profile` commit what was staged.
- [ ] `P_flush` leaves nothing staged (`preedit: ""`).
- [ ] With a preedit, a key the engine doesn't use commits it and returns `consumed: false`.
- [ ] Returned strings live until the next call on the same engine; arguments are copied.
- [ ] Engines on one `Data` work concurrently on different threads.
- [ ] `P_set_settings("{}")` gives the defaults, and invalid JSON the same.
- [ ] Nothing panics or unwinds across the boundary.
- [ ] With `P_state`: it changes nothing, and its `staged` is the `commit` `P_flush` would return.

`tests/conformance.rs` checks these through the C ABI.

## 11. Where bada ime 0.2.0 is more specific

Behaviour v1 doesn't require, but a bada-like engine should follow (bada `docs/spec.md`):

- **Henkan keys** (§6.3 there): Space, Enter, Esc and other keys commit what shows, then act; **Esc
  commits**, it doesn't cancel. ⇧Enter commits and does nothing else. Tab / ↓ / かな: next entry;
  ⇧Tab / ↑ / ⇧かな: previous. With nothing staged, keys go to the app. ⌘ / ⌃ shortcuts commit, then
  pass.
- **The menu** (§6.4): typing lands on entry 0; no wrap; committing commits the entry shown.
- **Muhenkan** (§6.2): consume only keys the engine types itself (bada: ⌥ + letter variants,
  半角/全角 on the selection).
- **F6–F10** in henkan are the engine's (§6.5): swallowed unless bound.
- **Caps Lock** (§6.7) is the shell's (§7.7 here). The Keyboard Viewer layouts the Mac app ships
  (`mac/tools/make-keylayout.swift`) have a Caps Lock layer of their own: `qwerty.keylayout`
  (muhenkan) shows ABC's capitals, `bada.keylayout` (henkan) bada's unshifted jamo; their ⌥, ⌘
  and ⌃ layers moved up one index (now 3–6).
- **Data**: `bada_data_new` takes 11 buffers (ko-words, en-words, hanja, oecd, ja-words, kanji,
  ko-jamo LM, symbols, emoji, unicode-names, en-ko), and `bada_new(NULL)` returns NULL.

## 12. State

**v1.1, optional.** `P_state` is how an engine reports itself to its **wrapper** (the host: bada
ime's macOS shell, bada-windows' text service, superkeyboard). It's for **reading**: other programs
that want to know what the IME is doing (Hammerspoon, Karabiner-Elements, AutoHotkey, a status bar)
read it through the wrapper, never through the engine.

```json
{"format": "bada-compatible-state/1", "mode": "henkan", "profile": "mac-kana", "staged": "한국어",
 "preedit": "한국어", "menu": ["한국어", "韓國語"], "selected": 0, "zero": 0, "horizontal": false,
 "need_selection": null}
```

| Field | Type | Meaning |
|---|---|---|
| `format` | string | `"bada-compatible-state/1"`. A reader checks it; a new major format is a new string |
| `mode` | `"muhenkan"` \| `"henkan"` \| null | The mode last set by `P_set_mode`; null before the first |
| `profile` | string \| null | The henkan profile in use (§8.1); null if the engine has none |
| `staged` | string | **What `P_flush` would commit now**: the text as it would be typed, not the keys or tokens behind it. `""`: nothing is staged |
| `preedit`, `menu`, `selected`, `zero`, `horizontal` | | As the last Output left them (§6) |
| `need_selection` | string \| null | The engine is waiting for `P_host_selection` |

- An engine that exports `P_state` **must** keep `staged` exact, in either mode: it is the `commit`
  of a `P_flush` called instead, character for character. It may differ from `preedit`, which is
  only what's drawn: when a flush would commit nothing (bada: a menu open on the host's selection,
  not on staged text), `staged` is `""`, whatever shows.
- `P_state` changes nothing; calling it any number of times between two other calls gives the same
  answer. Its string lives like any other reply (§4).
- Engines may add fields; readers ignore fields they don't know.

### 12.1 What the wrapper does with it

How, and whether, a wrapper exposes the state is the wrapper's. Exposing it is **recommended**: it
is what lets users script around the IME. A wrapper that does:

- **should** call `P_state` after every call that returns an Output, and publish the object as is,
  adding its own fields under a `host` key (for example the app in focus, Caps Lock §7.7, its
  version) so a reader never confuses the two;
- **should** be read-only from outside: reading the state never changes the IME, and nothing a
  reader does is fed back to the engine;
- **should** publish nothing typed while the focused field is a password field (secure input): it
  sets `staged`, `preedit` and `menu` to empty values and `host.secure` to true;
- **should** use a channel the platform's scripting tools can read without a plugin, and document
  it. For example: on macOS a JSON file rewritten atomically plus a distributed notification
  (Hammerspoon: `hs.pathwatcher`, `hs.distributednotifications`; Karabiner: `set_variable` from a
  script); on Windows a JSON file plus a registered window message, or a named pipe (AutoHotkey:
  `FileRead`, `OnMessage`).


# Keyboard shortcuts

This document defines how Strata turns a key press into a shortcut on every
keyboard layout, and how code must read key input. It covers every surface that
handles keys: browser windows, the file chooser, previews, Settings, and dialogs.
What individual keys do is documented with each feature, for example in
[keyboard navigation](keyboard-navigation.md) and [10xer mode](10xer-mode.md).

## How a key press becomes a shortcut

Shortcuts follow the key's label first, then its physical position:

- **Letters** use the letter the active layout prints on the key. On French
  AZERTY, German QWERTZ, or Dvorak, **Ctrl+Z** is the key labelled Z.
- **Non-Latin layouts.** When the key types a character outside printable ASCII
  (Cyrillic, Greek, Hebrew, Arabic, accented letters, and so on), the shortcut
  uses the Latin character that the same physical key types in another
  configured layout, at the same Shift level. If no configured layout has one,
  it uses the key's US-QWERTY character. With a Russian layout active, the key
  that types **с** is **Ctrl+C**, and in 10xer mode the key that types **о**
  is **j**.
- **The active layout's own ASCII character wins** over the fallback. On a
  Russian layout the key that types `.` is **Ctrl+.**; the key that types `ю`
  sits on the US `.` position and also resolves to **Ctrl+.**.
- **AltGr** (third- and fourth-level) characters are text input and never fall
  back.
- **The top digit row is 1–0 by position** on every layout, with or without
  Shift. **Ctrl+1**, **Ctrl+Shift+1** and 10xer digits work on AZERTY, where the
  row types `&é"'(` without Shift, and on layouts whose Shift+digit symbols
  differ from US. A chord's digit step (**t 1**, **g 1**, **; 1**) also accepts
  the Shift AZERTY types digits with. A digit typed elsewhere counts too, such
  as the Hungarian 0 left of 1, and keypad digits count when Num Lock is on.
  With AltGr, a top-row key counts only when it types a digit, as on
  Lithuanian; AltGr symbols there, such as the AZERTY `~` and `@` or the `\` of
  German Mac layouts, stay symbols.
- **`+` and `−` win over a digit position where they have a meaning.** Where
  a top-row key types `+` or `−`, Ctrl shortcuts and **g** read it as that
  symbol: on Swiss German, Ctrl and the `+` key (Shift+1) is **Ctrl++**, not
  tab 1, and on AZERTY **g** then the `−` key unpins. **t** and **;** have no
  `+` or `−` step, so they count the key's position: on AZERTY **t** then the
  `−` key selects tab 6, and on Czech **t** then the `+` key selects tab 1.
- **Keypad keys act as the main keys** they share a label with: keypad Enter,
  arrows, Home, End, Page Up, Page Down, Insert, Delete, Tab, Space, `+`, `−`,
  `*`, `/` and `=`.
- **Caps Lock and Num Lock never change a shortcut.**
- **Shift** is part of a letter shortcut: **Ctrl+Shift+K** is not **Ctrl+K**.
  For symbols, the Shift needed to type the symbol is ignored, so **Ctrl++**
  matches whether or not the layout needs Shift for `+`, and **Ctrl+.** works on
  AZERTY, where `.` needs Shift.
- **Modifiers match exactly** over Ctrl, Alt, Shift and Super. **Ctrl+C** does
  not fire for **Ctrl+Alt+C**. Navigation and dismiss keys (Escape, Enter,
  arrows, Tab, Home, End, Page Up, Page Down, Delete, Backspace, function keys)
  may also accept other held modifiers where a surface allows them; **Escape**
  closes a dialog whatever is held. Letter and symbol shortcuts never do. The one
  digit that does is **0** in media, which accepts the Shift AZERTY needs to
  type it.
- **Typing is unaffected.** Type-to-search, filter fields, rename fields, and
  footer prompts receive the character the layout produced.
- **Text fields** follow the same rules for copy, cut, paste, select all, undo,
  and redo.
- **Input methods** for Korean, Japanese, and Chinese run on a Latin layout and
  pass Ctrl shortcuts through, so the same rules apply without special cases.
  While one is composing, it sees every key first; see
  [Input methods and Chinese layouts](#input-methods-and-chinese-layouts).

## Input methods and Chinese layouts

- **Chinese keyboards use the US layout.** The XKB `cn` layout is US, so every
  shortcut is the US one. Chinese is typed with an input method, such as fcitx5
  or IBus, running on that layout: pinyin for Simplified Chinese, and Zhuyin
  (Bopomofo), Cangjie or Quick for Traditional Chinese.
- **An unfinished composition sees each key first.** While the focused text
  field shows an input method's unfinished text (the preedit, such as pinyin
  waiting for a candidate), each key goes to the input method before any Strata
  command. A key it uses never reaches Strata: Space and digits pick a
  candidate, Enter commits the typed letters, Escape cancels, and arrows, Tab
  and Backspace work in the composition. So Escape does not cancel a rename or
  close the filter, the location bar, a footer prompt, global search, or the
  F1 reference, and Space does not open quick preview from the filter. A key
  the input method passes back works as usual after it: Korean input methods
  such as fcitx5-hangul commit the current syllable and pass back Escape,
  arrows and Ctrl shortcuts, so one press both commits and runs the command.
  Keys the text field handles itself go to the field: Enter submits it, and
  cursor keys move in it. On Wayland the input method runs in the compositor
  and Strata never receives the keys it uses. Input methods that GTK loads into
  Strata (`GTK_IM_MODULE=fcitx` or `ibus`, and every input method on X11),
  GTK's own dead keys, and the Compose key would otherwise get each key after
  Strata's handlers.
- **Input method switch keys reach the focused text field.** **Ctrl+Space**
  (fcitx5), **Super+Space** (IBus), **Shift** and **Shift+Space** go to the input
  method in every text field, the F1 search included. Strata's own
  **Ctrl+Space** toggles selection only in a file list.
- **Zhuyin starts syllables with digit and punctuation keys.** Zhuyin input
  methods, such as chewing, type initials and finals on the digit row and on
  `-` `,` `.` `/` `;`, and tones with 3, 4, 6, 7 and Space. Array30 and
  Boshiamy codes use punctuation keys too. While a text field has focus, no
  Strata command takes a plain digit or punctuation key, so the first key of a
  syllable reaches the input method and the composition then sees the rest. New
  bindings on these keys must keep it that way.
- **Hanyu Pinyin with AltGr dead keys** (`cn(altgr-pinyin)`): AltGr+1 to
  AltGr+4 are the tone dead keys that type ā á ǎ à, AltGr+6 and Shift+AltGr+6
  are the ^ and ¨ dead keys, and AltGr+v types ü. These are text input, never
  digits: **Ctrl+1** and **t 1** use the digit keys without AltGr, as on US.
- **Taiwanese** (`tw`) is US without AltGr. AltGr types characters for
  Taiwanese Hokkien and Formosan languages, including combining marks and dead
  keys on the digit row, which are never digits. The `indigenous` and
  `saisiyat` variants type ʼ on the apostrophe key, and `saisiyat` types ː on the
  semicolon key, which the layout fallback still reads as `;` for the 10xer
  **;** chord.
- **Type-to-search starts without the input method.** An input method composes
  only in a focused text field. A key typed in the file list starts
  type-to-search with the character the layout produced, so a pinyin letter
  arrives as that Latin letter, a Zhuyin key as its digit or punctuation, and a
  dead key starts nothing. Typing in the Open With list starts its search the
  same way. To search with an input method, open the filter with **Ctrl+F**
  (**f** in 10xer mode), or move into the Open With search field, and type
  there.

## Shortcut labels

Every place that shows a shortcut renders it from the binding the app matches:
the F1 reference, Settings, context menus, and footer hints. A displayed
shortcut is therefore always one that works.

- **Labels show the Latin key**: **Ctrl+C** with a Russian layout active,
  **Ctrl+1** on AZERTY. Keyboards for non-Latin layouts print the Latin letters
  too, and AZERTY keyboards print the digits, so the label is on the key to
  press. Labels do not change when the active layout changes.
- **Key names follow the interface language**, the way keyboards sold for it are
  printed: German shows **Strg+C** and **Entf**, French **Suppr**, Spanish
  **Supr**. Languages whose keyboards print English names keep them.
- **One notation everywhere.** Modifiers appear in the order Ctrl, Alt, Shift,
  Super. Arrows are ↑ ↓ ← →, and other keys use their full names (Enter,
  Escape, Delete, Page Up). After a modifier, letters are uppercase and Shift is
  spelled out (**Ctrl+Shift+N**). Without a modifier, a letter appears as typed
  (**j**) and Shift+letter as the uppercase letter (**J**), the 10xer
  convention. Chords are separated by a space (**g h**).

## Reading key input in code

### One entry point

`ui::keys` (`src/ui/keys.rs`) is the only module that reads GDK key events.
Install handlers with `keys::on_press(&controller, |controller, press| …)`,
which returns a `Propagation`, and `keys::on_release(&controller,
|controller, press| …)`. Each event is normalized once into a `KeyPress` and
passed to the handler.

While the focused text field is composing with an input method, `on_press`
first forwards the key to that field, as GtkSearchEntry's key capture does,
and calls the handler only if the field and its input method leave the key
unhandled; the release of a key they took is forwarded too (see
[Input methods and Chinese layouts](#input-methods-and-chinese-layouts)). A
field that submits on Enter must also do so from its `activate` signal, which
is how Enter arrives after an input method passes it back.
`ui::keys` follows the preedit of each window's focused `gtk::Text` or
`gtk::TextView` through `preedit-changed`, so handlers need no check of their
own. Release handlers still run.

`clippy.toml` lists `EventControllerKey::connect_key_pressed`,
`EventControllerKey::connect_key_released`, and
`GtkApplicationExt::set_accels_for_action` under `disallowed-methods`. `ui::keys` calls them under a single
`#[expect(clippy::disallowed_methods, reason = "…")]`; no other module may.

### `KeyPress`

- `is(binding)` reports whether the press matches a `Binding`. Use it for
  every command, including navigation keys such as Escape, Return, and arrows.
  `is_any(bindings)` tries several.
- `digit_in(digits)` returns the digit-row or keypad digit of a catalog digit
  range, such as tab numbers or a chord's digit step. Single digit bindings
  (**Ctrl+1**) go through `is()`.
- `text()` returns the typed character, for typing consumers only.
- `is_modifier()` reports a bare modifier key press, and `modifier_key()` which
  modifier it is.
- `modifiers()` returns the held Ctrl, Alt, Shift and Super, for policies that
  depend on what is held rather than on one binding.
- `used_fallback()` reports a key recognized only through the layout fallback,
  which GTK's own bindings miss.
- `read_as(binding)` reads a press as another key, for keys that stand in for
  others, such as **h j k l** for arrows.

`keys::read_event(&event)` reads a key event that reached another controller,
such as `EventControllerLegacy`.

### `Binding` and the binding catalog

A `Binding` is a canonical key plus exact modifiers: `Binding::plain(Key::j)`,
`Binding::ctrl(Key::c)`, `Binding::ctrl_shift(Key::k)`, or
`Binding::new(key, modifiers)`. Letters are lowercase with Shift explicit.
Symbol bindings name the symbol (`Key::plus`), never the key that types it on a
US keyboard. `binding.ignoring(modifiers)` lets a navigation or dismiss key also
match while those modifiers are held; its label does not change.

Every shortcut is a named constant in the binding catalog,
`ui::keys::bindings`, with one file per area under `src/ui/keys/bindings/`. The
`catalog!` macro declares each area's constants, such as
`COPY: Binding = Binding::ctrl(Key::c);`, in a block that says which F1 maps
list them: `listed` (both), `default_only`, `tenxer_only`, or `unlisted`. A
chord's second key names its first with `, after GO`. Handlers match catalog
constants, and the F1 reference (`ui/shortcut_reference.rs`), context-menu
hints, and footer hints refer to the same constants. Dispatch logic stays in the
handlers; the catalog only defines keys.

What a reference row or hint shows is a `Keys` value, never free text:

- one binding: **Ctrl+C**
- alternatives: **F2 / Ctrl+R**
- a chord: **g h**
- a digit range with its modifiers or chord prefix: **Ctrl+Shift+1–9 / 0**,
  **t 1–9 / t 0**
- alternatives of different shapes: **g g / Home**

### Labels

`keys::label` is the only code that turns bindings into text; `label_in`
renders a given language, `modifiers_label` held modifiers alone (**hold
Ctrl+Shift**), and `digit_label` one member of a digit range. Key names come from
one translated table (`keys.name.*` in every catalog). Context templates such as
`%{keys} in header` stay translatable and receive the formatted label;
`shortcut_reference::with_keys` fills one, as tooltips and accessible names do.

Alternatives that share their modifiers write them once (**Ctrl+Alt+← / →**)
unless a key is a letter (**Ctrl+U / Ctrl+D**). Minus is shown as **−**.

F1 searches `keys::search_text`: the label in the interface language and in
English, with each alternative also spelled out in full, so **strg+0** and
**ctrl+0** both find **Strg++ / − / 0**. A typed `-` finds the **−** in labels
and the **–** of digit ranges, so **1-9** finds **Ctrl+Shift+1–9 / 0**.

Menus display the formatter's text too. GTK formats a menu item's `accel`
attribute itself: it puts Shift first, capitalizes letters, and uses its own
translations. So menu rows must not show GTK's rendering of an `accel` string,
and no code may build an accelerator string by parsing a label. A native menu
row carries its binding's `Binding::accelerator()`, which keeps GTK's
accessible shortcut, and `present_native_items` replaces GTK's rendering with the
formatter's label.

### Normalization

1. Start from `gdk::KeyEvent::match_()`, GTK's accelerator form: Caps Lock is
   ignored, modifiers consumed to produce the key are removed, and letters are
   lowercased with Shift added back. Keep only Ctrl, Alt, Shift and Super.
2. If that key is a character outside printable ASCII and the event level is 0
   or 1, replace it with the first printable-ASCII key that
   `Display::map_keycode` reports for another layout at the level Shift
   selects, then canonicalize it the same way. Otherwise use the built-in
   US-QWERTY table entry for the keycode. The level comes from Shift, not from
   the event, because Caps Lock raises the event's level for letters. GDK
   keycodes are evdev codes plus 8 on both X11 and Wayland. If neither source
   has a Latin key, keep the original.
3. At level 0 or 1, digit-row keycodes 10–19 map to 1–9 and 0 regardless of
   the key they type. At a higher level (AltGr, or a Caps Lock level some
   layouts add) a digit-row key counts only as the digit it types, because GDK
   reports no AltGr modifier. Another key counts as the digit it types at level
   0 or 1. Digit matching
   compares the event's modifier state before consumed modifiers are removed,
   so **Ctrl+1** and **Ctrl+Shift+1** stay distinct on every layout.
4. Keypad keys become the main keys they share a label with. Keypad digits stay
   keypad keys and count as digits when Num Lock is on.

The common path costs one `match_()` call and a range check. `map_keycode`
runs only for keys outside ASCII, and nothing is cached, so no keymap-change
listeners are needed.

### Built on GTK

The hard parts come from GDK, which Strata already uses. `ui::keys` adds only
what GTK lacks.

- `KeyEvent::match_()` handles Caps Lock, consumed modifiers, and case folding.
  Do not reimplement them.
- `Display::map_keycode()` supplies the other layouts' characters for the
  fallback.
- `gtk::accelerator_name()` builds a menu item's `accel` string from a binding.

GTK lacks three things: a fallback when no configured layout is Latin,
position-based digits (its own matching misses **Ctrl+1** on AZERTY), and a
single answer per event. `KeyEvent::matches()` is not called per binding:
Strata's dispatcher tries bindings in order, so a physical-key match checked
first could beat an exact match checked later, and each non-exact check looks
up the keymap again.

Labels do not use `gtk::accelerator_get_label()` or `GtkShortcutLabel`
(deprecated since GTK 4.18). GTK puts Shift first, capitalizes plain letters so
**j** and **J** look the same, spells out arrows and symbols, leaves some names
untranslated, and translates with the system locale rather than Strata's
language setting. Its translations are still a useful reference when filling in
the key-name table.

No third-party crate is needed. `keycode` or `xkbcommon` could stand in for the
US-QWERTY table, but that table is about 50 static entries. `keyboard-types`
and `xkeysym` duplicate types GDK already provides.

### Window actions

Search (**Ctrl+K**), jump to folder (**Ctrl+Shift+K**), open terminal
(**Ctrl+Alt+T**), refresh (**F5**), and arrow scope (**Ctrl+\\**) are bindings in
the window dispatcher, not GTK application accelerators. The dispatcher checks
them after its own stages yield the key and activates the same guarded `win.*`
actions; the key is consumed whenever the action is enabled.
`add_guarded_window_action` still decides whether they run while a modal or an
inline edit owns input. 10xer mode leaves jump to folder, open terminal, and
arrow scope unbound, which the dispatcher checks when the key arrives.

### Text fields

When a `gtk::Text` or `gtk::TextView` has focus and a press matched an editing
binding only through the layout fallback, the dispatcher activates the widget's
own action (`clipboard.cut`, `clipboard.copy`, `clipboard.paste`,
`selection.select-all`, `text.undo`, `text.redo`) and stops the event. Otherwise
GTK's own text bindings handle it. The file chooser applies the same rule
(`edit_focused_text`). `keys::TEXT_EDITING` holds these bindings and their
actions, and the open F1 reference lets the same bindings reach its search
field.

Plain keys that only the fallback recognizes stay text where typing goes:
type-to-search starts with the **þ** that Icelandic types on the US `/` key
instead of opening an empty filter, and in 10xer mode the Russian **Ё** key
reads as **~** but types **Ё** into the F1 search instead of closing the
reference. In 10xer mode, text fields such as the location bar type **~**
itself rather than opening the reference; only the open reference's search
closes it on **~**.

A focused cell of a document table follows the same rule for copy and select
all. The table's key handler runs the cell's action, because it owns the
selection that spans cells: **Ctrl+C** copies that selection when there is one,
and **Ctrl+A** clears it first.

## Adding or changing a shortcut

1. Add the binding to `ui::keys::bindings`, in the catalog block for the F1
   maps that list it, and match it in the handler with `press.is(…)` or
   `press.digit_in(…)`.
2. Reference the same constant from its F1 row in `ui/shortcut_reference.rs`
   and from any context hint. The label renders itself; translate only the
   action text, in every catalog (see
   [internationalization](internationalization.md)).
3. Never compare `text()` or a raw keyval to recognize a command, never type a
   key label by hand, and never add a second spelling (an uppercase letter or a
   US shifted symbol) to cover another layout.
4. Extend the normalizer table tests when a binding uses a symbol or digit, and
   add QA scenario coverage for a new input route.

## Testing

Rust tests emit `key-pressed` without a GDK event. `KeyPress::from_signal`
reads such a press as a US-QWERTY press, with Caps Lock matched the way GDK
matches it, so handler tests stay layout-neutral and the layout cases live in
the normalizer tests and QA scenarios.

`ui::keys::tests` drives the pure normalizer from hand-written keymap rows for
US, Russian, Ukrainian, Greek, Hebrew, French AZERTY, German QWERTZ, Hungarian,
Lithuanian, Dvorak, Caps Lock, and a Russian-only keymap, plus a Hanyu Pinyin
(`cn(altgr-pinyin)`) dead tone key recorded from GDK. Plain `cn` reports exactly
the US keymap, so the US rows cover it. A GTK test delivers real key events to
check that key handlers yield to a composition in the focused field and still get
the keys its input method passes back. The tests also cover label rendering
(notation, translated key names, chords, digit ranges) and catalog completeness:
every catalog binding has a row of its own in each F1 map its catalog block
names (`listed` for both, `default_only`, `tenxer_only`), a chord's second key
right after its first, or is explicitly marked as unlisted. Run them with
`./scripts/test-headless.py keys::`, and run `./scripts/test-headless.py i18n::`
after changing key names.

QA scenarios switch layouts on the run's private Xvfb with a `layout` step:
`setxkbmap` loads the layouts and an XKB group lock selects the active one. Never change the
desktop's layout from a test. Scenarios use US, Russian, Russian-only, French
AZERTY, German QWERTZ, Caps Lock, and Hanyu Pinyin. With Hanyu Pinyin, GTK's
own input method composes the AltGr tone keys in text fields, which stands in
for a pinyin input method's composition. See
[testing](testing.md).

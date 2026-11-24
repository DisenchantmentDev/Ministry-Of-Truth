# Ministry of Truth

Ministry of Truth is a desktop notebook for tracking lore during Minecraft roleplay events. It keeps notes on nations, players, and events in one project, links them to each other, and places them on a timeline.

The app is written in Rust with [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/master/crates/eframe).

## What it is

Roleplay events produce a lot of connected information. A single war can involve several nations, a dozen players, and a chain of events over many in-game days. Keeping this in a set of loose documents makes it hard to see how things relate.

Ministry of Truth stores each piece of information as a page. Pages hold structured fields (for example, which nation a player belongs to or what day an event happened) and free-form notes in Markdown. The app uses these fields to build relations, lists, and a timeline.

## Current features

### Projects

- A project is a folder. Creating a project writes a `project.json` file with the project name and an optional Day 0 date.
- Pages are plain Markdown files with a YAML header. The app reads and writes these files directly, so a project can be copied, backed up, or put under version control.
- The project tree on the left shows folders and pages. Folders and pages can be created, renamed, and deleted.
- The app warns about pages that share a title.

### Page types

| Type   | Structured fields                                                        |
|--------|--------------------------------------------------------------------------|
| Note   | Title and Markdown body only                                             |
| Nation | Leaders, location, founded day, and relations to other nations           |
| Player | Nation, Discord name, and death day                                      |
| Event  | Day, event type, location, nations, and participants                     |

Relations between nations use one of four stances: Allied, Hostile, Neutral, or Vassal.

Event types are Assassination, War, Battle, Diplomacy, and Other.

Templates are available for blank notes, assassinations, wars, players, and nations. Plain events are created from a nation's Events tab.

A nation page has three tabs:

- **Notes** holds the Markdown body.
- **Events** lists every event that names the nation, and creates a new linked event.
- **Relations** edits the stance toward other nations.

### Links

- Wiki links use the form `[[Title]]`. A link resolves by page title or by file name.
- Clicking a resolved link opens the page. Unresolved links are marked "(missing)" in the preview, and a button creates a note with that title.
- Renaming a page updates the wiki links, leader lists, nation fields, and event fields that refer to the old title in other pages.

### Editing

- Each page can be viewed in four modes: Edit (source only), Preview (rendered Markdown), Split (source and preview side by side), and Live (styled source editing).
- Pages can open in floating windows that can be moved and resized. This lets two pages stay visible at once.
- Save with `Ctrl+S` or from the File menu. Switching pages and closing a project also save the open page.

### Timeline

- The timeline lists events, nation foundings on or after Day 0, and recorded player deaths.
- Entries are grouped by day. If a Day 0 date is set, each day shows its calendar date.
- Events without a day appear at the end under "Undated".
- Nations founded before Day 0 are left off the timeline.

### Platforms

- The native desktop app is the main target. It handles folder selection and file access.
- A web build is set up with Trunk. The web build can display the interface, but it cannot open or create project folders.

## Planned features

These features are not yet implemented.

### Minimaps and border drawing

- **Minimaps** would show a map of the game world, with nations, events, and locations placed on it.
- **Manual border drawing** would let a user draw territory borders on the map by hand. Borders would be linked to nation pages, so a nation's territory can be read from its page and the map together.

### Ticket import and tracking

- **Ticket import** would bring in requests, reports, or issues from an external source, so they do not need to be entered by hand.
- **Ticket tracking** would keep a list of open items and their status, and link each item to the nations, players, or events it concerns.

## Project layout

A new project has this structure:

```text
my-project/
├── project.json
├── index.md
├── nations/
│   └── index.md
├── players/
│   └── index.md
└── events/
    └── index.md
```

Nation, player, and event pages are filed in `nations/`, `players/`, and `events/`. Notes are filed in the folder selected in the tree.

## Building and running

Requirements:

- Rust 1.92 or newer.
- On Debian or Ubuntu, the packages needed by eframe:

  ```sh
  sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
  ```

Run the desktop app:

```sh
cargo run --release
```

Run the tests:

```sh
cargo test
```

### Web build

The web build uses [Trunk](https://trunkrs.dev/) and the `wasm32-unknown-unknown` target.

1. Install the target with `rustup target add wasm32-unknown-unknown`.
2. Install Trunk with `cargo install --locked trunk`.
3. Run `trunk serve` and open `http://127.0.0.1:8080`.

Run `trunk build --release` to produce a static site in `dist`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
